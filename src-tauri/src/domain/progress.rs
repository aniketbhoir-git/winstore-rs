use serde::{Deserialize, Serialize};

use super::now_unix;
use super::package::PackageId;

string_enum!(
    /// Kind of long-running operation.
    Operation, "operation", {
        Install => "install",
        Remove => "remove",
        Update => "update",
    }
);

string_enum!(
    /// Where an operation currently is.
    OperationStage, "operation stage", {
        Queued => "queued",
        Resolving => "resolving",
        Downloading => "downloading",
        Installing => "installing",
        Finalizing => "finalizing",
        Done => "done",
        Failed => "failed",
        Cancelled => "cancelled",
    }
);

string_enum!(
    /// Severity of an operation log line.
    LogLevel, "log level", {
        Debug => "debug",
        Info => "info",
        Warn => "warn",
        Error => "error",
    }
);

impl OperationStage {
    pub fn is_terminal(self) -> bool {
        matches!(
            self,
            OperationStage::Done | OperationStage::Failed | OperationStage::Cancelled
        )
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProgressEvent {
    pub operation_id: String,
    pub package_id: PackageId,
    pub operation: Operation,
    pub stage: OperationStage,
    pub fraction: f32,
    pub message: String,
    pub timestamp: i64,
}

impl ProgressEvent {
    /// Builds an event stamped with the current time; `fraction` is clamped to 0.0..=1.0.
    pub fn new(
        operation_id: impl Into<String>,
        package_id: PackageId,
        operation: Operation,
        stage: OperationStage,
        fraction: f32,
        message: impl Into<String>,
    ) -> Self {
        let fraction = if fraction.is_nan() {
            0.0
        } else {
            fraction.clamp(0.0, 1.0)
        };
        ProgressEvent {
            operation_id: operation_id.into(),
            package_id,
            operation,
            stage,
            fraction,
            message: message.into(),
            timestamp: now_unix(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OperationLogEntry {
    pub id: Option<i64>,
    pub ts: i64,
    pub level: LogLevel,
    pub operation: Option<Operation>,
    pub package_id: Option<PackageId>,
    pub message: String,
    pub detail: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::package::SourceKind;

    #[test]
    fn enums_round_trip() {
        for v in Operation::VARIANTS {
            assert_eq!(v.to_string().parse::<Operation>().unwrap(), *v);
            assert_eq!(serde_json::to_value(v).unwrap(), v.to_string());
        }
        for v in OperationStage::VARIANTS {
            assert_eq!(v.to_string().parse::<OperationStage>().unwrap(), *v);
            assert_eq!(serde_json::to_value(v).unwrap(), v.to_string());
        }
        for v in LogLevel::VARIANTS {
            assert_eq!(v.to_string().parse::<LogLevel>().unwrap(), *v);
            assert_eq!(serde_json::to_value(v).unwrap(), v.to_string());
        }
        assert_eq!(Operation::VARIANTS.len(), 3);
        assert_eq!(OperationStage::VARIANTS.len(), 8);
        assert_eq!(LogLevel::VARIANTS.len(), 4);
        assert!("x".parse::<Operation>().is_err());
        assert!("x".parse::<OperationStage>().is_err());
        assert!("x".parse::<LogLevel>().is_err());
    }

    #[test]
    fn terminal_stages() {
        for s in OperationStage::VARIANTS {
            let expect = matches!(
                s,
                OperationStage::Done | OperationStage::Failed | OperationStage::Cancelled
            );
            assert_eq!(s.is_terminal(), expect, "{s}");
        }
    }

    #[test]
    fn fraction_is_clamped() {
        let mk = |f| {
            ProgressEvent::new(
                "op1",
                PackageId::new(SourceKind::Scoop, "x"),
                Operation::Install,
                OperationStage::Downloading,
                f,
                "m",
            )
        };
        assert_eq!(mk(-0.5).fraction, 0.0);
        assert_eq!(mk(1.7).fraction, 1.0);
        assert_eq!(mk(0.25).fraction, 0.25);
        assert_eq!(mk(f32::NAN).fraction, 0.0);
        assert!(mk(0.5).timestamp > 1_700_000_000);
    }
}
