//! Bounded, low-overhead runtime diagnostics and session event history.

use std::collections::VecDeque;
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

/// Maximum retained diagnostic events per runtime session.
pub const DEFAULT_EVENT_CAPACITY: usize = 2048;
/// Maximum unique unresolved symbols retained in each summary list.
pub const DEFAULT_MISSING_SYMBOL_CAPACITY: usize = 512;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiagnosticSeverity {
    Info,
    Warning,
    MissingApi,
    RuntimeError,
    FatalCrash,
}

impl DiagnosticSeverity {
    fn as_str(self) -> &'static str {
        match self {
            Self::Info => "INFO",
            Self::Warning => "WARNING",
            Self::MissingApi => "MISSING_API",
            Self::RuntimeError => "ERROR",
            Self::FatalCrash => "FATAL",
        }
    }
}

/// One timestamped event. Timestamp is Unix time in milliseconds.
#[derive(Debug, Clone, PartialEq)]
pub struct DiagnosticRecord {
    pub sequence: u64,
    pub timestamp_ms: u128,
    pub severity: DiagnosticSeverity,
    pub component: String,
    pub code: Option<String>,
    pub message: String,
    pub details: Option<String>,
    pub recovery: Option<String>,
}

#[derive(Debug, Default)]
struct DiagnosticState {
    records: VecDeque<DiagnosticRecord>,
    missing_classes: Vec<String>,
    missing_methods: Vec<String>,
    next_sequence: u64,
    dropped_events: u64,
}

/// Thread-safe bounded event recorder. Recording is event-driven: no polling or
/// background worker is created. Oldest events are evicted at the capacity limit.
#[derive(Debug)]
pub struct RuntimeDiagnostics {
    state: Mutex<DiagnosticState>,
    event_capacity: usize,
    missing_symbol_capacity: usize,
}

impl Default for RuntimeDiagnostics {
    fn default() -> Self {
        Self::with_capacity(DEFAULT_EVENT_CAPACITY, DEFAULT_MISSING_SYMBOL_CAPACITY)
    }
}

impl RuntimeDiagnostics {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_capacity(event_capacity: usize, missing_symbol_capacity: usize) -> Self {
        Self {
            state: Mutex::new(DiagnosticState::default()),
            event_capacity: event_capacity.max(1),
            missing_symbol_capacity: missing_symbol_capacity.max(1),
        }
    }

    pub fn record(&self, severity: DiagnosticSeverity, component: &str, message: &str, details: Option<&str>) {
        self.record_internal(severity, component, None, message, details, None);
    }

    /// Records a classified fault with a stable code and a concrete recovery hint.
    /// The extra strings are only formatted during export, not on execution hot paths.
    pub fn record_detailed(
        &self,
        severity: DiagnosticSeverity,
        component: &str,
        code: &str,
        message: &str,
        details: Option<&str>,
        recovery: Option<&str>,
    ) {
        self.record_internal(severity, component, Some(code), message, details, recovery);
    }

    fn record_internal(
        &self,
        severity: DiagnosticSeverity,
        component: &str,
        code: Option<&str>,
        message: &str,
        details: Option<&str>,
        recovery: Option<&str>,
    ) {
        let timestamp_ms = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis();
        if let Ok(mut state) = self.state.lock() {
            let sequence = state.next_sequence;
            state.next_sequence = state.next_sequence.saturating_add(1);
            if state.records.len() == self.event_capacity {
                state.records.pop_front();
                state.dropped_events = state.dropped_events.saturating_add(1);
            }
            state.records.push_back(DiagnosticRecord {
                sequence,
                timestamp_ms,
                severity,
                component: component.to_owned(),
                code: code.map(str::to_owned),
                message: message.to_owned(),
                details: details.map(str::to_owned),
                recovery: recovery.map(str::to_owned),
            });
        }
    }

    pub fn record_missing_class(&self, descriptor: &str) {
        let inserted = if let Ok(mut state) = self.state.lock() {
            if state.missing_classes.iter().any(|item| item == descriptor) {
                false
            } else if state.missing_classes.len() < self.missing_symbol_capacity {
                state.missing_classes.push(descriptor.to_owned());
                true
            } else {
                false
            }
        } else {
            false
        };
        if inserted {
            self.record(
                DiagnosticSeverity::MissingApi,
                "ClassLoader",
                &format!("Reference to unlinked class: {}", descriptor),
                None,
            );
        }
    }

