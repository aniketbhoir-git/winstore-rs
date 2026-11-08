//! Domain model (SPEC section 4): plain data types with no I/O.

use std::fmt;

use crate::error::AppError;

/// Error returned when text does not name a variant of a domain enum.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseEnumError {
    pub kind: &'static str,
    pub value: String,
}

impl fmt::Display for ParseEnumError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "unknown {}: {}", self.kind, self.value)
    }
}

impl std::error::Error for ParseEnumError {}

impl From<ParseEnumError> for AppError {
    fn from(e: ParseEnumError) -> Self {
        AppError::InvalidConfig(e.to_string())
    }
}

/// Declares a fieldless enum with snake_case serde, `Display`, `FromStr`,
/// `as_str` and a `VARIANTS` list, all from one table.
macro_rules! string_enum {
    ($(#[$meta:meta])* $name:ident, $label:literal, { $($variant:ident => $text:literal),+ $(,)? }) => {
        $(#[$meta])*
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
        #[serde(rename_all = "snake_case")]
        pub enum $name {
            $($variant),+
        }

        impl $name {
            pub const VARIANTS: &'static [$name] = &[$($name::$variant),+];

            pub fn as_str(&self) -> &'static str {
                match self {
                    $($name::$variant => $text),+
                }
            }
        }

        impl std::fmt::Display for $name {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str(self.as_str())
            }
        }

        impl std::str::FromStr for $name {
            type Err = $crate::domain::ParseEnumError;

            fn from_str(s: &str) -> Result<Self, Self::Err> {
                match s {
                    $($text => Ok($name::$variant),)+
                    _ => Err($crate::domain::ParseEnumError {
                        kind: $label,
                        value: s.to_string(),
                    }),
                }
            }
        }
    };
}

pub mod install;
pub mod package;
pub mod progress;
pub mod source;

pub use install::{InstallRecord, InstallState, PackageVersion, UpdateAvailable};
pub use package::{
    InstallerMetadata, PackageDetail, PackageId, PackageSummary, PackagingKind, SourceKind,
};
pub use progress::{LogLevel, Operation, OperationLogEntry, OperationStage, ProgressEvent};
pub use source::{SourceCapabilities, SourceConfig};

/// Current time as Unix seconds.
pub fn now_unix() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn now_is_plausible() {
        assert!(now_unix() > 1_700_000_000);
    }

    #[test]
    fn parse_error_converts_to_invalid_config() {
        let e: AppError = "nope".parse::<SourceKind>().unwrap_err().into();
        assert_eq!(e.code(), "invalid_config");
    }
}
