//! APK Application Launcher and Runtime Execution Coordinator.
//!
//! Integrates Manifest parsing, Multi-DEX loading, runtime initialization,
//! class linking, Activity lifecycle startup, and actual bytecode execution.

use std::sync::Arc;
use stride_axml::parse_manifest;
use crate::loader::AppClassLoader;
use crate::hierarchy::ClassHierarchy;
use crate::heap::ManagedHeap;
use crate::sync::MonitorTable;
use crate::jni::JniNativeRegistry;
use crate::framework::{AndroidEnvironment, ActivityLifecycleState, PackageInfo, ActivityInfo, ServiceInfo};
use crate::graphics::{Surface, InputQueue, DisplayMetrics};
use crate::interpreter::{ArtVm, ExecutionStopReason};
use crate::types::Value;
use crate::diagnostics::{RuntimeDiagnostics, DiagnosticSeverity};
use crate::bionic::NativeLinker;
use crate::gl::OpenGlEsEngine;
use crate::activity_manager::ActivityManager;
use crate::service_manager::ServiceManager;
use crate::telemetry::PerformanceTelemetry;

/// Diagnostic execution report returned after launching an APK.
#[derive(Debug, Clone, PartialEq)]
pub struct LaunchReport {
    pub package_name: String,
    pub launcher_activity: String,
    pub lifecycle_state: ActivityLifecycleState,
    pub total_classes_loaded: usize,
    pub instructions_executed: usize,
    pub status: LaunchStatus,
    pub diagnostics_summary: String,
}

#[derive(Debug, Clone, PartialEq)]
pub enum LaunchStatus {
    Running,
    FinishedNormally,
    UncaughtException { class: String, message: Option<String> },
    ActivityNotFound(String),
    ManifestError(String),
}

/// The runtime instance managing the lifecycle of an executed Android APK.
pub struct AndroidRuntime {
    pub class_loader: AppClassLoader,
    pub heap: ManagedHeap,
    pub hierarchy: ClassHierarchy,
    pub monitors: MonitorTable,
    pub jni: JniNativeRegistry,
    pub environment: AndroidEnvironment,
    pub surface: Surface,
    pub input_queue: Arc<InputQueue>,
    pub display_metrics: DisplayMetrics,
    pub active_activity_obj: Option<u32>,
    pub active_activity_class: Option<String>,
    pub diagnostics: Arc<RuntimeDiagnostics>,
    pub native_linker: NativeLinker,
    pub gl_engine: OpenGlEsEngine,
    pub activity_manager: ActivityManager,
    pub service_manager: ServiceManager,
    pub telemetry: PerformanceTelemetry,
}

impl AndroidRuntime {
    /// Creates a new Android runtime environment.
    pub fn new(package_name: &str) -> Self {
        let mut jni = JniNativeRegistry::new();
        Self::register_core_jni(&mut jni);

        Self {
            class_loader: AppClassLoader::new(),
            heap: ManagedHeap::default(),
            hierarchy: ClassHierarchy::new(),
            monitors: MonitorTable::new(),
            jni,
            environment: AndroidEnvironment::new(package_name),
            surface: Surface::new(1080, 1920),
            input_queue: Arc::new(InputQueue::new()),
            display_metrics: DisplayMetrics::default(),
            active_activity_obj: None,
            active_activity_class: None,
            diagnostics: Arc::new(RuntimeDiagnostics::new()),
            native_linker: NativeLinker::new(),
            gl_engine: OpenGlEsEngine::new(),
            activity_manager: ActivityManager::new(),
            service_manager: ServiceManager::new(),
            telemetry: PerformanceTelemetry::default(),
        }
    }

