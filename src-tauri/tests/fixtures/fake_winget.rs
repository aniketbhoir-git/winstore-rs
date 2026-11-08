//! Fake `winget.exe` for executor tests (Issue #4).
//!
//! Why a Rust bin and not PowerShell: starts in milliseconds, no execution-policy or
//! profile surprises, and can emit raw bytes (invalid UTF-8, bare `\r`).
//!
//! Environment (set per spawned child by the tests):
//! `FAKE_WINGET_SCRIPT` is a script file, one directive per line:
//!
//! ```text
//! `out TEXT`     print TEXT + "\r\n"      (escapes: `\r` `\n` `\\`)
//! `raw TEXT`     print TEXT with no line ending (same escapes)
//! `hex AABB..`   print raw bytes
//! `err TEXT`     print TEXT + "\r\n" to stderr
//! `sleep MS`     sleep
//! `exit CODE`    exit with CODE (decimal or 0x hex; u32 HRESULTs allowed)
//! ```
//!
//! Other variables:
//!
//! - `FAKE_WINGET_ARGV`    file; one line appended per invocation (args joined by spaces)
//! - `FAKE_WINGET_PID`     file; the process id is written there
//! - `FAKE_WINGET_VERSION` text printed for `--version` (default `v1.29.380`)

use std::io::Write;
use std::time::Duration;

fn unescape(s: &str) -> String {
    let mut out = String::new();
    let mut it = s.chars();
    while let Some(c) = it.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match it.next() {
            Some('r') => out.push('\r'),
            Some('n') => out.push('\n'),
            Some('\\') => out.push('\\'),
            Some(o) => {
                out.push('\\');
                out.push(o);
            }
            None => out.push('\\'),
        }
    }
    out
}

fn parse_code(s: &str) -> i32 {
    let s = s.trim();
    let v = match s.strip_prefix("0x") {
        Some(h) => u32::from_str_radix(h, 16).expect("hex exit code"),
        None => s
            .parse::<i64>()
            .map(|n| n as u32)
            .expect("decimal exit code"),
    };
    v as i32
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if let Ok(p) = std::env::var("FAKE_WINGET_ARGV") {
        let mut f = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(p)
            .expect("argv file");
        writeln!(f, "{}", args.join(" ")).unwrap();
    }
    if let Ok(p) = std::env::var("FAKE_WINGET_PID") {
        std::fs::write(p, std::process::id().to_string()).unwrap();
    }
    if args.first().map(String::as_str) == Some("--version") {
        let v = std::env::var("FAKE_WINGET_VERSION").unwrap_or_else(|_| "v1.29.380".into());
        println!("{v}");
        return;
    }
    let Ok(script) = std::env::var("FAKE_WINGET_SCRIPT") else {
        return;
    };
    let text = std::fs::read_to_string(script).expect("script file");
    let stdout = std::io::stdout();
    let stderr = std::io::stderr();
    for line in text.lines() {
        let (cmd, rest) = line.split_once(' ').unwrap_or((line, ""));
        match cmd {
            "out" => {
                let mut o = stdout.lock();
                write!(o, "{}\r\n", unescape(rest)).unwrap();
                o.flush().unwrap();
            }
            "raw" => {
                let mut o = stdout.lock();
                write!(o, "{}", unescape(rest)).unwrap();
                o.flush().unwrap();
            }
            "hex" => {
                let clean: String = rest.split_whitespace().collect();
                let bytes: Vec<u8> = (0..clean.len() / 2)
                    .map(|i| u8::from_str_radix(&clean[i * 2..i * 2 + 2], 16).unwrap())
                    .collect();
                let mut o = stdout.lock();
                o.write_all(&bytes).unwrap();
                o.flush().unwrap();
            }
            "err" => {
                let mut e = stderr.lock();
                write!(e, "{}\r\n", unescape(rest)).unwrap();
                e.flush().unwrap();
            }
            "sleep" => std::thread::sleep(Duration::from_millis(rest.trim().parse().unwrap())),
            "exit" => std::process::exit(parse_code(rest)),
            "" => {}
            other => panic!("unknown directive {other}"),
        }
    }
}
