//! winget CLI executor (Issue #4): discovery, argv builders, streaming progress, cancellation.
//!
//! Pure parsing lives in `winget_parse`; argv builders below are pure too. Only the `WingetCli`
//! impl touches processes. Design source: `.claude/skills/winget-executor`.

use std::collections::HashMap;
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use tokio::io::{AsyncRead, AsyncReadExt};
use tokio::process::{Child, Command};
use tokio::sync::{mpsc, oneshot, Notify};

use super::winget_parse::{
    map_exit, parse_table, ExitOutcome, LineSplitter, ProgressParser, WingetVersion,
    EXIT_NO_APPLICATIONS_FOUND, EXIT_UPDATE_NOT_APPLICABLE, MIN_VERSION,
};
use super::{
    ExecFuture, InstallRequest, InstalledPackage, OpGuard, OpPermit, OperationHandle,
    PackageExecutor,
};
use crate::db::repo::operation_log;
use crate::db::DbPool;
use crate::domain::{
    now_unix, LogLevel, Operation, OperationLogEntry, OperationStage, PackageId, ProgressEvent,
    SourceKind, UpdateAvailable,
};
use crate::error::{AppError, AppResult};

/// Env var that overrides discovery (tests and power users).
pub const PATH_OVERRIDE_ENV: &str = "STORE_WINGET_PATH";

const CAPTURE_TIMEOUT: Duration = Duration::from_secs(120);
const PROBE_TIMEOUT: Duration = Duration::from_secs(15);
const DRAIN_TIMEOUT: Duration = Duration::from_secs(2);
const STDERR_CAP: usize = 16 * 1024;
const EVENT_CHANNEL: usize = 256;

// ---------------------------------------------------------------------------------------------
// Discovery
// ---------------------------------------------------------------------------------------------

/// `Path::exists` is false for WindowsApps execution aliases (reparse points); metadata of
/// the link itself is enough to know the file is there.
fn exists_lenient(p: &Path) -> bool {
    std::fs::symlink_metadata(p).is_ok()
}

/// Steps 1-3 of the discovery order, with the environment injected so it is testable.
/// A set-but-wrong override is final: it does not fall through to PATH.
pub fn locate_with(
    override_path: Option<OsString>,
    path_var: Option<OsString>,
    local_app_data: Option<OsString>,
) -> Option<PathBuf> {
    if let Some(p) = override_path.filter(|p| !p.is_empty()) {
        let p = PathBuf::from(p);
        return exists_lenient(&p).then_some(p);
    }
    if let Some(var) = path_var {
        for dir in std::env::split_paths(&var) {
            let cand = dir.join("winget.exe");
            if exists_lenient(&cand) {
                return Some(cand);
            }
        }
    }
    let cand = PathBuf::from(local_app_data?)
        .join("Microsoft")
        .join("WindowsApps")
        .join("winget.exe");
    exists_lenient(&cand).then_some(cand)
}

/// Full discovery: override, PATH, WindowsApps alias, then the App Installer package location.
pub async fn discover() -> AppResult<PathBuf> {
    if let Some(p) = locate_with(
        std::env::var_os(PATH_OVERRIDE_ENV),
        std::env::var_os("PATH"),
        std::env::var_os("LOCALAPPDATA"),
    ) {
        return Ok(p);
    }
    if std::env::var_os(PATH_OVERRIDE_ENV).is_some_and(|v| !v.is_empty()) {
        return Err(AppError::WingetNotFound);
    }
    appx_location().await.ok_or(AppError::WingetNotFound)
}

async fn appx_location() -> Option<PathBuf> {
    let mut cmd = Command::new("powershell");
    cmd.args([
        "-NoProfile",
        "-NonInteractive",
        "-Command",
        "(Get-AppxPackage Microsoft.DesktopAppInstaller | Select-Object -First 1).InstallLocation",
    ])
    .stdin(Stdio::null())
    .kill_on_drop(true);
    no_window(&mut cmd);
    let out = tokio::time::timeout(PROBE_TIMEOUT, cmd.output())
        .await
        .ok()?
        .ok()?;
    let dir = String::from_utf8_lossy(&out.stdout).trim().to_string();
    if dir.is_empty() {
        return None;
    }
    let cand = PathBuf::from(dir).join("winget.exe");
    exists_lenient(&cand).then_some(cand)
}

#[allow(unused_variables)]
fn no_window(cmd: &mut Command) {
    #[cfg(windows)]
    cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
}

// ---------------------------------------------------------------------------------------------
// Argv builders (pure)
// ---------------------------------------------------------------------------------------------

/// Flags winget must never receive from this app.
pub const FORBIDDEN_FLAGS: [&str; 2] = ["--force", "--ignore-security-hash"];

fn s(v: &[&str]) -> Vec<String> {
    v.iter().map(|x| x.to_string()).collect()
}