    /// Registers essential Android & Java core native methods.
    fn register_core_jni(jni: &mut JniNativeRegistry) {
        // Activity lifecycle methods are framework-owned and therefore do not
        // have app DEX MethodCode entries. The launcher owns lifecycle state
        // transitions; this bridge safely accepts the Activity's super call.
        jni.register(
            "Landroid/app/Activity;",
            "onCreate",
            "(Landroid/os/Bundle;)V",
            |_env, _args| Ok(None),
        );

        jni.register(
            "Ljava/lang/System;",
            "currentTimeMillis",
            "()J",
            |_env, _args| {
                use std::time::{SystemTime, UNIX_EPOCH};
                let now = SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_millis() as i64;
                Ok(Some(Value::Long(now)))
            },
        );

        jni.register(
            "Ljava/lang/System;",
            "nanoTime",
            "()J",
            |_env, _args| {
                use std::time::{SystemTime, UNIX_EPOCH};
                let nanos = SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_nanos() as i64;
                Ok(Some(Value::Long(nanos)))
            },
        );

        jni.register(
            "Ljava/lang/Float;",
            "floatToRawIntBits",
            "(F)I",
            |_env, args| {
                let f = args.first().map(|v| v.as_float()).unwrap_or(0.0);
                Ok(Some(Value::Int(f.to_bits() as i32)))
            },
        );

        jni.register(
            "Ljava/lang/Float;",
            "intBitsToFloat",
            "(I)F",
            |_env, args| {
                let i = args.first().map(|v| v.as_int()).unwrap_or(0);
                Ok(Some(Value::Float(f32::from_bits(i as u32))))
            },
        );

        jni.register(
            "Ljava/lang/Double;",
            "doubleToRawLongBits",
            "(D)J",
            |_env, args| {
                let d = args.first().map(|v| v.as_double()).unwrap_or(0.0);
                Ok(Some(Value::Long(d.to_bits() as i64)))
            },
        );

        jni.register(
            "Ljava/lang/Double;",
            "longBitsToDouble",
            "(J)D",
            |_env, args| {
                let l = args.first().map(|v| v.as_long()).unwrap_or(0);
                Ok(Some(Value::Double(f64::from_bits(l as u64))))
            },
        );

        let log_handler = |_env: &mut crate::jni::JniEnv, _args: &[Value]| {
            Ok(Some(Value::Int(1)))
        };

        jni.register("Landroid/util/Log;", "i", "(Ljava/lang/String;Ljava/lang/String;)I", log_handler);
        jni.register("Landroid/util/Log;", "d", "(Ljava/lang/String;Ljava/lang/String;)I", log_handler);
        jni.register("Landroid/util/Log;", "e", "(Ljava/lang/String;Ljava/lang/String;)I", log_handler);
        jni.register("Landroid/util/Log;", "w", "(Ljava/lang/String;Ljava/lang/String;)I", log_handler);
        jni.register("Landroid/util/Log;", "v", "(Ljava/lang/String;Ljava/lang/String;)I", log_handler);
    }

    /// Loads one or more DEX binaries into the runtime.
    pub fn load_dex(&mut self, dex_data: &[u8]) -> Result<usize, String> {
        self.diagnostics.record(
            DiagnosticSeverity::Info, "DexLoader",
            &format!("DEX load started ({} bytes)", dex_data.len()), None,
        );
        let count = match self.class_loader.load_dex_with_diagnostics(dex_data, &self.diagnostics) {
            Ok(count) => count,
            Err(error) => {
                self.diagnostics.record_detailed(
                    DiagnosticSeverity::RuntimeError, "DexLoader", "DEX_LOAD_FAILED",
                    "DEX load failed", Some(&error),
                    Some("Check DEX header, checksum, map offsets and class-data/code-item bounds"),
                );
                return Err(error);
            }
        };
        self.class_loader.link_hierarchy(&mut self.hierarchy);
        self.diagnostics.record(
            DiagnosticSeverity::Info, "DexLoader",
            &format!("DEX load completed ({} classes)", count), None,
        );
        Ok(count)
    }

    /// Loads and links an ELF shared library (.so) into the native environment.
    pub fn load_native_library(&mut self, soname: &str, elf_bytes: &[u8]) -> Result<(), String> {
        self.diagnostics.record(
            DiagnosticSeverity::Info, "NativeLinker",
            &format!("Native library load started: {} ({} bytes)", soname, elf_bytes.len()), None,
        );
        let module = match self.native_linker.load_library(soname, elf_bytes) {
            Ok(module) => module,
            Err(error) => {
                self.diagnostics.record_detailed(
                    DiagnosticSeverity::RuntimeError, "NativeLinker", "ELF_LOAD_FAILED",
                    "Native library inspection or load planning failed", Some(&error),
                    Some("Verify ELF class, machine ABI, program headers, dynamic table and dependencies"),
                );
                return Err(error);
            }
        };
        let count = module.exported_symbols.len();
        let details = format!(
            "architecture={:?}; exports={}; JNI_OnLoad_present={}; DT_NEEDED_entries={:?}",
            module.architecture, count, module.has_jni_onload, module.needed_libraries
        );
        self.diagnostics.record_detailed(
            DiagnosticSeverity::Warning, "NativeLinker", "ELF_PLAN_ONLY",
            "ELF was inspected and a memory plan was generated; native code was not mapped, relocated or executed",
            Some(&details),
            Some("Do not treat this as native execution support; memory mapping, relocations, symbol binding and JNI_OnLoad dispatch are still required"),
        );
        Ok(())
    }