    pub fn record_missing_method(&self, class: &str, name: &str, proto: &str) {
        let sig = format!("{}->{}{}", class, name, proto);
        let inserted = if let Ok(mut state) = self.state.lock() {
            if state.missing_methods.iter().any(|item| item == &sig) {
                false
            } else if state.missing_methods.len() < self.missing_symbol_capacity {
                state.missing_methods.push(sig.clone());
                true
            } else {
                false
            }
        } else {
            false
        };
        if inserted {
            self.record(
                DiagnosticSeverity::MissingApi,
                "MethodResolver",
                &format!("Invocation of unresolved method: {}", sig),
                None,
            );
        }
    }

    pub fn snapshot(&self) -> Vec<DiagnosticRecord> {
        self.state.lock()
            .map(|state| state.records.iter().cloned().collect())
            .unwrap_or_default()
    }

    pub fn dropped_event_count(&self) -> u64 {
        self.state.lock().map(|state| state.dropped_events).unwrap_or(0)
    }

    pub fn event_count(&self) -> usize {
        self.state.lock().map(|state| state.records.len()).unwrap_or(0)
    }

    pub fn count_by_severity(&self, severity: DiagnosticSeverity) -> usize {
        self.state.lock()
            .map(|state| state.records.iter().filter(|record| record.severity == severity).count())
            .unwrap_or(0)
    }

    pub fn missing_class_count(&self) -> usize {
        self.state.lock().map(|state| state.missing_classes.len()).unwrap_or(0)
    }

    pub fn missing_method_count(&self) -> usize {
        self.state.lock().map(|state| state.missing_methods.len()).unwrap_or(0)
    }

    /// Export a stable, human-readable snapshot. Formatting happens only when
    /// requested, not on the runtime's hot execution path.
    pub fn export_text(&self) -> String {
        let (records, dropped, classes, methods) = match self.state.lock() {
            Ok(state) => (
                state.records.iter().cloned().collect::<Vec<_>>(),
                state.dropped_events,
                state.missing_classes.clone(),
                state.missing_methods.clone(),
            ),
            Err(_) => return "STRIDE SESSION DIAGNOSTICS\nERROR: diagnostics lock poisoned\n".into(),
        };

        let mut output = String::from("STRIDE SESSION DIAGNOSTICS\n");
        let count = |severity: DiagnosticSeverity| records.iter().filter(|event| event.severity == severity).count();
        output.push_str("SESSION SUMMARY\n");
        output.push_str(&format!(
            "Retained events: {}\nDropped older events: {}\nInfo: {}\nWarnings: {}\nMissing API events: {}\nRuntime errors: {}\nFatal events: {}\nMissing classes: {}\nMissing methods: {}\n",
            records.len(), dropped,
            count(DiagnosticSeverity::Info),
            count(DiagnosticSeverity::Warning),
            count(DiagnosticSeverity::MissingApi),
            count(DiagnosticSeverity::RuntimeError),
            count(DiagnosticSeverity::FatalCrash),
            classes.len(), methods.len()
        ));
        output.push_str("\nEVENT TIMELINE\n");
        for event in records {
            let code = event.code.as_deref()
                .map(|value| format!(" [{}]", value))
                .unwrap_or_default();
            output.push_str(&format!(
                "[{}] #{} [{}] {}{}: {}",
                event.timestamp_ms, event.sequence, event.severity.as_str(),
                event.component, code, event.message
            ));
            if let Some(details) = event.details {
                output.push_str(&format!(" | {}", details));
            }
            if let Some(recovery) = event.recovery {
                output.push_str(&format!(" | Recovery: {}", recovery));
            }
            output.push('\n');
        }
        output.push_str("\nMISSING CLASSES\n");
        for item in classes { output.push_str(&format!("{}\n", item)); }
        output.push_str("\nMISSING METHODS\n");
        for item in methods { output.push_str(&format!("{}\n", item)); }
        output
    }

