//! Windows-only checks and toasts (SPEC §6.8, §6.9). WP11 step 1 holds the elevation check,
//! step 3 the toast notifier; the scheduled task comes in a later step.

use std::sync::{Arc, Mutex, OnceLock};

use serde::Serialize;
use tauri::{AppHandle, Manager};

/// Tells whether this process runs with an elevated token (SPEC §6.8 rule 7).
pub trait ElevationCheck: Send + Sync {
    fn is_elevated(&self) -> bool;
}

/// Reads `TokenElevation` of the process token. A failed read counts as "not elevated":
/// the banner only informs, so the safe answer is to show none.
pub struct WindowsElevation;

impl ElevationCheck for WindowsElevation {
    fn is_elevated(&self) -> bool {
        #[cfg(windows)]
        {
            read_token_elevation().unwrap_or_else(|e| {
                tracing::warn!("elevation check failed: {e}");
                false
            })
        }
        #[cfg(not(windows))]
        {
            false
        }
    }
}

#[cfg(windows)]
fn read_token_elevation() -> windows::core::Result<bool> {
    use windows::Win32::Foundation::{CloseHandle, HANDLE};
    use windows::Win32::Security::{
        GetTokenInformation, TokenElevation, TOKEN_ELEVATION, TOKEN_QUERY,
    };
    use windows::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};

    let mut token = HANDLE::default();
    // SAFETY: `token` is a valid out pointer; the handle is closed below on every path.
    unsafe { OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token)? };
    let mut elevation = TOKEN_ELEVATION::default();
    let mut returned = 0u32;
    // SAFETY: the buffer is a `TOKEN_ELEVATION` and its exact size is passed.
    let result = unsafe {
        GetTokenInformation(
            token,
            TokenElevation,
            Some(&mut elevation as *mut TOKEN_ELEVATION as *mut _),
            std::mem::size_of::<TOKEN_ELEVATION>() as u32,
            &mut returned,
        )
    };
    // SAFETY: `token` came from `OpenProcessToken` and is closed once.
    let _ = unsafe { CloseHandle(token) };
    result?;
    Ok(elevation.TokenIsElevated != 0)
}

/// Payload of the `app_status` command.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct AppStatus {
    /// True when Store itself runs as administrator (the UI shows a banner).
    pub elevated: bool,
}

pub fn status_from(check: &dyn ElevationCheck) -> AppStatus {
    AppStatus {
        elevated: check.is_elevated(),
    }
}

// ---------------------------------------------------------------------------------------------
// Toasts (SPEC §6.9)
// ---------------------------------------------------------------------------------------------

/// A toast: a title and a body, both plain text from the app (SPEC §6.9 rule 3).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Toast {
    pub title: String,
    pub body: String,
}

/// Shows a toast. The real one calls Windows; tests record the toasts instead.
pub trait Notifier: Send + Sync {
    /// `Err` carries a short reason. The caller writes one Warn row and goes on.
    fn show(&self, toast: &Toast) -> Result<(), String>;
}

/// Tells whether the main window has focus.
pub trait WindowFocus: Send + Sync {
    fn is_focused(&self) -> bool;
}

/// Notifier that shows nothing (a state built without a window).
pub struct NoNotifier;

impl Notifier for NoNotifier {
    fn show(&self, _toast: &Toast) -> Result<(), String> {
        Ok(())
    }
}

/// A window that always has focus, so no toast is due.
pub struct AlwaysFocused;

impl WindowFocus for AlwaysFocused {
    fn is_focused(&self) -> bool {
        true
    }
}

/// Escapes text for an XML text node and drops characters XML 1.0 forbids.
fn xml_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            c if (c as u32) < 0x20 && !matches!(c, '\t' | '\n' | '\r') => {}
            c => out.push(c),
        }
    }
    out
}

/// The toast XML: two text lines, no image, no link, no action element (SPEC §6.9 rule 3).
pub fn toast_xml(toast: &Toast) -> String {
    format!(
        "<toast><visual><binding template=\"ToastGeneric\"><text>{}</text><text>{}</text></binding></visual></toast>",
        xml_escape(&toast.title),
        xml_escape(&toast.body)
    )
}

/// The real notifier. The AppUserModelID is the bundle identifier of `tauri.conf.json`. The app
/// handle arrives in `attach` once Tauri has built the app (the state exists before it).
#[derive(Default)]
pub struct WindowsNotifier {
    app: OnceLock<AppHandle>,
}

impl WindowsNotifier {
    pub fn attach(&self, app: &AppHandle) {
        let _ = self.app.set(app.clone());
    }
}

impl Notifier for WindowsNotifier {
    fn show(&self, toast: &Toast) -> Result<(), String> {
        let app = self.app.get().ok_or("the app is not ready")?;
        #[cfg(windows)]
        {
            let aumid = app.config().identifier.clone();
            let handle = app.clone();
            show_windows_toast(&aumid, &toast_xml(toast), move || {
                let h = handle.clone();
                let _ = handle.run_on_main_thread(move || focus_main_window(&h));
            })
            .map_err(|e| e.to_string())
        }
        #[cfg(not(windows))]
        {
            let _ = (app, toast);
            Err("toasts need Windows".into())
        }
    }
}