/// Splits a `PackageId` into the winget id and the `--source` name.
pub fn winget_target(package: &PackageId) -> AppResult<(String, &'static str)> {
    let (kind, id) = PackageId::parse(package.as_str())?;
    let source = match kind {
        SourceKind::WingetCommunity => "winget",
        SourceKind::MsStore => "msstore",
        other => {
            return Err(AppError::Unsupported(format!(
                "winget cannot handle source {other}"
            )))
        }
    };
    Ok((id, source))
}

pub fn install_args(
    id: &str,
    source: &str,
    req: &InstallRequest,
    accept_agreements: bool,
) -> Vec<String> {
    let mut a = s(&["install", "--id", id, "--exact", "--source", source]);
    if let Some(v) = &req.version {
        a.extend(s(&["--version", v]));
    }
    if let Some(scope) = req.scope {
        a.extend(s(&["--scope", scope.as_str()]));
    }
    a.extend(s(&[
        "--silent",
        "--disable-interactivity",
        "--verbose-logs",
    ]));
    if accept_agreements {
        a.extend(s(&[
            "--accept-source-agreements",
            "--accept-package-agreements",
        ]));
    }
    a
}

pub fn update_args(id: &str, source: &str, accept_agreements: bool) -> Vec<String> {
    let mut a = s(&["upgrade", "--id", id, "--exact", "--source", source]);
    a.extend(s(&[
        "--silent",
        "--disable-interactivity",
        "--verbose-logs",
    ]));
    if accept_agreements {
        a.extend(s(&[
            "--accept-source-agreements",
            "--accept-package-agreements",
        ]));
    }
    a
}

pub fn remove_args(id: &str, source: &str, accept_agreements: bool) -> Vec<String> {
    let mut a = s(&["uninstall", "--id", id, "--exact", "--source", source]);
    a.extend(s(&["--silent", "--disable-interactivity"]));
    if accept_agreements {
        a.push("--accept-source-agreements".into());
    }
    a
}

pub fn list_installed_args(accept_agreements: bool) -> Vec<String> {
    let mut a = s(&["list", "--disable-interactivity"]);
    if accept_agreements {
        a.push("--accept-source-agreements".into());
    }
    a
}

pub fn list_updates_args(accept_agreements: bool) -> Vec<String> {
    let mut a = s(&["upgrade", "--include-unknown", "--disable-interactivity"]);
    if accept_agreements {
        a.push("--accept-source-agreements".into());
    }
    a
}

pub fn pin_args(id: &str, add: bool, accept_agreements: bool) -> Vec<String> {
    let mut a = s(&[
        "pin",
        if add { "add" } else { "remove" },
        "--id",
        id,
        "--exact",
    ]);
    if accept_agreements {
        // Supported by `pin add` and `pin remove` (checked with --help on winget 1.29).
        a.push("--accept-source-agreements".into());
    }
    a
}

/// Last line of defence: no code path may hand these flags to winget.
pub fn guard_args(args: &[String]) -> AppResult<()> {
    match args.iter().find(|a| FORBIDDEN_FLAGS.contains(&a.as_str())) {
        Some(bad) => Err(AppError::InvalidConfig(format!(
            "refusing to pass {bad} to winget"
        ))),
        None => Ok(()),
    }
}

// ---------------------------------------------------------------------------------------------
// Process tree control
// ---------------------------------------------------------------------------------------------

#[cfg(windows)]
mod job {
    use std::ffi::c_void;
    use std::mem::size_of;

    use tokio::process::Child;
    use windows::core::PCWSTR;
    use windows::Win32::Foundation::{CloseHandle, HANDLE};
    use windows::Win32::System::JobObjects::{
        AssignProcessToJobObject, CreateJobObjectW, JobObjectExtendedLimitInformation,
        SetInformationJobObject, TerminateJobObject, JOBOBJECT_BASIC_LIMIT_INFORMATION,
        JOBOBJECT_EXTENDED_LIMIT_INFORMATION, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
    };

    /// Job Object that kills the whole process tree when closed (or when the app dies).
    pub struct ProcessJob(HANDLE);

    // SAFETY: a job handle is a kernel object handle; it may be used from any thread.
    unsafe impl Send for ProcessJob {}
    unsafe impl Sync for ProcessJob {}

    fn set_flags(job: HANDLE, flags: u32) -> bool {
        let info = JOBOBJECT_EXTENDED_LIMIT_INFORMATION {
            BasicLimitInformation: JOBOBJECT_BASIC_LIMIT_INFORMATION {
                LimitFlags: windows::Win32::System::JobObjects::JOB_OBJECT_LIMIT(flags),
                ..Default::default()
            },
            ..Default::default()
        };
        // SAFETY: `info` is a valid, correctly sized struct for this information class.
        unsafe {
            SetInformationJobObject(
                job,
                JobObjectExtendedLimitInformation,
                &info as *const _ as *const c_void,
                size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
            )
            .is_ok()
        }
    }