    pub fn format_summary(&self) -> String {
        let warnings = self.count_by_severity(DiagnosticSeverity::Warning);
        let missing = self.count_by_severity(DiagnosticSeverity::MissingApi);
        let errors = self.count_by_severity(DiagnosticSeverity::RuntimeError)
            + self.count_by_severity(DiagnosticSeverity::FatalCrash);
        format!(
            "Diagnostics: {} missing API events ({} classes, {} methods), {} warnings, {} runtime errors, {} retained events, {} dropped events",
            missing,
            self.missing_class_count(),
            self.missing_method_count(),
            warnings,
            errors,
            self.event_count(),
            self.dropped_event_count()
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn event_history_is_bounded_and_reports_evictions() {
        let diagnostics = RuntimeDiagnostics::with_capacity(2, 2);
        diagnostics.record(DiagnosticSeverity::Info, "Import", "started", None);
        diagnostics.record(DiagnosticSeverity::Info, "DEX", "scanned", None);
        diagnostics.record(DiagnosticSeverity::RuntimeError, "DEX", "rejected", Some("bad header"));
        let events = diagnostics.snapshot();
        assert_eq!(events.len(), 2);
        assert_eq!(events[0].message, "scanned");
        assert_eq!(events[1].message, "rejected");
        assert_eq!(diagnostics.dropped_event_count(), 1);
        assert!(diagnostics.export_text().contains("bad header"));
    }

    #[test]
    fn missing_symbols_are_deduplicated_and_capped() {
        let diagnostics = RuntimeDiagnostics::with_capacity(8, 1);
        diagnostics.record_missing_class("Lmissing/One;");
        diagnostics.record_missing_class("Lmissing/One;");
        diagnostics.record_missing_class("Lmissing/Two;");
        assert_eq!(diagnostics.missing_class_count(), 1);
        assert_eq!(diagnostics.count_by_severity(DiagnosticSeverity::MissingApi), 1);
    }

    #[test]
    fn records_have_monotonic_sequence_numbers() {
        let diagnostics = RuntimeDiagnostics::new();
        diagnostics.record(DiagnosticSeverity::Info, "A", "one", None);
        diagnostics.record(DiagnosticSeverity::Warning, "B", "two", None);
        let events = diagnostics.snapshot();
        assert!(events[0].timestamp_ms > 0);
        assert_eq!(events[1].sequence, events[0].sequence + 1);
    }
    #[test]
    fn export_summary_includes_severity_and_missing_symbol_totals() {
        let diagnostics = RuntimeDiagnostics::new();
        diagnostics.record(DiagnosticSeverity::Warning, "Archive", "split warning", None);
        diagnostics.record(DiagnosticSeverity::RuntimeError, "DEX", "interpreter failed", None);
        diagnostics.record(DiagnosticSeverity::FatalCrash, "Native", "guest panic", None);
        diagnostics.record_missing_class("Lmissing/Screen;");
        diagnostics.record_missing_method("Lmissing/Screen;", "draw", "()V");
        let report = diagnostics.export_text();
        assert!(report.contains("Warnings: 1"));
        assert!(report.contains("Missing API events: 2"));
        assert!(report.contains("Runtime errors: 1"));
        assert!(report.contains("Fatal events: 1"));
        assert!(report.contains("Missing classes: 1"));
        assert!(report.contains("Missing methods: 1"));
    }

    #[test]
    fn detailed_faults_export_code_details_and_recovery() {
        let diagnostics = RuntimeDiagnostics::new();
        diagnostics.record_detailed(
            DiagnosticSeverity::RuntimeError,
            "Manifest",
            "AXML_STRING_OFFSET_INVALID",
            "Manifest parsing failed",
            Some("string index 7 points outside the pool"),
            Some("Verify string-pool offsets against the containing chunk"),
        );
        let report = diagnostics.export_text();
        assert!(report.contains("Manifest [AXML_STRING_OFFSET_INVALID]: Manifest parsing failed"));
        assert!(report.contains("string index 7 points outside the pool"));
        assert!(report.contains("Recovery: Verify string-pool offsets"));
    }

}