/// Shows, restores and focuses the main window (the toast activation, SPEC §6.9 rule 1).
#[cfg(windows)]
fn focus_main_window(app: &AppHandle) {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.show();
        let _ = w.unminimize();
        let _ = w.set_focus();
    }
}

#[cfg(windows)]
fn show_windows_toast(
    aumid: &str,
    xml: &str,
    on_activated: impl Fn() + Send + 'static,
) -> windows::core::Result<()> {
    use windows::core::HSTRING;
    use windows::Data::Xml::Dom::XmlDocument;
    use windows::Foundation::TypedEventHandler;
    use windows::UI::Notifications::{ToastNotification, ToastNotificationManager};

    let doc = XmlDocument::new()?;
    doc.LoadXml(&HSTRING::from(xml))?;
    let toast = ToastNotification::CreateToastNotification(&doc)?;
    let on_activated = Mutex::new(on_activated);
    toast.Activated(&TypedEventHandler::new(move |_, _| {
        if let Ok(f) = on_activated.lock() {
            f();
        }
        Ok(())
    }))?;
    let notifier = ToastNotificationManager::CreateToastNotifierWithId(&HSTRING::from(aumid))?;
    notifier.Show(&toast)
}

/// The real focus check: the Tauri main window. A missing window counts as not focused.
#[derive(Default)]
pub struct TauriWindowFocus {
    app: OnceLock<AppHandle>,
}

impl TauriWindowFocus {
    pub fn attach(&self, app: &AppHandle) {
        let _ = self.app.set(app.clone());
    }
}

impl WindowFocus for TauriWindowFocus {
    fn is_focused(&self) -> bool {
        self.app
            .get()
            .and_then(|a| a.get_webview_window("main"))
            .and_then(|w| w.is_focused().ok())
            .unwrap_or(false)
    }
}

/// Test double: records every toast. `fail` makes `show` fail.
#[derive(Default)]
pub struct FakeNotifier {
    pub shown: Mutex<Vec<Toast>>,
    pub fail: std::sync::atomic::AtomicBool,
}

impl FakeNotifier {
    pub fn toasts(&self) -> Vec<Toast> {
        self.shown.lock().unwrap_or_else(|e| e.into_inner()).clone()
    }
}

impl Notifier for FakeNotifier {
    fn show(&self, toast: &Toast) -> Result<(), String> {
        if self.fail.load(std::sync::atomic::Ordering::SeqCst) {
            return Err("fake failure".into());
        }
        self.shown
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .push(toast.clone());
        Ok(())
    }
}

/// Test double: a fixed focus value.
pub struct FakeWindowFocus(pub bool);

impl WindowFocus for FakeWindowFocus {
    fn is_focused(&self) -> bool {
        self.0
    }
}

/// The two platform parts a `Core` needs for toasts.
#[derive(Clone)]
pub struct ToastDeps {
    pub notifier: Arc<dyn Notifier>,
    pub focus: Arc<dyn WindowFocus>,
}

impl Default for ToastDeps {
    fn default() -> Self {
        ToastDeps {
            notifier: Arc::new(NoNotifier),
            focus: Arc::new(AlwaysFocused),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Fake(bool);
    impl ElevationCheck for Fake {
        fn is_elevated(&self) -> bool {
            self.0
        }
    }

    #[test]
    fn status_reports_an_elevated_app() {
        assert!(status_from(&Fake(true)).elevated);
        assert!(!status_from(&Fake(false)).elevated);
    }

    #[test]
    fn status_serialises_for_the_ui() {
        let v = serde_json::to_value(status_from(&Fake(true))).unwrap();
        assert_eq!(v, serde_json::json!({ "elevated": true }));
    }

    #[test]
    fn real_check_reads_the_token_without_panicking() {
        // The value depends on the machine (a hosted runner is elevated); only the read is tested.
        let _ = WindowsElevation.is_elevated();
    }

    #[test]
    fn toast_xml_escapes_text_and_has_no_image_link_or_action() {
        let t = Toast {
            title: "A <b> & \"c\"".into(),
            body: "it's \u{1}fine".into(),
        };
        let xml = toast_xml(&t);
        assert!(xml.contains("A &lt;b&gt; &amp; &quot;c&quot;"));
        assert!(xml.contains("it&apos;s fine"));
        for bad in ["<image", "<action", "<a ", "href", "src=", "activationType"] {
            assert!(!xml.contains(bad), "{bad} in {xml}");
        }
        // Only the fixed elements: toast, visual, binding, two text, each opened and closed.
        assert_eq!(xml.matches('<').count(), 10, "{xml}");
    }

    #[test]
    fn toast_xml_plain_case() {
        let xml = toast_xml(&Toast {
            title: "T".into(),
            body: "B".into(),
        });
        assert_eq!(
            xml,
            "<toast><visual><binding template=\"ToastGeneric\"><text>T</text><text>B</text></binding></visual></toast>"
        );
    }
}
