use serde::ser::SerializeStruct;
use serde::{Serialize, Serializer};

/// One agreement that winget printed: a label, an optional text and an optional https link
/// (SPEC §6.6). The UI shows all three as text.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AgreementEntry {
    pub label: String,
    pub text: Option<String>,
    pub url: Option<String>,
}

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
    #[error("This app needs administrator rights to install. Windows did not give them. Try again and accept the Windows prompt, or choose 'Only for me' if the app supports it.")]
    ElevationRequired,
    #[error("invalid configuration: {0}")]
    InvalidConfig(String),
    /// winget needs the user to accept source and/or package agreements (SPEC §0 rule 4).
    /// Fields are not named `source`/`package` because thiserror treats `source` as the cause.
    #[error(
        "agreements must be accepted (source: {source_agreements}, package: {package_agreements})"
    )]
    AgreementsRequired {
        source_agreements: bool,
        package_agreements: bool,
        /// Data for the consent dialog (SPEC §1.3); the executor fills `winget_source` and
        /// `version`, the text and the link stay `None` in WP5.
        winget_source: Option<String>,
        version: Option<String>,
        agreement_text: Option<String>,
        agreement_url: Option<String>,
        /// Every agreement winget printed (label, text, link), for the consent dialog.
        agreements: Vec<AgreementEntry>,
        /// Handle that binds a later consent to this package, version, winget source and
        /// kind (SPEC §6.6 "How consent is bound"). Set by `services/install.rs`.
        consent_token: Option<String>,
    },
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
            AppError::AgreementsRequired { .. } => "agreements_required",
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
        if let AppError::AgreementsRequired {
            source_agreements,
            package_agreements,
            winget_source,
            version,
            agreement_text,
            agreement_url,
            agreements,
            consent_token,
        } = self
        {
            let mut st = serializer.serialize_struct("AppError", 12)?;
            st.serialize_field("code", self.code())?;
            st.serialize_field("message", &self.to_string())?;
            st.serialize_field("detail", &self.detail())?;
            st.serialize_field("retryable", &self.retryable())?;
            st.serialize_field("source_agreements", source_agreements)?;
            st.serialize_field("package_agreements", package_agreements)?;
            st.serialize_field("winget_source", winget_source)?;
            st.serialize_field("version", version)?;
            st.serialize_field("agreement_text", agreement_text)?;
            st.serialize_field("agreement_url", agreement_url)?;
            st.serialize_field("agreements", agreements)?;
            st.serialize_field("consent_token", consent_token)?;
            return st.end();
        }
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
            (
                AppError::AgreementsRequired {
                    source_agreements: true,
                    package_agreements: false,
                    winget_source: None,
                    version: None,
                    agreement_text: None,
                    agreement_url: None,
                    agreements: Vec::new(),
                    consent_token: None,
                },
                "agreements_required",
                false,
            ),
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
    fn agreements_required_json_carries_eight_extra_fields() {
        let err = AppError::AgreementsRequired {
            source_agreements: false,
            package_agreements: true,
            winget_source: Some("winget".into()),
            version: None,
            agreement_text: None,
            agreement_url: None,
            agreements: Vec::new(),
            consent_token: None,
        };
        let v = serde_json::to_value(&err).unwrap();
        let o = v.as_object().unwrap();
        assert_eq!(o.len(), 12);
        assert_eq!(v["agreements"], serde_json::json!([]));
        assert_eq!(v["code"], "agreements_required");
        assert_eq!(v["source_agreements"], false);
        assert_eq!(v["package_agreements"], true);
        assert_eq!(v["winget_source"], "winget");
        for k in [
            "version",
            "agreement_text",
            "agreement_url",
            "consent_token",
            "detail",
        ] {
            assert!(o.get(k).is_some_and(|x| x.is_null()), "{k}");
        }
        assert_eq!(v["retryable"], false);
        // every other variant keeps the four-field shape
        for (err, _, _) in all() {
            if err.code() != "agreements_required" {
                let v = serde_json::to_value(&err).unwrap();
                assert_eq!(v.as_object().unwrap().len(), 4, "{}", err.code());
            }
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
