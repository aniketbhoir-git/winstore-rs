//! Windows-only checks (SPEC §6.8). WP11 step 1 holds the elevation check; toasts and the
//! scheduled task come in later steps.

use serde::Serialize;

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
}