    impl ProcessJob {
        /// Known limitation: the job is assigned after `spawn`, so a grandchild started in that
        /// window is not in the job (CREATE_SUSPENDED is deliberately not used).
        ///
        /// `None` when the job cannot be created or the process cannot be assigned
        /// (callers fall back to `taskkill /T /F`).
        pub fn attach(child: &Child) -> Option<ProcessJob> {
            let process = HANDLE(child.raw_handle()?);
            // SAFETY: plain Win32 calls with valid arguments; the handle is closed on failure.
            unsafe {
                let job = CreateJobObjectW(None, PCWSTR::null()).ok()?;
                if !set_flags(job, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE.0)
                    || AssignProcessToJobObject(job, process).is_err()
                {
                    let _ = CloseHandle(job);
                    return None;
                }
                Some(ProcessJob(job))
            }
        }

        pub fn terminate(&self) -> bool {
            // SAFETY: `self.0` is a live job handle owned by this struct.
            unsafe { TerminateJobObject(self.0, 1).is_ok() }
        }

        /// Drops the kill-on-close flag so a finished operation never reaps survivors.
        pub fn disarm(&self) {
            set_flags(self.0, 0);
        }
    }

    impl Drop for ProcessJob {
        fn drop(&mut self) {
            // SAFETY: the handle is owned and closed exactly once.
            unsafe {
                let _ = CloseHandle(self.0);
            }
        }
    }
}

#[cfg(not(windows))]
mod job {
    use tokio::process::Child;

    pub struct ProcessJob;

    impl ProcessJob {
        pub fn attach(_child: &Child) -> Option<ProcessJob> {
            None
        }
        pub fn terminate(&self) -> bool {
            false
        }
        pub fn disarm(&self) {}
    }
}

use job::ProcessJob;

/// True when the process tree is known to be gone: the Job Object or `taskkill /T /F` worked.
/// A bare `start_kill` (direct child only) does not count: an elevated installer would keep
/// running and we must not claim Cancelled (skill rule 6).
fn tree_killed(job_ok: bool, taskkill_ok: bool) -> bool {
    job_ok || taskkill_ok
}

/// Kills the child's process tree. Job Object first, `taskkill /T /F` as fallback.
/// Returns false when the tree could not be killed (for example an elevated child).
async fn kill_tree(child: &mut Child, job: Option<&ProcessJob>) -> bool {
    let job_ok = job.is_some_and(ProcessJob::terminate);
    #[allow(unused_mut)]
    let mut taskkill_ok = false;
    #[cfg(windows)]
    if !job_ok {
        if let Some(pid) = child.id() {
            let mut cmd = Command::new("taskkill");
            cmd.args(["/PID", &pid.to_string(), "/T", "/F"])
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null());
            no_window(&mut cmd);
            taskkill_ok = cmd.status().await.is_ok_and(|st| st.success());
        }
    }
    if tree_killed(job_ok, taskkill_ok) {
        return true;
    }
    // Last resort: kill the direct child only. This is NOT a successful cancel.
    if child.start_kill().is_ok() {
        tracing::warn!("only the direct winget process could be killed; its children may live on");
    }
    false
}

/// A cancel only counts if the child did not finish with exit 0 first ("Done wins").
fn cancel_wins(status: &std::io::Result<std::process::ExitStatus>) -> bool {
    !matches!(status, Ok(st) if st.code() == Some(0))
}

// ---------------------------------------------------------------------------------------------
// Executor
// ---------------------------------------------------------------------------------------------

#[derive(Debug, Clone, Default)]
pub struct WingetOptions {
    /// Mirrors `AppConfig.accept_source_agreements`. Default false: agreements are surfaced.
    pub accept_agreements: bool,
    /// When set, operations write `operation_log` rows.
    pub log_pool: Option<DbPool>,
    /// Extra environment for each spawned child (used by tests; never process-global).
    pub env: Vec<(String, String)>,
}

struct Inner {
    path: PathBuf,
    options: WingetOptions,
    guard: OpGuard,
    cancels: Mutex<HashMap<String, Arc<Notify>>>,
    next_op: AtomicU64,
}

#[derive(Clone)]
pub struct WingetCli {
    inner: Arc<Inner>,
}

struct OpCtx {
    inner: Arc<Inner>,
    op_id: String,
    package: PackageId,
    winget_id: String,
    operation: Operation,
    tx: mpsc::Sender<ProgressEvent>,
    notify: Arc<Notify>,
}

impl OpCtx {
    fn event(&self, stage: OperationStage, fraction: f32, msg: &str) -> ProgressEvent {
        ProgressEvent::new(
            self.op_id.clone(),
            self.package.clone(),
            self.operation,
            stage,
            fraction,
            msg,
        )
    }

