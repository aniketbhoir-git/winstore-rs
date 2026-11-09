//! Event names and payload types pushed from the core to the frontend (SPEC §1.2, §6).

use std::sync::Arc;

use serde::Serialize;

use crate::domain::{PackageId, PackageSummary, ProgressEvent};
use crate::error::AppError;

/// One `ProgressEvent` per message; payload is the `ProgressEvent` itself.
pub const EVENT_PROGRESS: &str = "store:progress";
/// Search results for one `search_id` (`SearchResultsEvent`).
pub const EVENT_SEARCH_RESULTS: &str = "store:search-results";
/// Search finished with an error for one `search_id` (`SearchFailedEvent`).
pub const EVENT_SEARCH_FAILED: &str = "store:search-failed";
/// An install / update ended; carries the typed error on failure (`OperationFinishedEvent`).
pub const EVENT_OPERATION_FINISHED: &str = "store:operation-finished";
/// Summary at the end of `update_all` (`UpdatesSummaryEvent`).
pub const EVENT_UPDATES_SUMMARY: &str = "store:updates-summary";

/// Where events go. Tauri in the app, a recorder in tests.
pub trait EventSink: Send + Sync {
    fn emit_json(&self, event: &str, payload: serde_json::Value);
}

pub type SharedSink = Arc<dyn EventSink>;

/// Serializes `payload` and sends it; a serialization failure is logged, never fatal.
pub fn emit<T: Serialize>(sink: &dyn EventSink, event: &str, payload: &T) {
    match serde_json::to_value(payload) {
        Ok(v) => sink.emit_json(event, v),
        Err(e) => tracing::error!(event, "could not serialize event payload: {e}"),
    }
}

/// The `{code, message, detail, retryable}` JSON the frontend already gets from failed commands.
pub fn error_json(e: &AppError) -> serde_json::Value {
    serde_json::to_value(e).unwrap_or(serde_json::Value::Null)
}

pub fn emit_progress(sink: &dyn EventSink, ev: &ProgressEvent) {
    emit(sink, EVENT_PROGRESS, ev);
}

#[derive(Debug, Clone, Serialize)]
pub struct SearchResultsEvent {
    pub search_id: String,
    pub results: Vec<PackageSummary>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SearchFailedEvent {
    pub search_id: String,
    pub error: serde_json::Value,
}

#[derive(Debug, Serialize)]
pub struct OperationFinishedEvent {
    pub operation_id: String,
    pub package_id: PackageId,
    pub ok: bool,
    pub error: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize)]
pub struct UpdateOutcome {
    pub package_id: PackageId,
    pub ok: bool,
    pub message: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct UpdatesSummaryEvent {
    pub total: usize,
    pub succeeded: usize,
    pub failed: usize,
    pub outcomes: Vec<UpdateOutcome>,
}

#[cfg(test)]
pub(crate) mod testing {
    use std::sync::Mutex;

    use super::EventSink;

    /// Records every emitted event for assertions.
    #[derive(Default)]
    pub struct RecordingSink {
        pub events: Mutex<Vec<(String, serde_json::Value)>>,
    }

    impl EventSink for RecordingSink {
        fn emit_json(&self, event: &str, payload: serde_json::Value) {
            self.events
                .lock()
                .unwrap()
                .push((event.to_string(), payload));
        }
    }

    impl RecordingSink {
        pub fn named(&self, name: &str) -> Vec<serde_json::Value> {
            self.events
                .lock()
                .unwrap()
                .iter()
                .filter(|(n, _)| n == name)
                .map(|(_, p)| p.clone())
                .collect()
        }
    }
}
