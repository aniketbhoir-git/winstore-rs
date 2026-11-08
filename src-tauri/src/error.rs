use serde::ser::SerializeStruct;
use serde::{Serialize, Serializer};

/// Application-wide error type (SPEC §1.3).
#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("database error: {0}")]
    Sql(#[from] sqlx::Error),
    #[error("network error: {0}")]
    Http(String),
    #[error("winget was not found on this system")]
    WingetNotFound,
    #[error("winget failed with exit code {code}")]
    WingetFailed { code: i32, stderr: String },
    #[error("source unavailable: {0}")]
    SourceUnavailable(String),
    #[error("package not found: {0}")]
    PackageNotFound(String),
    #[error("deployment failed: {0}")]
    DeploymentFailed(String),
    #[error("operation cancelled")]
    Cancelled,
    #[error("unsupported: {0}")]
    Unsupported(String),
    #[error("administrator permission is required")]
    ElevationRequired,
    #[error("invalid configuration: {0}")]
    InvalidConfig(String),
}

pub type AppResult<T> = Result<T, AppError>;

impl AppError {
    /// Stable snake_case identifier for the frontend.
    pub fn code(&self) -> &'static str {
        match self {
            AppError::Io(_) => "io",
            AppError::Sql(_) => "sql",
            AppError::Http(_) => "http",
            AppError::WingetNotFound => "winget_not_found",
            AppError::WingetFailed { .. } => "winget_failed",
            AppError::SourceUnavailable(_) => "source_unavailable",
            AppError::PackageNotFound(_) => "package_not_found",
            AppError::DeploymentFailed(_) => "deployment_failed",
            AppError::Cancelled => "cancelled",
            AppError::Unsupported(_) => "unsupported",
            AppError::ElevationRequired => "elevation_required",
            AppError::InvalidConfig(_) => "invalid_config",
        }
    }

    /// Whether retrying the same operation may succeed.
    pub fn retryable(&self) -> bool {
        matches!(
            self,
            AppError::Io(_)
                | AppError::Sql(_)
                | AppError::Http(_)
                | AppError::SourceUnavailable(_)
                | AppError::WingetFailed { .. }
        )
    }

    /// Raw technical detail (stderr, underlying error chain), if any.
    pub fn detail(&self) -> Option<String> {
        match self {
            AppError::Io(e) => Some(error_chain(e)),
            AppError::Sql(e) => Some(error_chain(e)),
            AppError::WingetFailed { stderr, .. } => Some(stderr.clone()),
            _ => None,
        }
    }
}

fn error_chain(e: &dyn std::error::Error) -> String {
    let mut out = e.to_string();
    let mut src = e.source();
    while let Some(s) = src {
        out.push_str(": ");
        out.push_str(&s.to_string());
        src = s.source();
    }
    out
}

impl Serialize for AppError {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut st = serializer.serialize_struct("AppError", 4)?;
        st.serialize_field("code", self.code())?;
        st.serialize_field("message", &self.to_string())?;
        st.serialize_field("detail", &self.detail())?;
        st.serialize_field("retryable", &self.retryable())?;
        st.end()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn all() -> Vec<(AppError, &'static str, bool)> {
        vec![
            (AppError::Io(std::io::Error::other("x")), "io", true),
            (AppError::Sql(sqlx::Error::RowNotFound), "sql", true),
            (AppError::Http("x".into()), "http", true),
            (AppError::WingetNotFound, "winget_not_found", false),
            (
                AppError::WingetFailed {
                    code: 1,
                    stderr: "boom".into(),
                },
                "winget_failed",
                true,
            ),
            (
                AppError::SourceUnavailable("x".into()),
                "source_unavailable",
                true,
            ),
            (
                AppError::PackageNotFound("x".into()),
                "package_not_found",
                false,
            ),
            (
                AppError::DeploymentFailed("x".into()),
                "deployment_failed",
                false,
            ),
            (AppError::Cancelled, "cancelled", false),
            (AppError::Unsupported("x".into()), "unsupported", false),
            (AppError::ElevationRequired, "elevation_required", false),
            (AppError::InvalidConfig("x".into()), "invalid_config", false),
        ]
    }

    #[test]
    fn codes_and_retryable_flags() {
        for (err, code, retry) in all() {
            assert_eq!(err.code(), code);
            assert_eq!(err.retryable(), retry, "{code}");
            let v = serde_json::to_value(&err).unwrap();
            assert_eq!(v["code"], code);
            assert_eq!(v["retryable"], retry);
        }
    }

    #[test]
    fn json_shape_and_detail() {
        let err = AppError::WingetFailed {
            code: 3,
            stderr: "boom".into(),
        };
        let v = serde_json::to_value(&err).unwrap();
        for k in ["code", "message", "detail", "retryable"] {
            assert!(v.get(k).is_some(), "missing {k}");
        }
        assert_eq!(v["detail"], "boom");
        assert!(v["message"].as_str().unwrap().contains('3'));
        let none = serde_json::to_value(AppError::Cancelled).unwrap();
        assert!(none["detail"].is_null());
    }
}