    /// Progress is lossy-safe (fractions are monotonic), so it is dropped when the consumer
    /// lags. The last free slot is reserved for the terminal event, which is the only sender
    /// of the final slot, so exactly one terminal event always fits.
    fn progress(&self, stage: OperationStage, fraction: f32, msg: &str) {
        if self.tx.capacity() > 1 {
            let _ = self.tx.try_send(self.event(stage, fraction, msg));
        }
    }

    async fn log(&self, level: LogLevel, message: &str, detail: Option<String>) {
        self.inner
            .log(
                level,
                Some(self.operation),
                Some(&self.package),
                message,
                detail,
            )
            .await;
    }
}

impl Inner {
    async fn log(
        &self,
        level: LogLevel,
        operation: Option<Operation>,
        package: Option<&PackageId>,
        message: &str,
        detail: Option<String>,
    ) {
        let Some(pool) = &self.options.log_pool else {
            return;
        };
        let entry = OperationLogEntry {
            id: None,
            ts: now_unix(),
            level,
            operation,
            package_id: package.cloned(),
            message: message.to_string(),
            detail,
        };
        if let Err(e) = operation_log::append(pool, &entry).await {
            tracing::warn!("operation_log write failed: {e}");
        }
    }
}

impl WingetCli {
    /// Builds an executor for a known binary without probing it.
    pub fn new(path: impl Into<PathBuf>, options: WingetOptions) -> Self {
        WingetCli {
            inner: Arc::new(Inner {
                path: path.into(),
                options,
                guard: OpGuard::default(),
                cancels: Mutex::new(HashMap::new()),
                next_op: AtomicU64::new(1),
            }),
        }
    }

    /// Discovers winget and checks the version floor.
    pub async fn connect(options: WingetOptions) -> AppResult<Self> {
        Self::connect_at(discover().await?, options).await
    }

    /// Like `connect` for a known path. A missing binary is `WingetNotFound`.
    pub async fn connect_at(path: impl Into<PathBuf>, options: WingetOptions) -> AppResult<Self> {
        let cli = Self::new(path, options);
        cli.probe().await?;
        Ok(cli)
    }

    pub fn path(&self) -> &Path {
        &self.inner.path
    }

    pub fn op_guard(&self) -> &OpGuard {
        &self.inner.guard
    }

    /// Runs `winget --version`; enforces `MIN_VERSION`.
    pub async fn probe(&self) -> AppResult<WingetVersion> {
        let out = self
            .run_capture_timeout(&["--version".to_string()], PROBE_TIMEOUT)
            .await?;
        let version = WingetVersion::parse(&out.stdout).ok_or_else(|| {
            AppError::Unsupported(format!(
                "unrecognised winget version output: {}",
                out.stdout.trim()
            ))
        })?;
        if version < MIN_VERSION {
            return Err(AppError::Unsupported("winget >= 1.6 required".into()));
        }
        Ok(version)
    }

