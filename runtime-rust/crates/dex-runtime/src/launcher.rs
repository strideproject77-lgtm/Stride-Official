//! APK Application Launcher and Runtime Execution Coordinator.
//!
//! Integrates Manifest parsing, Multi-DEX loading, runtime initialization,
//! class linking, Activity lifecycle startup, and actual bytecode execution.

use std::collections::HashMap;
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
use crate::canvas::{Canvas, Paint, PaintStyle, RectF};
use crate::resources::ResourceCatalog;

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
    /// Launcher lifecycle callbacks completed, but no persistent guest loop is active yet.
    LifecycleReady,
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
    /// App-owned static field storage shared by every interpreter invocation in this session.
    pub static_fields: HashMap<(String, String), crate::types::Value>,
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
    pub resources: ResourceCatalog,
    pub image_assets: std::collections::HashMap<u32,(u32,u32,Vec<u32>)>,
}

impl AndroidRuntime {
    pub fn load_resource_table(&mut self,data:&[u8])->Result<(),String>{self.resources=ResourceCatalog::parse(data)?;Ok(())}
    pub fn set_resource_image(&mut self,id:u32,width:u32,height:u32,pixels:&[u32])->Result<(),String>{let count=(width as usize).checked_mul(height as usize).ok_or("image dimensions overflow")?;if width==0||height==0||count>16_777_216||pixels.len()!=count{return Err("invalid decoded image dimensions".into());}self.image_assets.insert(id,(width,height,pixels.to_vec()));Ok(())}

