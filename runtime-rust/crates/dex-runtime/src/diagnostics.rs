//! Runtime diagnostics, missing API tracking, exception telemetry, and execution analysis.

use std::sync::Mutex;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiagnosticSeverity {
    Info,
    Warning,
    MissingApi,
    RuntimeError,
    FatalCrash,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DiagnosticRecord {
    pub severity: DiagnosticSeverity,
    pub component: String,
    pub message: String,
    pub details: Option<String>,
}

/// Central diagnostics aggregator recording execution metrics, missing classes/APIs, and crashes.
#[derive(Debug, Default)]
pub struct RuntimeDiagnostics {
    records: Mutex<Vec<DiagnosticRecord>>,
    missing_classes: Mutex<Vec<String>>,
    missing_methods: Mutex<Vec<String>>,
}

impl RuntimeDiagnostics {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn record(&self, severity: DiagnosticSeverity, component: &str, message: &str, details: Option<&str>) {
        if let Ok(mut recs) = self.records.lock() {
            recs.push(DiagnosticRecord {
                severity,
                component: component.to_string(),
                message: message.to_string(),
                details: details.map(|s| s.to_string()),
            });
        }
    }

    pub fn record_missing_class(&self, descriptor: &str) {
        if let Ok(mut mc) = self.missing_classes.lock() {
            if !mc.contains(&descriptor.to_string()) {
                mc.push(descriptor.to_string());
            }
        }
        self.record(
            DiagnosticSeverity::MissingApi,
            "ClassLoader",
            &format!("Reference to unlinked class: {}", descriptor),
            None,
        );
    }

    pub fn record_missing_method(&self, class: &str, name: &str, proto: &str) {
        let sig = format!("{}->{}{}", class, name, proto);
        if let Ok(mut mm) = self.missing_methods.lock() {
            if !mm.contains(&sig) {
                mm.push(sig.clone());
            }
        }
        self.record(
            DiagnosticSeverity::MissingApi,
            "MethodResolver",
            &format!("Invocation of unresolved method: {}", sig),
            None,
        );
    }

    pub fn count_by_severity(&self, severity: DiagnosticSeverity) -> usize {
        self.records.lock().map(|r| r.iter().filter(|rec| rec.severity == severity).count()).unwrap_or(0)
    }

    pub fn missing_class_count(&self) -> usize {
        self.missing_classes.lock().map(|mc| mc.len()).unwrap_or(0)
    }

    pub fn missing_method_count(&self) -> usize {
        self.missing_methods.lock().map(|mm| mm.len()).unwrap_or(0)
    }

    pub fn format_summary(&self) -> String {
        let warnings = self.count_by_severity(DiagnosticSeverity::Warning);
        let missing = self.count_by_severity(DiagnosticSeverity::MissingApi);
        let errors = self.count_by_severity(DiagnosticSeverity::RuntimeError)
            + self.count_by_severity(DiagnosticSeverity::FatalCrash);
        format!(
            "Diagnostics: {} missing APIs ({} classes, {} methods), {} warnings, {} runtime errors",
            missing,
            self.missing_class_count(),
            self.missing_method_count(),
            warnings,
            errors
        )
    }
}