    fn command(&self, args: &[String]) -> AppResult<Command> {
        guard_args(args)?;
        let mut cmd = Command::new(&self.inner.path);
        cmd.args(args)
            .envs(self.inner.options.env.iter().map(|(k, v)| (k, v)))
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);
        no_window(&mut cmd);
        Ok(cmd)
    }

    fn spawn(&self, args: &[String]) -> AppResult<Child> {
        self.command(args)?.spawn().map_err(map_spawn_err)
    }

    async fn run_capture_timeout(&self, args: &[String], limit: Duration) -> AppResult<Captured> {
        let child = self.spawn(args)?;
        let out = tokio::time::timeout(limit, child.wait_with_output())
            .await
            .map_err(|_| AppError::SourceUnavailable("winget did not answer in time".into()))??;
        Ok(Captured {
            code: out.status.code().unwrap_or(-1),
            stdout: String::from_utf8_lossy(&out.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&out.stderr).into_owned(),
        })
    }

    async fn run_capture(&self, args: &[String]) -> AppResult<Captured> {
        self.run_capture_timeout(args, CAPTURE_TIMEOUT).await
    }

    // -- operations -----------------------------------------------------------------------

    async fn start_op(
        &self,
        package: &PackageId,
        operation: Operation,
        winget_id: String,
        args: Vec<String>,
    ) -> AppResult<OperationHandle> {
        guard_args(&args)?;
        let permit = self.inner.guard.try_acquire(package)?;
        let mut child = self.spawn(&args)?;
        let job = ProcessJob::attach(&child);
        if job.is_none() {
            tracing::debug!("job object unavailable; cancel falls back to taskkill");
        }
        let n = self.inner.next_op.fetch_add(1, Ordering::Relaxed);
        let op_id = format!("winget-{}-{n}", now_unix());
        let (tx, events) = mpsc::channel(EVENT_CHANNEL);
        let (otx, outcome) = oneshot::channel();
        let notify = Arc::new(Notify::new());
        self.inner
            .cancels
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .insert(op_id.clone(), Arc::clone(&notify));
        let ctx = OpCtx {
            inner: Arc::clone(&self.inner),
            op_id: op_id.clone(),
            package: package.clone(),
            winget_id,
            operation,
            tx,
            notify,
        };
        ctx.progress(OperationStage::Queued, 0.0, "queued");
        ctx.progress(OperationStage::Resolving, 0.0, "starting winget");
        let stdout = child.stdout.take();
        let stderr = child.stderr.take();
        tokio::spawn(async move {
            ctx.log(
                LogLevel::Info,
                &format!("{operation} started"),
                Some(args.join(" ")),
            )
            .await;
            let result = drive(&ctx, &mut child, job.as_ref(), stdout, stderr).await;
            if let Some(j) = &job {
                j.disarm();
            }
            finish(ctx, result, permit, otx).await;
        });
        Ok(OperationHandle {
            operation_id: op_id,
            events,
            outcome,
        })
    }

    pub async fn start_install(
        &self,
        package: &PackageId,
        request: &InstallRequest,
    ) -> AppResult<OperationHandle> {
        let (id, source) = winget_target(package)?;
        let args = install_args(&id, source, request, self.inner.options.accept_agreements);
        self.start_op(package, Operation::Install, id, args).await
    }

    pub async fn start_remove(&self, package: &PackageId) -> AppResult<OperationHandle> {
        let (id, source) = winget_target(package)?;
        let args = remove_args(&id, source, self.inner.options.accept_agreements);
        self.start_op(package, Operation::Remove, id, args).await
    }

    pub async fn start_update(&self, package: &PackageId) -> AppResult<OperationHandle> {
        let (id, source) = winget_target(package)?;
        let args = update_args(&id, source, self.inner.options.accept_agreements);
        self.start_op(package, Operation::Update, id, args).await
    }

    pub fn request_cancel(&self, operation_id: &str) {
        let notify = self
            .inner
            .cancels
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .get(operation_id)
            .cloned();
        if let Some(n) = notify {
            n.notify_one();
        }
    }

    pub async fn query_installed(&self) -> AppResult<Vec<InstalledPackage>> {
        let args = list_installed_args(self.inner.options.accept_agreements);
        let out = self.run_capture(&args).await?;
        if out.code as u32 == EXIT_NO_APPLICATIONS_FOUND {
            return Ok(Vec::new());
        }
        out.require_ok("installed packages")?;
        let table = parse_table(&out.stdout);
        for w in &table.warnings {
            tracing::warn!("winget list: {w}");
        }
        Ok(table
            .rows
            .into_iter()
            .map(|r| InstalledPackage {
                source: r.source.as_deref().and_then(source_kind),
                id: r.id,
                name: r.name,
                version: r.version,
                available: r.available,
            })
            .collect())
    }

    pub async fn query_updates(&self) -> AppResult<Vec<UpdateAvailable>> {
        let args = list_updates_args(self.inner.options.accept_agreements);
        let out = self.run_capture(&args).await?;
        let code = out.code as u32;
        if code == EXIT_NO_APPLICATIONS_FOUND || code == EXIT_UPDATE_NOT_APPLICABLE {
            return Ok(Vec::new());
        }
        out.require_ok("update check")?;
        let table = parse_table(&out.stdout);
        for w in &table.warnings {
            tracing::warn!("winget upgrade: {w}");
        }
        // Rows without a catalog source (ARP entries) cannot be updated by id; skip them.
        Ok(table
            .rows
            .into_iter()
            .filter_map(|r| {
                let source = r.source.as_deref().and_then(source_kind)?;
                Some(UpdateAvailable {
                    package_id: PackageId::new(source, &r.id),
                    installed_version: r.version,
                    latest_version: r.available?,
                    source,
                    is_auto: false,
                })
            })
            .collect())
    }

    pub async fn set_pin(&self, package: &PackageId, add: bool) -> AppResult<()> {
        let (id, _) = winget_target(package)?;
        let _permit = self.inner.guard.try_acquire(package)?;
        let out = self
            .run_capture(&pin_args(&id, add, self.inner.options.accept_agreements))
            .await?;
        out.require_ok(&id)
    }
}

fn source_kind(name: &str) -> Option<SourceKind> {
    match name {
        "winget" => Some(SourceKind::WingetCommunity),
        "msstore" => Some(SourceKind::MsStore),
        _ => None,
    }
}

fn map_spawn_err(e: std::io::Error) -> AppError {
    if e.kind() == std::io::ErrorKind::NotFound {
        AppError::WingetNotFound
    } else {
        AppError::Io(e)
    }
}

struct Captured {
    code: i32,
    stdout: String,
    stderr: String,
}

impl Captured {
    fn require_ok(&self, what: &str) -> AppResult<()> {
        let detail = if self.stderr.trim().is_empty() {
            self.stdout.trim()
        } else {
            self.stderr.trim()
        };
        match map_exit(self.code, what, detail, None) {
            ExitOutcome::Success | ExitOutcome::NoOp => Ok(()),
            ExitOutcome::Failure(e) => Err(e),
        }
    }
}