    /// Creates a new Android runtime environment.
    pub fn new(package_name: &str) -> Self {
        let mut jni = JniNativeRegistry::new();
        Self::register_core_jni(&mut jni);

        Self {
            class_loader: AppClassLoader::new(),
            heap: ManagedHeap::default(),
            hierarchy: ClassHierarchy::new(),
            monitors: MonitorTable::new(),
            static_fields: HashMap::new(),
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
            resources: ResourceCatalog::default(),
            image_assets: std::collections::HashMap::new(),
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

        // Activity.getWindow() returns a stable per-Activity Window object.
        jni.register(
            "Landroid/app/Activity;",
            "getWindow",
            "()Landroid/view/Window;",
            |env, args| {
                let activity = args.first().and_then(|value| value.as_object())
                    .ok_or_else(|| "Activity.getWindow receiver is null".to_string())?;
                let existing = env.heap.get_instance_field(activity, "mWindow")
                    .map_err(|error| format!("Activity window lookup failed: {:?}", error))?
                    .as_object();
                let window = match existing {
                    Some(id) => id,
                    None => {
                        let id = env.heap.allocate_instance("Landroid/view/Window;")
                            .map_err(|error| format!("Window allocation failed: {:?}", error))?;
                        env.heap.set_instance_field(activity, "mWindow", Value::Object(id))
                            .map_err(|error| format!("Activity window store failed: {:?}", error))?;
                        id
                    }
                };
                Ok(Some(Value::Object(window)))
            },
        );

        // Window flag APIs used during app startup. Flags are retained on the
        // Window object; visual system-bar effects remain host-renderer work.
        for (name, prototype, operation) in [
            ("addFlags", "(I)V", 0i32),
            ("clearFlags", "(I)V", 1i32),
            ("setFlags", "(II)V", 2i32),
        ] {
            jni.register("Landroid/view/Window;", name, prototype, move |env, args| {
                let window = args.first().and_then(|value| value.as_object())
                    .ok_or_else(|| "Window receiver is null".to_string())?;
                let current = env.heap.get_instance_field(window, "flags")
                    .map_err(|error| format!("Window flags lookup failed: {:?}", error))?
                    .as_int();
                let flags = args.get(1).map(|value| value.as_int()).unwrap_or(0);
                let updated = match operation {
                    0 => current | flags,
                    1 => current & !flags,
                    _ => {
                        let mask = args.get(2).map(|value| value.as_int()).unwrap_or(0);
                        (current & !mask) | (flags & mask)
                    }
                };
                env.heap.set_instance_field(window, "flags", Value::Int(updated))
                    .map_err(|error| format!("Window flags update failed: {:?}", error))?;
                Ok(None)
            });
        }
        for name in ["setStatusBarColor", "setNavigationBarColor"] {
            jni.register("Landroid/view/Window;", name, "(I)V", |_env, _args| Ok(None));
        }

        // App Activities commonly call these superclass implementations from
        // their lifecycle overrides. Register the full core callback set so
        // those calls resolve instead of being reported as missing framework APIs.
        for (name, signature) in [
            ("onRestart", "()V"),
            ("onStart", "()V"),
            ("onResume", "()V"),
            ("onPause", "()V"),
            ("onStop", "()V"),
            ("onDestroy", "()V"),
        ] {
            jni.register(
                "Landroid/app/Activity;",
                name,
                signature,
                |_env, _args| Ok(None),
            );
        }

        // MotionEvent instances created by the host input bridge expose
        // Android getters used by Activity.onTouchEvent implementations.
        for (name, prototype, field) in [
            ("getAction", "()I", "action"),
            ("getX", "()F", "x"),
            ("getY", "()F", "y"),
            ("getRawX", "()F", "x"),
            ("getRawY", "()F", "y"),
            ("getPressure", "()F", "pressure"),
        ] {
            jni.register("Landroid/view/MotionEvent;", name, prototype, move |env, args| {
                let receiver = args.first().and_then(|value| value.as_object())
                    .ok_or_else(|| "MotionEvent receiver is null".to_string())?;
                let value = env.heap.get_instance_field(receiver, field)
                    .map_err(|error| format!("MotionEvent.{} read failed: {:?}", field, error))?;
                let result = if prototype == "()I" { Value::Int(value.as_int()) } else { Value::Float(value.as_float()) };
                Ok(Some(result))
            });
        }
        jni.register("Landroid/view/MotionEvent;", "getActionMasked", "()I", |env, args| {
            let receiver = args.first().and_then(|value| value.as_object())
                .ok_or_else(|| "MotionEvent receiver is null".to_string())?;
            let action = env.heap.get_instance_field(receiver, "action")
                .map_err(|error| format!("MotionEvent action read failed: {:?}", error))?
                .as_int();
            Ok(Some(Value::Int(action & 0xff)))
        });

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

        crate::view::register_core_view_methods(jni);
        crate::paint::register_core_paint_methods(jni);
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
            &mut self.static_fields,
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
                let dex_metadata = self.class_loader.dex_inventories.first().cloned();
                let dex_strings = dex_metadata.as_ref().map(|inventory| inventory.strings.as_slice()).unwrap_or(&[]);
                let mut vm = ArtVm::new(
                    &mut self.heap,
                    &mut self.hierarchy,
                    &mut self.monitors,
                    &mut self.jni,
                    &method_map,
                    dex_strings,
                );
                vm.attach_diagnostics(&self.diagnostics);
                vm.attach_static_fields(&mut self.static_fields);
                if let Some(inventory) = dex_metadata.as_ref() {
                    vm.attach_dex_metadata(&inventory.type_descriptors, &inventory.field_references);
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

        // Android lifecycle callbacks must execute in order, not just update state labels.
        self.environment.transition_activity(&activity_descriptor, ActivityLifecycleState::Started);
        self.diagnostics.record(DiagnosticSeverity::Info, "Lifecycle", "Activity started", Some(&activity_descriptor));

        // Execute the app-defined onStart() callback before onResume().
        if let Some(mid) = self.class_loader.find_method(&activity_descriptor, "onStart", "()V") {
            if let Ok(mid16) = u16::try_from(mid) {
                let method_map = self.class_loader.export_method_codes();
                let dex_metadata = self.class_loader.dex_inventories.first().cloned();
                let dex_strings = dex_metadata.as_ref().map(|inventory| inventory.strings.as_slice()).unwrap_or(&[]);
                let mut vm = ArtVm::new(
                    &mut self.heap, &mut self.hierarchy, &mut self.monitors,
                    &mut self.jni, &method_map, dex_strings,
                );
                vm.attach_diagnostics(&self.diagnostics);
                vm.attach_static_fields(&mut self.static_fields);
                if let Some(inventory) = dex_metadata.as_ref() {
                    vm.attach_dex_metadata(&inventory.type_descriptors, &inventory.field_references);
                    vm.attach_method_resolver(&inventory.method_references, &self.class_loader.method_lookup);
                }
                match vm.execute_method(mid16, &[Value::Object(activity_obj)], step_budget.saturating_sub(total_steps)) {
                    Ok(report) => {
                        total_steps += report.steps_executed;
                        self.diagnostics.record_detailed(
                            DiagnosticSeverity::Info, "ART", "ONSTART_EXECUTED",
                            "Activity onStart execution returned",
                            Some(&format!("steps={} stop={:?}", report.steps_executed, report.stop_reason)),
                            None,
                        );
                        match report.stop_reason {
                            ExecutionStopReason::UncaughtException(ex) => {
                                self.diagnostics.record_detailed(
                                    DiagnosticSeverity::RuntimeError, "ART", "ONSTART_UNCAUGHT_EXCEPTION",
                                    "Activity onStart terminated with an uncaught exception",
                                    Some(&format!("{:?}", ex)),
                                    Some("Inspect the exception class, message and captured DEX stack trace"),
                                );
                                return LaunchReport {
                                    package_name: self.environment.package_name.clone(),
                                    launcher_activity: activity_descriptor,
                                    lifecycle_state: ActivityLifecycleState::Started,
                                    total_classes_loaded: self.class_loader.classes.len(),
                                    instructions_executed: total_steps,
                                    status: LaunchStatus::UncaughtException { class: ex.class_descriptor, message: ex.message },
                                    diagnostics_summary: self.diagnostics.format_summary(),
                                };
                            }
                            ExecutionStopReason::StepLimitExceeded => {
                                self.diagnostics.record_detailed(
                                    DiagnosticSeverity::RuntimeError, "ART", "ONSTART_STEP_LIMIT",
                                    "Activity onStart exceeded the remaining instruction budget",
                                    Some(&format!("executed={} remaining={}", report.steps_executed, step_budget.saturating_sub(total_steps))),
                                    Some("Inspect onStart for repeated work before increasing the instruction budget"),
                                );
                                return LaunchReport {
                                    package_name: self.environment.package_name.clone(),
                                    launcher_activity: activity_descriptor,
                                    lifecycle_state: ActivityLifecycleState::Started,
                                    total_classes_loaded: self.class_loader.classes.len(),
                                    instructions_executed: total_steps,
                                    status: LaunchStatus::UncaughtException {
                                        class: "InstructionBudgetExceeded".into(),
                                        message: Some("onStart exceeded the remaining instruction budget".into()),
                                    },
                                    diagnostics_summary: self.diagnostics.format_summary(),
                                };
                            }
                            _ => {}
                        }
                    }
                    Err(error) => {
                        self.diagnostics.record_detailed(
                            DiagnosticSeverity::RuntimeError, "ART", "ONSTART_INTERPRETER_ERROR",
                            "ART interpreter failed while executing Activity onStart",
                            Some(&format!("method_id={} error={}", mid, error)),
                            Some("Inspect the failing opcode, register indices, method reference mapping and DEX code-item bounds"),
                        );
                        return LaunchReport {
                            package_name: self.environment.package_name.clone(),
                            launcher_activity: activity_descriptor,
                            lifecycle_state: ActivityLifecycleState::Started,
                            total_classes_loaded: self.class_loader.classes.len(),
                            instructions_executed: total_steps,
                            status: LaunchStatus::UncaughtException { class: "InterpreterError".into(), message: Some(error) },
                            diagnostics_summary: self.diagnostics.format_summary(),
                        };
                    }
                }
            }
        }

        self.environment.transition_activity(&activity_descriptor, ActivityLifecycleState::Resumed);
        self.diagnostics.record(DiagnosticSeverity::Info, "Lifecycle", "Activity resumed", Some(&activity_descriptor));

        // Check for onResume()V
        let on_resume_mid = self.class_loader.find_method(&activity_descriptor, "onResume", "()V");
        if let Some(mid) = on_resume_mid {
            if let Ok(mid16) = u16::try_from(mid) {
                let method_map = self.class_loader.export_method_codes();
                let dex_metadata = self.class_loader.dex_inventories.first().cloned();
                let dex_strings = dex_metadata.as_ref().map(|inventory| inventory.strings.as_slice()).unwrap_or(&[]);
                let mut vm = ArtVm::new(
                    &mut self.heap,
                    &mut self.hierarchy,
                    &mut self.monitors,
                    &mut self.jni,
                    &method_map,
                    dex_strings,
                );
                vm.attach_diagnostics(&self.diagnostics);
                vm.attach_static_fields(&mut self.static_fields);
                if let Some(inventory) = dex_metadata.as_ref() {
                    vm.attach_dex_metadata(&inventory.type_descriptors, &inventory.field_references);
                    vm.attach_method_resolver(&inventory.method_references, &self.class_loader.method_lookup);
                }
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

        match self.render_active_activity_surface() {
            Ok(true)=>self.diagnostics.record(DiagnosticSeverity::Info,"Graphics","ACTIVITY_VIEW_TREE_RENDERED",Some("Managed Activity View hierarchy rasterized into the guest Surface")),
            Ok(false)=>self.diagnostics.record(DiagnosticSeverity::Info,"Graphics","ACTIVITY_VIEW_TREE_EMPTY",Some("Activity resumed without a managed setContentView(View) root")),
            Err(error)=>self.diagnostics.record(DiagnosticSeverity::RuntimeError,"Graphics","ACTIVITY_VIEW_RENDER_FAILED",Some(&error)),
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
            status: LaunchStatus::LifecycleReady,
            diagnostics_summary: self.diagnostics.format_summary(),
        }
    }


    /// Rebuilds the current Activity's managed View hierarchy into the guest Surface.
    /// This is the first real Activity -> Canvas -> Surface bridge; unsupported custom
    /// drawing callbacks and unresolved layout-resource IDs are deliberately skipped.
    pub fn render_active_activity_surface(&mut self) -> Result<bool, String> {
        let Some(activity)=self.active_activity_obj else{return Ok(false)};
        let root=self.heap.get_instance_field(activity,"mContentView")
            .map_err(|e|format!("Activity content lookup failed: {:?}",e))?;
        let Some(root_id)=root.as_object() else{return Ok(false)};
        self.surface.clear(0xff000000);
        let width=self.surface.width as f32;let height=self.surface.height as f32;
        let mut canvas=Canvas::new(&mut self.surface);
        fn draw(heap:&ManagedHeap,resources:&ResourceCatalog,images:&std::collections::HashMap<u32,(u32,u32,Vec<u32>)>,canvas:&mut Canvas<'_>,id:u32,bounds:RectF,depth:usize,seen:&mut std::collections::HashSet<u32>)->Result<(),String>{
            if depth>128||!seen.insert(id){return Ok(());}
            let field=|name:&str|heap.get_instance_field(id,name).map_err(|e|format!("View field {} read failed: {:?}",name,e));
            let visibility=field("mVisibility")?.as_int();if visibility!=0{return Ok(());}
            let bg=field("mBackgroundColor")?.as_int() as u32;
            if bg!=0{let mut p=Paint::new();p.color=bg;p.style=PaintStyle::Fill;canvas.draw_rect(bounds,&p);}
            let text_value=field("mText")?;
            let text_resource=field("mTextResourceId").map(|v|v.as_int() as u32).unwrap_or(0);
            if let Some(text_id)=text_value.as_object(){
                if let Ok(text)=heap.get_string(text_id){
                    let mut p=Paint::new();let color=field("mTextColor")?.as_int() as u32;
                    p.color=if color==0{0xff000000}else{color};
                    let size=field("mTextSize")?.as_float();p.text_size=if size.is_finite()&&size>0.0{size}else{14.0};
                    let baseline=(bounds.top+bounds.bottom)*0.5+p.text_size*0.35;
                    let text_width=text.chars().count()as f32*p.text_size*6.0/7.0;
                    canvas.draw_text(text,bounds.left+(bounds.width()-text_width).max(0.0)*0.5,baseline,&p);
                }
            } else if text_resource!=0 {
                if let Some(text)=resources.string(text_resource){let mut p=Paint::new();let color=field("mTextColor")?.as_int() as u32;p.color=if color==0{0xff000000}else{color};let size=field("mTextSize")?.as_float();p.text_size=if size.is_finite()&&size>0.0{size}else{14.0};let baseline=(bounds.top+bounds.bottom)*0.5+p.text_size*0.35;canvas.draw_text(text,bounds.left+2.0,baseline,&p);}
            }
            let image_id=field("mImageResourceId").map(|v|v.as_int() as u32).unwrap_or(0);
            let image_id=field("mImageResourceId").map(|v|v.as_int() as u32).unwrap_or(0);
            if let Some((iw,ih,pixels))=images.get(&image_id){canvas.draw_image_argb(pixels,*iw,*ih,bounds.left as i32,bounds.top as i32);}
            let bg_resource=field("mBackgroundResourceId").map(|v|v.as_int() as u32).unwrap_or(0);
            if bg_resource!=0{if let Some(color)=resources.color(bg_resource){let mut p=Paint::new();p.color=color;canvas.draw_rect(bounds,&p);}}
            let count=field("mChildCount")?.as_int().clamp(0,4096)as usize;
            if count==0{return Ok(());}
            let orientation=field("mOrientation")?.as_int();let vertical=orientation!=0;
            let main_extent=if vertical{bounds.height()}else{bounds.width()};
            let mut weighted_total=0.0f32;
            let mut weights=Vec::with_capacity(count);
            for index in 0..count {
                let child=field(&format!("mChild{}",index))?;
                let Some(child_id)=child.as_object()else{weights.push((0,0.0));continue};
                let raw=heap.get_instance_field(child_id,"mLayoutWeight").map(|v|v.as_float()).unwrap_or(0.0);
                let weight=if raw.is_finite(){raw.max(0.0)}else{0.0};weighted_total+=weight;weights.push((child_id,weight));
            }
            let mut cursor=if vertical{bounds.top}else{bounds.left};
            let mut remaining=main_extent;
            let mut remaining_weight=weighted_total;
            let equal_segment=main_extent/count as f32;
            for (child_id,weight) in weights {
                if child_id==0{continue;}
                let extent=if weight>0.0&&remaining_weight>0.0{remaining*weight/remaining_weight}else if weighted_total>0.0{equal_segment.min(remaining)}else{equal_segment};
                let read_int=|name:&str|heap.get_instance_field(child_id,name).map(|v|v.as_int()).unwrap_or(0).max(0) as f32;
                let ml=read_int("mMarginLeft");let mt=read_int("mMarginTop");let mr=read_int("mMarginRight");let mb=read_int("mMarginBottom");
                let child_bounds=if vertical {
                    RectF::new(bounds.left+ml,cursor+mt,(bounds.right-mr).max(bounds.left+ml),cursor+extent-mb)
                } else {
                    RectF::new(cursor+ml,bounds.top+mt,cursor+extent-mr,(bounds.bottom-mb).max(bounds.top+mt))
                };
                draw(heap,resources,images,canvas,child_id,child_bounds,depth+1,seen)?;
                cursor+=extent;remaining=(remaining-extent).max(0.0);remaining_weight=(remaining_weight-weight).max(0.0);
            }
            Ok(())
        }
        draw(&self.heap,&self.resources,&self.image_assets,&mut canvas,root_id,RectF::new(0.0,0.0,width,height),0,&mut std::collections::HashSet::new())?;
        Ok(true)
    }

    /// Pumps queued Android touch events on the host display-link cadence.
    /// The guest Activity receives real MotionEvent objects through its DEX
    /// onTouchEvent/dispatchTouchEvent override; no event is reported as
    /// delivered when the APK has no compatible callback.
    pub fn pump_guest_events(&mut self, max_events: usize, step_budget: usize) -> Result<usize, String> {
        let activity_obj = self.active_activity_obj
            .ok_or_else(|| "No resumed Android Activity is active".to_string())?;
        let activity_class = self.active_activity_class.clone()
            .ok_or_else(|| "No active Android Activity class is recorded".to_string())?;

        let callback = self.class_loader
            .find_method(&activity_class, "dispatchTouchEvent", "(Landroid/view/MotionEvent;)Z")
            .or_else(|| self.class_loader.find_method(
                &activity_class, "onTouchEvent", "(Landroid/view/MotionEvent;)Z"
            ));
        let Some(method_id) = callback else {
            return Ok(0);
        };
        let method_id = u16::try_from(method_id)
            .map_err(|_| "Touch callback method ID exceeds the interpreter limit".to_string())?;
        let method_map = self.class_loader.export_method_codes();
                let dex_metadata = self.class_loader.dex_inventories.first().cloned();
                let dex_strings = dex_metadata.as_ref().map(|inventory| inventory.strings.as_slice()).unwrap_or(&[]);
        let mut delivered = 0usize;

        while delivered < max_events {
            let Some(event) = self.input_queue.poll_motion() else { break };
            let event_obj = self.heap.allocate_instance("Landroid/view/MotionEvent;")
                .map_err(|error| format!("MotionEvent allocation failed: {:?}", error))?;
            self.heap.set_instance_field(event_obj, "action", Value::Int(event.action))
                .map_err(|error| format!("MotionEvent action setup failed: {:?}", error))?;
            let pointer = event.pointers.first();
            self.heap.set_instance_field(event_obj, "x", Value::Float(pointer.map(|p| p.x).unwrap_or(0.0)))
                .map_err(|error| format!("MotionEvent X setup failed: {:?}", error))?;
            self.heap.set_instance_field(event_obj, "y", Value::Float(pointer.map(|p| p.y).unwrap_or(0.0)))
                .map_err(|error| format!("MotionEvent Y setup failed: {:?}", error))?;
            self.heap.set_instance_field(event_obj, "pressure", Value::Float(pointer.map(|p| p.pressure).unwrap_or(0.0)))
                .map_err(|error| format!("MotionEvent pressure setup failed: {:?}", error))?;

            let mut vm = ArtVm::new(
                &mut self.heap,
                &mut self.hierarchy,
                &mut self.monitors,
                &mut self.jni,
                &method_map,
                dex_strings,
            );
            vm.attach_diagnostics(&self.diagnostics);
            vm.attach_static_fields(&mut self.static_fields);
            if let Some(inventory) = dex_metadata.as_ref() {
                    vm.attach_dex_metadata(&inventory.type_descriptors, &inventory.field_references);
                    vm.attach_method_resolver(&inventory.method_references, &self.class_loader.method_lookup);
                }
            let args = [Value::Object(activity_obj), Value::Object(event_obj)];
            let report = vm.execute_method(method_id, &args, step_budget.max(1))
                .map_err(|error| format!("Touch callback interpreter error: {}", error))?;
            drop(vm);
            match report.stop_reason {
                ExecutionStopReason::UncaughtException(exception) => {
                    return Err(format!("Touch callback threw {}: {:?}", exception.class_descriptor, exception.message));
                }
                ExecutionStopReason::StepLimitExceeded => {
                    return Err("Touch callback exceeded its per-event instruction budget".to_string());
                }
                _ => { delivered += 1; }
            }
        }
        Ok(delivered)
    }
}


#[cfg(test)]
mod framework_bridge_tests {
    use super::*;

    #[test]
    fn activity_core_lifecycle_super_calls_have_framework_bridges() {
        let runtime = AndroidRuntime::new("com.stride.test");
        for (name, signature) in [
            ("onCreate", "(Landroid/os/Bundle;)V"),
            ("onRestart", "()V"),
            ("onStart", "()V"),
            ("onResume", "()V"),
            ("onPause", "()V"),
            ("onStop", "()V"),
            ("onDestroy", "()V"),
        ] {
            assert!(
                runtime.jni.find("Landroid/app/Activity;", name, signature).is_some(),
                "Activity.{}{} must resolve through the framework bridge",
                name,
                signature
            );
        }
    }
}