    /// Launches an Android application package by reading its manifest and executing its main activity.
    pub fn launch_application(
        &mut self,
        manifest_bytes: &[u8],
        step_budget: usize,
    ) -> LaunchReport {
        self.diagnostics.record(DiagnosticSeverity::Info, "Launcher", "APK launch started", Some(&format!("manifest bytes={}, step budget={}", manifest_bytes.len(), step_budget)));
        let manifest = match parse_manifest(manifest_bytes) {
            Ok(m) => m,
            Err(e) => {
                self.diagnostics.record_detailed(
                    DiagnosticSeverity::RuntimeError, "Manifest", "AXML_PARSE_FAILED",
                    "Binary Android manifest parsing failed", Some(&format!("{:?}", e)),
                    Some("Inspect the reported AXML chunk/string-pool field; compare it with the real manifest fixture"),
                );
                return LaunchReport {
                    package_name: self.environment.package_name.clone(),
                    launcher_activity: String::new(),
                    lifecycle_state: ActivityLifecycleState::NotStarted,
                    total_classes_loaded: self.class_loader.classes.len(),
                    instructions_executed: 0,
                    status: LaunchStatus::ManifestError(format!("{:?}", e)),
                    diagnostics_summary: self.diagnostics.format_summary(),
                };
            }
        };

        if let Some(pkg) = manifest.package_name {
            self.environment.package_name = pkg.clone();
            self.environment.package_info = Some(PackageInfo {
                package_name: pkg,
                version_code: manifest.version_code.unwrap_or(1),
                version_name: manifest.version_name.unwrap_or_else(|| "1.0".into()),
                min_sdk: manifest.min_sdk.unwrap_or(21),
                target_sdk: manifest.target_sdk.unwrap_or(33),
                permissions: manifest.permissions,
                activities: manifest.components.iter().filter(|c| c.kind == "activity").map(|c| ActivityInfo {
                    name: c.name.clone(),
                    exported: c.exported.unwrap_or(false),
                    screen_orientation: 0,
                }).collect(),
                services: manifest.components.iter().filter(|c| c.kind == "service").map(|c| ServiceInfo {
                    name: c.name.clone(),
                    exported: c.exported.unwrap_or(false),
                    permission: None,
                }).collect(),
                application_class: manifest.application_name,
                launcher_activity: manifest.launcher_activity.clone(),
            });
        }

        let activity_class_raw = match manifest.launcher_activity {
            Some(a) => a,
            None => {
                self.diagnostics.record_detailed(
                    DiagnosticSeverity::RuntimeError, "Launcher", "LAUNCHER_ACTIVITY_MISSING",
                    "Manifest contains no MAIN/LAUNCHER activity",
                    Some("No launchable activity was extracted from AndroidManifest.xml"),
                    Some("Check the activity intent-filter for MAIN action and LAUNCHER category"),
                );
                return LaunchReport {
                    package_name: self.environment.package_name.clone(),
                    launcher_activity: String::new(),
                    lifecycle_state: ActivityLifecycleState::NotStarted,
                    total_classes_loaded: self.class_loader.classes.len(),
                    instructions_executed: 0,
                    status: LaunchStatus::ActivityNotFound("No launcher activity declared in AndroidManifest.xml".into()),
                    diagnostics_summary: self.diagnostics.format_summary(),
                };
            }
        };

        // Convert dot notation (com.example.MainActivity) to DEX descriptor (Lcom/example/MainActivity;)
        let activity_descriptor = if activity_class_raw.starts_with('L') && activity_class_raw.ends_with(';') {
            activity_class_raw.clone()
        } else {
            let replaced = activity_class_raw.replace('.', "/");
            format!("L{};", replaced)
        };

        // Run <clinit> on the activity class
        if let Err(error) = self.class_loader.initialize_class(
            &activity_descriptor,
            &mut self.heap,
            &mut self.hierarchy,
            &mut self.monitors,
            &mut self.jni,
            &[],
        ) {
            self.diagnostics.record_detailed(
                DiagnosticSeverity::RuntimeError, "ClassInitializer", "CLASS_INITIALIZATION_FAILED",
                "Launcher class static initialization failed", Some(&error),
                Some("Inspect <clinit>, static field references and unresolved superclass dependencies"),
            );
            return LaunchReport {
                package_name: self.environment.package_name.clone(),
                launcher_activity: activity_descriptor,
                lifecycle_state: ActivityLifecycleState::NotStarted,
                total_classes_loaded: self.class_loader.classes.len(),
                instructions_executed: 0,
                status: LaunchStatus::UncaughtException {
                    class: "ClassInitializationError".into(),
                    message: Some(error),
                },
                diagnostics_summary: self.diagnostics.format_summary(),
            };
        }

        // Allocate Activity object
        let activity_obj = match self.heap.allocate_instance(&activity_descriptor) {
            Ok(id) => id,
            Err(primary_error) => {
                self.diagnostics.record_detailed(
                    DiagnosticSeverity::Warning, "Heap", "ACTIVITY_ALLOCATION_FALLBACK",
                    "Could not allocate the app Activity class; trying generic Activity fallback",
                    Some(&format!("{:?}", primary_error)),
                    Some("Check managed heap usage and whether the Activity class was linked"),
                );
                match self.heap.allocate_instance("Landroid/app/Activity;") {
                    Ok(id) => id,
                    Err(fallback_error) => {
                        self.diagnostics.record_detailed(
                            DiagnosticSeverity::FatalCrash, "Heap", "ACTIVITY_ALLOCATION_FAILED",
                            "Activity allocation failed, including the generic fallback",
                            Some(&format!("primary={:?}; fallback={:?}", primary_error, fallback_error)),
                            Some("Stop this launch session; inspect heap limit, allocation accounting and object growth"),
                        );
                        return LaunchReport {
                            package_name: self.environment.package_name.clone(),
                            launcher_activity: activity_descriptor,
                            lifecycle_state: ActivityLifecycleState::NotStarted,
                            total_classes_loaded: self.class_loader.classes.len(),
                            instructions_executed: 0,
                            status: LaunchStatus::UncaughtException {
                                class: "OutOfMemoryError".into(),
                                message: Some(format!("{:?}", fallback_error)),
                            },
                            diagnostics_summary: self.diagnostics.format_summary(),
                        };
                    }
                }
            }
        };

        self.active_activity_obj = Some(activity_obj);
        self.active_activity_class = Some(activity_descriptor.clone());
        self.environment.transition_activity(&activity_descriptor, ActivityLifecycleState::Created);
        self.diagnostics.record(DiagnosticSeverity::Info, "Lifecycle", "Activity created", Some(&activity_descriptor));

        // Find and execute onCreate(Landroid/os/Bundle;)V if present
        let mut total_steps = 0;
        let on_create_mid = self.class_loader.find_method(
            &activity_descriptor,
            "onCreate",
            "(Landroid/os/Bundle;)V",
        );

        if let Some(mid) = on_create_mid {
            if let Ok(mid16) = u16::try_from(mid) {
                let method_map = self.class_loader.export_method_codes();
                let mut vm = ArtVm::new(
                    &mut self.heap,
                    &mut self.hierarchy,
                    &mut self.monitors,
                    &mut self.jni,
                    &method_map,
                    &[],
                );
                vm.attach_diagnostics(&self.diagnostics);
                if let Some(inventory) = self.class_loader.dex_inventories.first() {
                    vm.attach_method_resolver(&inventory.method_references, &self.class_loader.method_lookup);
                }
                // Call onCreate with this=activity_obj, savedInstanceState=null
                let args = [Value::Object(activity_obj), Value::Null];
                match vm.execute_method(mid16, &args, step_budget) {
                    Ok(report) => {
                        total_steps += report.steps_executed;
                        self.diagnostics.record_detailed(
                            DiagnosticSeverity::Info, "ART", "ONCREATE_EXECUTED",
                            "Activity onCreate execution returned",
                            Some(&format!("steps={} stop={:?}", report.steps_executed, report.stop_reason)),
                            None,
                        );
                        match report.stop_reason {
                            ExecutionStopReason::UncaughtException(ex) => {
                                self.diagnostics.record_detailed(
                                    DiagnosticSeverity::RuntimeError, "ART", "ONCREATE_UNCAUGHT_EXCEPTION",
                                    "Activity onCreate terminated with an uncaught exception",
                                    Some(&format!("{:?}", ex)),
                                    Some("Inspect the exception class, message and captured DEX stack trace"),
                                );
                                return LaunchReport {
                                    package_name: self.environment.package_name.clone(),
                                    launcher_activity: activity_descriptor,
                                    lifecycle_state: ActivityLifecycleState::Created,
                                    total_classes_loaded: self.class_loader.classes.len(),
                                    instructions_executed: total_steps,
                                    status: LaunchStatus::UncaughtException {
                                        class: ex.class_descriptor,
                                        message: ex.message,
                                    },
                                    diagnostics_summary: self.diagnostics.format_summary(),
                                };
                            }
                            ExecutionStopReason::StepLimitExceeded => {
                                self.diagnostics.record_detailed(
                                    DiagnosticSeverity::RuntimeError, "ART", "ONCREATE_STEP_LIMIT",
                                    "Activity onCreate exceeded the instruction budget",
                                    Some(&format!("executed={} budget={}", report.steps_executed, step_budget)),
                                    Some("Inspect the current method for an infinite loop or unusually expensive initialization before raising the budget"),
                                );
                                return LaunchReport {
                                    package_name: self.environment.package_name.clone(),
                                    launcher_activity: activity_descriptor,
                                    lifecycle_state: ActivityLifecycleState::Created,
                                    total_classes_loaded: self.class_loader.classes.len(),
                                    instructions_executed: total_steps,
                                    status: LaunchStatus::UncaughtException {
                                        class: "InstructionBudgetExceeded".into(),
                                        message: Some(format!("onCreate exceeded the {} instruction budget", step_budget)),
                                    },
                                    diagnostics_summary: self.diagnostics.format_summary(),
                                };
                            }
                            _ => {}
                        }
                    }
                    Err(error) => {
                        self.diagnostics.record_detailed(
                            DiagnosticSeverity::RuntimeError, "ART", "ONCREATE_INTERPRETER_ERROR",
                            "ART interpreter failed while executing Activity onCreate",
                            Some(&format!("method_id={} error={}", mid, error)),
                            Some("Inspect the failing opcode, register indices, method reference mapping and DEX code-item bounds"),
                        );
                        return LaunchReport {
                            package_name: self.environment.package_name.clone(),
                            launcher_activity: activity_descriptor,
                            lifecycle_state: ActivityLifecycleState::Created,
                            total_classes_loaded: self.class_loader.classes.len(),
                            instructions_executed: total_steps,
                            status: LaunchStatus::UncaughtException {
                                class: "InterpreterError".into(),
                                message: Some(error),
                            },
                            diagnostics_summary: self.diagnostics.format_summary(),
                        };
                    }
                }
            }
        }

        // Transition through onStart() and onResume()
        self.environment.transition_activity(&activity_descriptor, ActivityLifecycleState::Started);
        self.diagnostics.record(DiagnosticSeverity::Info, "Lifecycle", "Activity started", Some(&activity_descriptor));
        self.environment.transition_activity(&activity_descriptor, ActivityLifecycleState::Resumed);
        self.diagnostics.record(DiagnosticSeverity::Info, "Lifecycle", "Activity resumed", Some(&activity_descriptor));

        // Check for onResume()V
        let on_resume_mid = self.class_loader.find_method(&activity_descriptor, "onResume", "()V");
        if let Some(mid) = on_resume_mid {
            if let Ok(mid16) = u16::try_from(mid) {
                let method_map = self.class_loader.export_method_codes();
                let mut vm = ArtVm::new(
                    &mut self.heap,
                    &mut self.hierarchy,
                    &mut self.monitors,
                    &mut self.jni,
                    &method_map,
                    &[],
                );
                vm.attach_diagnostics(&self.diagnostics);
                let args = [Value::Object(activity_obj)];
                match vm.execute_method(mid16, &args, step_budget.saturating_sub(total_steps)) {
                    Ok(report) => {
                        total_steps += report.steps_executed;
                        self.diagnostics.record_detailed(
                            DiagnosticSeverity::Info, "ART", "ONRESUME_EXECUTED",
                            "Activity onResume execution returned",
                            Some(&format!("steps={} stop={:?}", report.steps_executed, report.stop_reason)),
                            None,
                        );
                        match report.stop_reason {
                            ExecutionStopReason::UncaughtException(ex) => {
                                self.diagnostics.record_detailed(
                                    DiagnosticSeverity::RuntimeError, "ART", "ONRESUME_UNCAUGHT_EXCEPTION",
                                    "Activity onResume terminated with an uncaught exception",
                                    Some(&format!("{:?}", ex)),
                                    Some("Inspect the exception class, message and captured DEX stack trace"),
                                );
                                return LaunchReport {
                                    package_name: self.environment.package_name.clone(),
                                    launcher_activity: activity_descriptor,
                                    lifecycle_state: ActivityLifecycleState::Resumed,
                                    total_classes_loaded: self.class_loader.classes.len(),
                                    instructions_executed: total_steps,
                                    status: LaunchStatus::UncaughtException {
                                        class: ex.class_descriptor,
                                        message: ex.message,
                                    },
                                    diagnostics_summary: self.diagnostics.format_summary(),
                                };
                            }
                            ExecutionStopReason::StepLimitExceeded => {
                                self.diagnostics.record_detailed(
                                    DiagnosticSeverity::RuntimeError, "ART", "ONRESUME_STEP_LIMIT",
                                    "Activity onResume exceeded the remaining instruction budget",
                                    Some(&format!("executed={} remaining={}", report.steps_executed, step_budget.saturating_sub(total_steps))),
                                    Some("Inspect onResume for repeated work or a loop; do not increase the budget without confirming progress"),
                                );
                                return LaunchReport {
                                    package_name: self.environment.package_name.clone(),
                                    launcher_activity: activity_descriptor,
                                    lifecycle_state: ActivityLifecycleState::Resumed,
                                    total_classes_loaded: self.class_loader.classes.len(),
                                    instructions_executed: total_steps,
                                    status: LaunchStatus::UncaughtException {
                                        class: "InstructionBudgetExceeded".into(),
                                        message: Some(format!("onResume exceeded the remaining instruction budget ({})", step_budget.saturating_sub(total_steps))),
                                    },
                                    diagnostics_summary: self.diagnostics.format_summary(),
                                };
                            }
                            _ => {}
                        }
                    }
                    Err(error) => {
                        self.diagnostics.record_detailed(
                            DiagnosticSeverity::RuntimeError, "ART", "ONRESUME_INTERPRETER_ERROR",
                            "ART interpreter failed while executing Activity onResume",
                            Some(&format!("method_id={} error={}", mid, error)),
                            Some("Inspect the failing opcode, register indices, method reference mapping and DEX code-item bounds"),
                        );
                        return LaunchReport {
                            package_name: self.environment.package_name.clone(),
                            launcher_activity: activity_descriptor,
                            lifecycle_state: ActivityLifecycleState::Resumed,
                            total_classes_loaded: self.class_loader.classes.len(),
                            instructions_executed: total_steps,
                            status: LaunchStatus::UncaughtException {
                                class: "InterpreterError".into(),
                                message: Some(error),
                            },
                            diagnostics_summary: self.diagnostics.format_summary(),
                        };
                    }
                }
            }
        }

        self.diagnostics.record_detailed(
            DiagnosticSeverity::Info, "Launcher", "LAUNCH_PIPELINE_RETURNED",
            "APK launch pipeline reached the resumed lifecycle state",
            Some(&format!(
                "instructions={} classes={} heap_bytes={} missing_api_events={}",
                total_steps,
                self.class_loader.classes.len(),
                self.heap.allocated_bytes(),
                self.diagnostics.count_by_severity(DiagnosticSeverity::MissingApi)
            )),
            Some("This confirms lifecycle dispatch only; it does not prove a persistent app session or rendered frames"),
        );
        LaunchReport {
            package_name: self.environment.package_name.clone(),
            launcher_activity: activity_descriptor,
            lifecycle_state: ActivityLifecycleState::Resumed,
            total_classes_loaded: self.class_loader.classes.len(),
            instructions_executed: total_steps,
            status: LaunchStatus::Running,
            diagnostics_summary: self.diagnostics.format_summary(),
        }
    }
}


#[cfg(test)]
mod framework_bridge_tests {
    use super::*;

    #[test]
    fn activity_on_create_super_call_has_framework_bridge() {
        let runtime = AndroidRuntime::new("com.stride.test");
        assert!(
            runtime.jni.find(
                "Landroid/app/Activity;",
                "onCreate",
                "(Landroid/os/Bundle;)V"
            ).is_some(),
            "Activity.onCreate(Bundle) must resolve through the framework bridge"
        );
    }
}