// ---------------------------------------------------------------------------------------------
// Operation driver
// ---------------------------------------------------------------------------------------------

struct Driven {
    /// `Ok(true)` = winget had nothing to do.
    result: AppResult<bool>,
    fraction: f32,
}

async fn read_bounded(mut r: impl AsyncRead + Unpin) -> String {
    let mut all = Vec::new();
    let mut buf = [0u8; 4096];
    while let Ok(n) = r.read(&mut buf).await {
        if n == 0 {
            break;
        }
        all.extend_from_slice(&buf[..n]);
        if all.len() > STDERR_CAP * 2 {
            all.drain(..all.len() - STDERR_CAP);
        }
    }
    String::from_utf8_lossy(&all).into_owned()
}

async fn process_lines(ctx: &OpCtx, parser: &mut ProgressParser, lines: Vec<String>) {
    for line in lines {
        let lower = line.trim_start().to_ascii_lowercase();
        if lower.starts_with("warning") {
            ctx.log(LogLevel::Warn, line.trim(), None).await;
        } else if lower.starts_with("error") || lower.starts_with("installer failed") {
            ctx.log(LogLevel::Error, line.trim(), None).await;
        }
        if let Some(u) = parser.feed_line(&line) {
            ctx.progress(u.stage, u.fraction, &u.message);
        }
    }
}

async fn drive<O, E>(
    ctx: &OpCtx,
    child: &mut Child,
    job: Option<&ProcessJob>,
    mut stdout: Option<O>,
    stderr: Option<E>,
) -> Driven
where
    O: AsyncRead + Unpin,
    E: AsyncRead + Unpin + Send + 'static,
{
    let stderr_task = stderr.map(|e| tokio::spawn(read_bounded(e)));
    let mut splitter = LineSplitter::default();
    let mut parser = ProgressParser::new();
    let mut buf = [0u8; 4096];
    let mut eof = stdout.is_none();
    let mut cancel_seen = false;
    let mut cancelled = false;

    let status = loop {
        tokio::select! {
            r = async { stdout.as_mut().unwrap().read(&mut buf).await }, if !eof => {
                match r {
                    Ok(n) if n > 0 => {
                        let lines = splitter.push(&buf[..n]);
                        process_lines(ctx, &mut parser, lines).await;
                    }
                    _ => eof = true,
                }
            }
            _ = ctx.notify.notified(), if !cancel_seen => {
                cancel_seen = true;
                // The child may have finished first; then the real outcome stands.
                if let Ok(Some(st)) = child.try_wait() {
                    break Ok(st);
                }
                if kill_tree(child, job).await {
                    // winget may have exited 0 between try_wait and the kill: Done wins.
                    let st = child.wait().await;
                    cancelled = cancel_wins(&st);
                    break st;
                }
                ctx.log(
                    LogLevel::Warn,
                    "cancel requested but the process tree could not be killed (elevated child?)",
                    None,
                )
                .await;
            }
            st = child.wait() => break st,
        }
    };

    if !eof && !cancelled {
        // winget exited; pick up output still sitting in the pipe (bounded: grandchildren
        // may keep the pipe open).
        let _ = tokio::time::timeout(DRAIN_TIMEOUT, async {
            while let Ok(n) = stdout.as_mut().unwrap().read(&mut buf).await {
                if n == 0 {
                    break;
                }
                let lines = splitter.push(&buf[..n]);
                process_lines(ctx, &mut parser, lines).await;
            }
        })
        .await;
    }
    if let Some(rest) = splitter.finish() {
        process_lines(ctx, &mut parser, vec![rest]).await;
    }
    let stderr_text = match stderr_task {
        Some(t) => tokio::time::timeout(DRAIN_TIMEOUT, t)
            .await
            .ok()
            .and_then(Result::ok)
            .unwrap_or_default(),
        None => String::new(),
    };

    let fraction = parser.fraction();
    if cancelled {
        return Driven {
            result: Err(AppError::Cancelled),
            fraction,
        };
    }
    let status = match status {
        Ok(s) => s,
        Err(e) => {
            return Driven {
                result: Err(AppError::Io(e)),
                fraction,
            }
        }
    };
    let detail = if stderr_text.trim().is_empty() {
        parser.tail_text()
    } else {
        stderr_text.trim().to_string()
    };
    let result = match map_exit(
        status.code().unwrap_or(-1),
        &ctx.winget_id,
        &detail,
        parser.installer_exit_code(),
    ) {
        ExitOutcome::Success => Ok(false),
        ExitOutcome::NoOp => Ok(true),
        ExitOutcome::Failure(e) => Err(e),
    };
    Driven { result, fraction }
}

/// Sends the single terminal event, logs, releases the guard and reports the outcome.
async fn finish(
    ctx: OpCtx,
    driven: Driven,
    permit: OpPermit,
    outcome: oneshot::Sender<AppResult<()>>,
) {
    let (stage, fraction, message) = match &driven.result {
        Ok(false) => (OperationStage::Done, 1.0, "done".to_string()),
        Ok(true) => (OperationStage::Done, 1.0, "already up to date".to_string()),
        Err(AppError::Cancelled) => (
            OperationStage::Cancelled,
            driven.fraction,
            "cancelled".to_string(),
        ),
        Err(e) => (OperationStage::Failed, driven.fraction, e.to_string()),
    };
    match &driven.result {
        Ok(_) => {
            ctx.log(LogLevel::Info, &format!("{} finished", ctx.operation), None)
                .await
        }
        Err(AppError::Cancelled) => {
            ctx.log(
                LogLevel::Info,
                &format!("{} cancelled", ctx.operation),
                None,
            )
            .await
        }
        Err(e) => {
            ctx.log(
                LogLevel::Error,
                &format!("{} failed: {e}", ctx.operation),
                e.detail(),
            )
            .await
        }
    }
    // A slot is always free (see `progress`); this only fails if the receiver is gone.
    let _ = ctx.tx.send(ctx.event(stage, fraction, &message)).await;
    ctx.inner
        .cancels
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .remove(&ctx.op_id);
    drop(permit);
    let _ = outcome.send(driven.result.map(|_| ()));
}

// ---------------------------------------------------------------------------------------------
// Trait impl
// ---------------------------------------------------------------------------------------------

impl PackageExecutor for WingetCli {
    fn install<'a>(
        &'a self,
        package: &'a PackageId,
        request: &'a InstallRequest,
    ) -> ExecFuture<'a, OperationHandle> {
        Box::pin(self.start_install(package, request))
    }

    fn cancel<'a>(&'a self, operation_id: &'a str) -> ExecFuture<'a, ()> {
        Box::pin(async move {
            self.request_cancel(operation_id);
            Ok(())
        })
    }

    fn remove<'a>(&'a self, package: &'a PackageId) -> ExecFuture<'a, OperationHandle> {
        Box::pin(self.start_remove(package))
    }

    fn list_installed<'a>(&'a self) -> ExecFuture<'a, Vec<InstalledPackage>> {
        Box::pin(self.query_installed())
    }

    fn list_updates<'a>(&'a self) -> ExecFuture<'a, Vec<UpdateAvailable>> {
        Box::pin(self.query_updates())
    }

    fn update<'a>(&'a self, package: &'a PackageId) -> ExecFuture<'a, OperationHandle> {
        Box::pin(self.start_update(package))
    }

    fn pin<'a>(&'a self, package: &'a PackageId) -> ExecFuture<'a, ()> {
        Box::pin(self.set_pin(package, true))
    }

    fn unpin<'a>(&'a self, package: &'a PackageId) -> ExecFuture<'a, ()> {
        Box::pin(self.set_pin(package, false))
    }

    fn is_available<'a>(
        &'a self,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = bool> + Send + 'a>> {
        Box::pin(async move { self.probe().await.is_ok() })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::executor::InstallScope;

    fn has(args: &[String], flag: &str) -> bool {
        args.iter().any(|a| a == flag)
    }

    fn never_forbidden(args: &[String]) {
        for f in FORBIDDEN_FLAGS {
            assert!(!has(args, f), "{f} in {args:?}");
        }
        guard_args(args).unwrap();
    }

    #[test]
    fn install_argv_matches_skill() {
        let req = InstallRequest {
            version: Some("1.2.3".into()),
            scope: Some(InstallScope::User),
        };
        let a = install_args("Foo.Bar", "winget", &req, false);
        assert_eq!(
            a,
            [
                "install",
                "--id",
                "Foo.Bar",
                "--exact",
                "--source",
                "winget",
                "--version",
                "1.2.3",
                "--scope",
                "user",
                "--silent",
                "--disable-interactivity",
                "--verbose-logs"
            ]
        );
        never_forbidden(&a);
    }

    #[test]
    fn agreement_flags_only_when_accepted() {
        let req = InstallRequest::default();
        let off = install_args("A.B", "winget", &req, false);
        assert!(!has(&off, "--accept-source-agreements"));
        assert!(!has(&off, "--accept-package-agreements"));
        let on = install_args("A.B", "winget", &req, true);
        assert!(has(&on, "--accept-source-agreements") && has(&on, "--accept-package-agreements"));

        // uninstall / list accept only the source flag
        assert!(!has(
            &remove_args("A.B", "winget", false),
            "--accept-source-agreements"
        ));
        let rm = remove_args("A.B", "winget", true);
        assert!(has(&rm, "--accept-source-agreements") && !has(&rm, "--accept-package-agreements"));
        let up = update_args("A.B", "winget", true);
        assert!(has(&up, "--accept-package-agreements"));
        assert!(!has(
            &update_args("A.B", "winget", false),
            "--accept-package-agreements"
        ));
        assert!(has(
            &list_installed_args(true),
            "--accept-source-agreements"
        ));
        assert!(!has(
            &list_updates_args(false),
            "--accept-source-agreements"
        ));
    }

    #[test]
    fn every_builder_is_clean_and_exact() {
        let all = [
            install_args("A.B", "msstore", &InstallRequest::default(), true),
            update_args("A.B", "winget", true),
            remove_args("A.B", "winget", true),
            list_installed_args(true),
            list_updates_args(true),
            pin_args("A.B", true, true),
            pin_args("A.B", false, true),
        ];
        for a in &all {
            never_forbidden(a);
            assert!(a.iter().all(|x| !x.contains("force")));
        }
        for a in [&all[0], &all[1], &all[2], &all[5], &all[6]] {
            assert!(has(a, "--exact"), "{a:?}");
        }
        assert!(has(&all[3], "--disable-interactivity") && has(&all[4], "--include-unknown"));
        assert_eq!(all[5][..3], ["pin", "add", "--id"]);
        assert_eq!(all[6][1], "remove");
    }

    #[test]
    fn pin_agreements_flag_only_when_accepted() {
        assert!(!has(
            &pin_args("A.B", true, false),
            "--accept-source-agreements"
        ));
        assert!(has(
            &pin_args("A.B", true, true),
            "--accept-source-agreements"
        ));
        assert!(has(
            &pin_args("A.B", false, true),
            "--accept-source-agreements"
        ));
    }

    #[test]
    fn bare_start_kill_is_not_a_cancel() {
        assert!(tree_killed(true, false));
        assert!(tree_killed(false, true));
        assert!(!tree_killed(false, false));
    }

    #[cfg(windows)]
    #[test]
    fn exit_zero_beats_cancel() {
        use std::os::windows::process::ExitStatusExt;
        use std::process::ExitStatus;
        assert!(!cancel_wins(&Ok(ExitStatus::from_raw(0))));
        assert!(cancel_wins(&Ok(ExitStatus::from_raw(1))));
        assert!(cancel_wins(&Err(std::io::Error::other("x"))));
    }

    #[test]
    fn guard_rejects_forbidden_flags() {
        for f in FORBIDDEN_FLAGS {
            let err = guard_args(&["install".into(), f.into()]).unwrap_err();
            assert_eq!(err.code(), "invalid_config");
        }
    }

    #[test]
    fn target_maps_sources() {
        let p = PackageId::new(SourceKind::WingetCommunity, "Foo.Bar");
        assert_eq!(winget_target(&p).unwrap(), ("Foo.Bar".into(), "winget"));
        let p = PackageId::new(SourceKind::MsStore, "9NBLGGH4NNS1");
        assert_eq!(winget_target(&p).unwrap().1, "msstore");
        let p = PackageId::new(SourceKind::Scoop, "x");
        assert_eq!(winget_target(&p).unwrap_err().code(), "unsupported");
    }

    #[test]
    fn discovery_order() {
        let dir = tempfile::tempdir().unwrap();
        let over = dir.path().join("fake.exe");
        std::fs::write(&over, b"").unwrap();
        let on_path = dir.path().join("bin");
        std::fs::create_dir(&on_path).unwrap();
        std::fs::write(on_path.join("winget.exe"), b"").unwrap();
        let apps = dir.path().join("lad");
        std::fs::create_dir_all(apps.join("Microsoft").join("WindowsApps")).unwrap();
        std::fs::write(
            apps.join("Microsoft")
                .join("WindowsApps")
                .join("winget.exe"),
            b"",
        )
        .unwrap();

        let path_var = std::env::join_paths([&on_path]).unwrap();
        // 1) override wins
        let got = locate_with(
            Some(over.clone().into()),
            Some(path_var.clone()),
            Some(apps.clone().into()),
        );
        assert_eq!(got.unwrap(), over);
        // override set but missing is final
        let missing = dir.path().join("nope.exe");
        assert!(locate_with(Some(missing.into()), Some(path_var.clone()), None).is_none());
        // 2) PATH
        let got = locate_with(None, Some(path_var), Some(apps.clone().into())).unwrap();
        assert_eq!(got, on_path.join("winget.exe"));
        // 3) WindowsApps alias
        let got = locate_with(None, None, Some(apps.clone().into())).unwrap();
        assert!(
            got.ends_with("WindowsApps/winget.exe") || got.ends_with("WindowsApps\\winget.exe")
        );
        // none
        assert!(locate_with(None, None, None).is_none());
    }

    #[tokio::test]
    async fn missing_binary_is_winget_not_found() {
        let dir = tempfile::tempdir().unwrap();
        let err = WingetCli::connect_at(dir.path().join("absent.exe"), WingetOptions::default())
            .await
            .map(|_| ())
            .unwrap_err();
        assert!(matches!(err, AppError::WingetNotFound), "{err:?}");
    }
}
