//! APK Application Launcher and Runtime Execution Coordinator.
//!
//! Integrates Manifest parsing, Multi-DEX loading, runtime initialization,
//! class linking, Activity lifecycle startup, and actual bytecode execution.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use stride_axml::parse_manifest;
use crate::loader::AppClassLoader;
use crate::hierarchy::ClassHierarchy;
use crate::heap::ManagedHeap;
use crate::sync::MonitorTable;
use crate::jni::JniNativeRegistry;
use crate::framework::{AndroidEnvironment, ActivityLifecycleState, PackageInfo, ActivityInfo, ServiceInfo};
use crate::graphics::{Surface, InputQueue, DisplayMetrics, ACTION_DOWN, ACTION_UP, ACTION_CANCEL};
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
    /// Preference values shared by Activity instances for this guest process.
    pub shared_preferences_store: crate::shared_prefs::PreferenceStore,
    pub jni: JniNativeRegistry,
    pub environment: AndroidEnvironment,
    pub surface: Surface,
    pub input_queue: Arc<InputQueue>,
    pub display_metrics: DisplayMetrics,
    pub active_activity_obj: Option<u32>,
    pub active_activity_class: Option<String>,
    /// Ensures the first managed-view render emits one non-sensitive text-pipeline summary.
    pub initial_render_summary_recorded: bool,
    /// Last laid-out managed View bounds, in guest-surface pixels, used for hit testing.
    pub rendered_view_bounds: Vec<(u32, RectF)>,
    /// View pressed on ACTION_DOWN; clicks are dispatched only when ACTION_UP lands on it.
    pub pressed_view: Option<u32>,
    /// EditText currently selected by the guest touch surface for native keyboard input.
    pub focused_edit_text: Option<u32>,
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
        let shared_preferences_store = Arc::new(Mutex::new(HashMap::new()));
        Self::register_core_jni(&mut jni, shared_preferences_store.clone());

        Self {
            class_loader: AppClassLoader::new(),
            heap: ManagedHeap::default(),
            hierarchy: ClassHierarchy::new(),
            monitors: MonitorTable::new(),
            static_fields: HashMap::new(),
            shared_preferences_store,
            jni,
            environment: AndroidEnvironment::new(package_name),
            surface: Surface::new(1080, 1920),
            input_queue: Arc::new(InputQueue::new()),
            display_metrics: DisplayMetrics::default(),
            active_activity_obj: None,
            active_activity_class: None,
            initial_render_summary_recorded: false,
            rendered_view_bounds: Vec::new(),
            pressed_view: None,
            focused_edit_text: None,
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
    fn register_core_jni(jni: &mut JniNativeRegistry, shared_preferences_store: crate::shared_prefs::PreferenceStore) {
        // Activity lifecycle methods are framework-owned and therefore do not
        // have app DEX MethodCode entries. The launcher owns lifecycle state
        // transitions; this bridge safely accepts the Activity's super call.
        jni.register(
            "Landroid/app/Activity;",
            "<init>",
            "()V",
            |_env, _args| Ok(None),
        );

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

        jni.register("Landroid/view/Window;", "getDecorView", "()Landroid/view/View;", |env, args| {
            let window = args.first().and_then(|value| value.as_object())
                .ok_or_else(|| "Window.getDecorView receiver is null".to_string())?;
            if let Some(existing) = env.heap.get_instance_field(window, "mDecorView")
                .map_err(|error| format!("Window decor lookup failed: {:?}", error))?.as_object() {
                return Ok(Some(Value::Object(existing)));
            }
            let decor = env.heap.get_instance_field(window, "mContentView")
                .ok().and_then(|value| value.as_object())
                .or_else(|| env.heap.allocate_instance("Landroid/view/View;").ok());
            let decor = decor.ok_or("Window decor View allocation failed")?;
            env.heap.set_instance_field(window, "mDecorView", Value::Object(decor))
                .map_err(|error| format!("Window decor store failed: {:?}", error))?;
            Ok(Some(Value::Object(decor)))
        });

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
            jni.register("Landroid/view/Window;", name, "(I)V", |env, args| {
                let window = args.first().and_then(|value| value.as_object())
                    .ok_or_else(|| "Window receiver is null".to_string())?;
                let color = args.get(1).map(Value::as_int).unwrap_or(0);
                let field = if name == "setStatusBarColor" { "mStatusBarColor" } else { "mNavigationBarColor" };
                env.heap.set_instance_field(window, field, Value::Int(color))
                    .map_err(|error| format!("Window system bar color update failed: {:?}", error))?;
                Ok(None)
            });
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
        crate::canvas_jni::register_core_canvas_methods(jni);
        crate::bitmap::register_core_bitmap_methods(jni);
        crate::typeface::register_core_typeface_methods(jni);
        crate::drawable::register_core_drawable_methods(jni);
        crate::shared_prefs::register_shared_preferences(jni, shared_preferences_store);
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

        // Android constructs an Activity before dispatching onCreate. Java field
        // initializers live in <init>, so skipping this callback leaves app state null.
        if let Some(constructor_id) = self.class_loader.find_method(&activity_descriptor, "<init>", "()V") {
            if let Ok(constructor_id16) = u16::try_from(constructor_id) {
                let method_map = self.class_loader.export_method_codes();
                let dex_metadata = self.class_loader.dex_inventories.first().cloned();
                let dex_strings = dex_metadata.as_ref()
                    .map(|inventory| inventory.strings.as_slice()).unwrap_or(&[]);
                let mut vm = ArtVm::new(
                    &mut self.heap, &mut self.hierarchy, &mut self.monitors,
                    &mut self.jni, &method_map, dex_strings,
                );
                vm.attach_diagnostics(&self.diagnostics);
                vm.attach_static_fields(&mut self.static_fields);
                vm.attach_multi_dex_metadata(&self.class_loader.dex_inventories, &self.class_loader.method_dex_indices);
                if let Some(inventory) = dex_metadata.as_ref() {
                    vm.attach_dex_metadata(&inventory.type_descriptors, &inventory.field_references);
                    vm.attach_method_resolver(&inventory.method_references, &self.class_loader.method_lookup);
                }
                match vm.execute_method(constructor_id16, &[Value::Object(activity_obj)], step_budget) {
                    Ok(report) => match report.stop_reason {
                        ExecutionStopReason::UncaughtException(ex) => {
                            self.diagnostics.record_detailed(
                                DiagnosticSeverity::RuntimeError, "Lifecycle", "ACTIVITY_CONSTRUCTOR_FAILED",
                                "Activity constructor terminated with an uncaught exception",
                                Some(&format!("{:?}", ex)), None,
                            );
                            return LaunchReport {
                                package_name: self.environment.package_name.clone(),
                                launcher_activity: activity_descriptor,
                                lifecycle_state: ActivityLifecycleState::NotStarted,
                                total_classes_loaded: self.class_loader.classes.len(),
                                instructions_executed: report.steps_executed,
                                status: LaunchStatus::UncaughtException {
                                    class: ex.class_descriptor, message: ex.message,
                                },
                                diagnostics_summary: self.diagnostics.format_summary(),
                            };
                        }
                        ExecutionStopReason::StepLimitExceeded => {
                            return LaunchReport {
                                package_name: self.environment.package_name.clone(),
                                launcher_activity: activity_descriptor,
                                lifecycle_state: ActivityLifecycleState::NotStarted,
                                total_classes_loaded: self.class_loader.classes.len(),
                                instructions_executed: report.steps_executed,
                                status: LaunchStatus::UncaughtException {
                                    class: "InstructionBudgetExceeded".into(),
                                    message: Some("Activity constructor exceeded the instruction budget".into()),
                                },
                                diagnostics_summary: self.diagnostics.format_summary(),
                            };
                        }
                        _ => {}
                    },
                    Err(error) => {
                        self.diagnostics.record_detailed(
                            DiagnosticSeverity::RuntimeError, "Lifecycle", "ACTIVITY_CONSTRUCTOR_INTERPRETER_ERROR",
                            "ART interpreter failed while executing the Activity constructor",
                            Some(&error), None,
                        );
                        return LaunchReport {
                            package_name: self.environment.package_name.clone(),
                            launcher_activity: activity_descriptor,
                            lifecycle_state: ActivityLifecycleState::NotStarted,
                            total_classes_loaded: self.class_loader.classes.len(),
                            instructions_executed: 0,
                            status: LaunchStatus::UncaughtException {
                                class: "ConstructorError".into(), message: Some(error),
                            },
                            diagnostics_summary: self.diagnostics.format_summary(),
                        };
                    }
                }
            }
        }

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
                vm.attach_multi_dex_metadata(&self.class_loader.dex_inventories, &self.class_loader.method_dex_indices);
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
                vm.attach_multi_dex_metadata(&self.class_loader.dex_inventories, &self.class_loader.method_dex_indices);
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
                vm.attach_multi_dex_metadata(&self.class_loader.dex_inventories, &self.class_loader.method_dex_indices);
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
        let Some(activity) = self.active_activity_obj else { return Ok(false) };
        let root = self.heap.get_instance_field(activity, "mContentView")
            .map_err(|error| format!("Activity content lookup failed: {:?}", error))?;
        let Some(root_id) = root.as_object() else { return Ok(false) };

        #[derive(Default)]
        struct RenderStats {
            views: usize,
            text_views: usize,
            text_views_with_text: usize,
            empty_text_views: usize,
            unresolved_text_references: usize,
            text_color_matches_background: usize,
        }

        self.surface.clear(0xff000000);
        let width = self.surface.width as f32;
        let height = self.surface.height as f32;
        let mut canvas = Canvas::new(&mut self.surface);

        fn draw(
            heap: &ManagedHeap,
            resources: &ResourceCatalog,
            images: &std::collections::HashMap<u32, (u32, u32, Vec<u32>)>,
            canvas: &mut Canvas<'_>,
            id: u32,
            bounds: RectF,
            depth: usize,
            seen: &mut std::collections::HashSet<u32>,
            stats: &mut RenderStats,
            bounds_out: &mut Vec<(u32, RectF)>,
        ) -> Result<(), String> {
            if depth > 128 || !seen.insert(id) { return Ok(()); }
            stats.views += 1;

            let field = |name: &str| heap.get_instance_field(id, name)
                .map_err(|error| format!("View field {} read failed: {:?}", name, error));
            let visibility = field("mVisibility")?.as_int();
            if visibility != 0 { return Ok(()); }
            bounds_out.push((id, bounds));

            let bg = field("mBackgroundColor")?.as_int() as u32;
            if bg != 0 {
                let mut paint = Paint::new();
                paint.color = bg;
                paint.style = PaintStyle::Fill;
                canvas.draw_rect(bounds, &paint);
            }

            let is_text_view = heap.get(id)
                .map(|object| object.class_descriptor() == "Landroid/widget/TextView;")
                .unwrap_or(false);
            if is_text_view { stats.text_views += 1; }

            let text_value = field("mText")?;
            let text_resource = field("mTextResourceId").map(|value| value.as_int() as u32).unwrap_or(0);
            let mut text_to_draw: Option<String> = None;

            if let Some(text_id) = text_value.as_object() {
                match heap.get_string(text_id) {
                    Ok(text) => {
                        if text.is_empty() {
                            if is_text_view { stats.empty_text_views += 1; }
                        } else {
                            if is_text_view { stats.text_views_with_text += 1; }
                            text_to_draw = Some(text.to_owned());
                        }
                    }
                    Err(_) => {
                        if is_text_view { stats.unresolved_text_references += 1; }
                    }
                }
            } else if text_resource != 0 {
                match resources.string(text_resource) {
                    Some(text) if !text.is_empty() => {
                        if is_text_view { stats.text_views_with_text += 1; }
                        text_to_draw = Some(text.to_owned());
                    }
                    Some(_) => {
                        if is_text_view { stats.empty_text_views += 1; }
                    }
                    None => {
                        if is_text_view { stats.unresolved_text_references += 1; }
                    }
                }
            } else if is_text_view {
                stats.empty_text_views += 1;
            }

            if let Some(text) = text_to_draw {
                let mut paint = Paint::new();
                let color = field("mTextColor")?.as_int() as u32;
                paint.color = if color == 0 { 0xff000000 } else { color };
                if is_text_view && bg != 0 && (paint.color & 0x00ff_ffff) == (bg & 0x00ff_ffff) {
                    stats.text_color_matches_background += 1;
                }
                let size = field("mTextSize")?.as_float();
                paint.text_size = if size.is_finite() && size > 0.0 { size } else { 14.0 };
                let baseline = (bounds.top + bounds.bottom) * 0.5 + paint.text_size * 0.35;
                let text_width = text.chars().count() as f32 * paint.text_size * 6.0 / 7.0;
                canvas.draw_text(
                    &text,
                    bounds.left + (bounds.width() - text_width).max(0.0) * 0.5,
                    baseline,
                    &paint,
                );
            }

            let image_id = field("mImageResourceId").map(|value| value.as_int() as u32).unwrap_or(0);
            if let Some((image_width, image_height, pixels)) = images.get(&image_id) {
                canvas.draw_image_argb(pixels, *image_width, *image_height, bounds.left as i32, bounds.top as i32);
            }

            let background_resource = field("mBackgroundResourceId").map(|value| value.as_int() as u32).unwrap_or(0);
            if background_resource != 0 {
                if let Some(color) = resources.color(background_resource) {
                    let mut paint = Paint::new();
                    paint.color = color;
                    canvas.draw_rect(bounds, &paint);
                }
            }

            let count = field("mChildCount")?.as_int().clamp(0, 4096) as usize;
            if count == 0 { return Ok(()); }

            let orientation = field("mOrientation")?.as_int();
            let vertical = orientation != 0;
            let main_extent = if vertical { bounds.height() } else { bounds.width() };
            let mut weighted_total = 0.0f32;
            let mut weights = Vec::with_capacity(count);
            for index in 0..count {
                let child = field(&format!("mChild{}", index))?;
                let Some(child_id) = child.as_object() else {
                    weights.push((0, 0.0));
                    continue;
                };
                let raw = heap.get_instance_field(child_id, "mLayoutWeight")
                    .map(|value| value.as_float()).unwrap_or(0.0);
                let weight = if raw.is_finite() { raw.max(0.0) } else { 0.0 };
                weighted_total += weight;
                weights.push((child_id, weight));
            }

            let mut cursor = if vertical { bounds.top } else { bounds.left };
            let mut remaining = main_extent;
            let mut remaining_weight = weighted_total;
            let equal_segment = main_extent / count as f32;
            for (child_id, weight) in weights {
                if child_id == 0 { continue; }
                let extent = if weight > 0.0 && remaining_weight > 0.0 {
                    remaining * weight / remaining_weight
                } else if weighted_total > 0.0 {
                    equal_segment.min(remaining)
                } else {
                    equal_segment
                };
                let read_margin = |name: &str| {
                    heap.get_instance_field(child_id, name)
                        .map(|value| value.as_int()).unwrap_or(0).max(0) as f32
                };
                let left = read_margin("mMarginLeft");
                let top = read_margin("mMarginTop");
                let right = read_margin("mMarginRight");
                let bottom = read_margin("mMarginBottom");
                let child_bounds = if vertical {
                    RectF::new(
                        bounds.left + left,
                        cursor + top,
                        (bounds.right - right).max(bounds.left + left),
                        cursor + extent - bottom,
                    )
                } else {
                    RectF::new(
                        cursor + left,
                        bounds.top + top,
                        cursor + extent - right,
                        (bounds.bottom - bottom).max(bounds.top + top),
                    )
                };
                draw(heap, resources, images, canvas, child_id, child_bounds, depth + 1, seen, stats, bounds_out)?;
                cursor += extent;
                remaining = (remaining - extent).max(0.0);
                remaining_weight = (remaining_weight - weight).max(0.0);
            }
            Ok(())
        }

        let mut stats = RenderStats::default();
        let mut rendered_view_bounds = Vec::new();
        draw(
            &self.heap,
            &self.resources,
            &self.image_assets,
            &mut canvas,
            root_id,
            RectF::new(0.0, 0.0, width, height),
            0,
            &mut std::collections::HashSet::new(),
            &mut stats,
            &mut rendered_view_bounds,
        )?;
        drop(canvas);
        self.rendered_view_bounds = rendered_view_bounds;
        self.render_guest_custom_view_drawing()?;


        if !self.initial_render_summary_recorded {
            self.diagnostics.record(
                DiagnosticSeverity::Info,
                "Graphics",
                "ACTIVITY_TEXT_RENDER_SUMMARY",
                Some(&format!(
                    "views={} text_views={} text_views_with_text={} empty_text_views={} unresolved_text_references={} text_color_matches_background={}",
                    stats.views,
                    stats.text_views,
                    stats.text_views_with_text,
                    stats.empty_text_views,
                    stats.unresolved_text_references,
                    stats.text_color_matches_background,
                )),
            );
            self.initial_render_summary_recorded = true;
        }
        Ok(true)
    }

    fn replay_guest_canvas_commands(
        heap: &ManagedHeap, canvas_id: u32, surface: &mut Surface,
    ) -> Result<usize, String> {
        let command_count = crate::canvas_jni::recorded_command_count(heap, canvas_id)?;
        let mut canvas = Canvas::new(surface);
        for index in 0..command_count {
            let prefix = format!("mStrideCommand{}", index);
            let read = |suffix: &str| heap.get_instance_field(canvas_id, &format!("{prefix}{suffix}"))
                .map_err(|error| format!("Guest Canvas command {}{} unavailable: {:?}", index, suffix, error));
            let number = |suffix: &str| -> Result<f32, String> { Ok(read(suffix)?.as_float()) };
            let color = |suffix: &str| -> Result<u32, String> { Ok(read(suffix)?.as_int() as u32) };
            match read("Type")?.as_int() {
                1 => canvas.draw_color(color("Arg0")?),
                2 => {
                    let mut paint = Paint::new();
                    paint.color = color("Arg4")?;
                    canvas.draw_rect(RectF::new(number("Arg0")?, number("Arg1")?, number("Arg2")?, number("Arg3")?), &paint);
                }
                3 => {
                    let text_id = read("Arg0")?.as_object().ok_or("Guest Canvas drawText received null text")?;
                    let text = heap.get_string(text_id)
                        .map_err(|error| format!("Guest Canvas text lookup failed: {:?}", error))?.to_owned();
                    let mut paint = Paint::new();
                    paint.color = color("Arg3")?;
                    paint.text_size = number("Arg4")?;
                    canvas.draw_text(&text, number("Arg1")?, number("Arg2")?, &paint);
                }
                4 => {
                    let mut paint = Paint::new();
                    paint.color = color("Arg4")?;
                    canvas.draw_line(number("Arg0")?, number("Arg1")?, number("Arg2")?, number("Arg3")?, &paint);
                }
                5 => {
                    let mut paint = Paint::new();
                    paint.color = color("Arg3")?;
                    canvas.draw_circle(number("Arg0")?, number("Arg1")?, number("Arg2")?, &paint);
                }
                6 => { canvas.save(); }
                7 => { canvas.restore(); }
                8 => { canvas.translate(number("Arg0")?, number("Arg1")?); }
                9 => { canvas.scale(number("Arg0")?, number("Arg1")?); }
                10 => {
                    let bitmap_id = read("Arg0")?.as_object().ok_or("Guest Canvas drawBitmap received null bitmap")?;
                    let width = heap.get_instance_field(bitmap_id, "mStrideWidth")
                        .map_err(|error| format!("Bitmap width lookup failed: {:?}", error))?.as_int();
                    let height = heap.get_instance_field(bitmap_id, "mStrideHeight")
                        .map_err(|error| format!("Bitmap height lookup failed: {:?}", error))?.as_int();
                    let pixels_id = heap.get_instance_field(bitmap_id, "mStridePixels")
                        .map_err(|error| format!("Bitmap pixel buffer lookup failed: {:?}", error))?.as_object()
                        .ok_or("Guest Bitmap pixel buffer is null")?;
                    if width <= 0 || height <= 0 || (width as i64) * (height as i64) > 16_777_216 {
                        return Err("Guest Bitmap dimensions are invalid".into());
                    }
                    let pixel_count = (width * height) as usize;
                    let pixels = match heap.get(pixels_id)
                        .map_err(|error| format!("Bitmap pixel array lookup failed: {:?}", error))? {
                        crate::heap::HeapObject::Array(array) if array.elements.len() == pixel_count =>
                            array.elements.iter().map(|value| value.as_int() as u32).collect::<Vec<_>>(),
                        _ => return Err("Guest Bitmap pixel array has an invalid shape".into()),
                    };
                    canvas.draw_image_argb(&pixels, width as u32, height as u32,
                        number("Arg1")? as i32, number("Arg2")? as i32);
                }
                unknown => return Err(format!("Unsupported guest Canvas command type {}", unknown)),
            }
        }
        Ok(command_count)
    }

    /// Flushes offscreen Canvas(Bitmap) command queues into their managed pixel
    /// arrays before the screen Canvas consumes those bitmaps via drawBitmap().
    fn flush_guest_bitmap_canvases(&mut self) -> Result<(), String> {
        let mut ids = self.heap.instance_ids();
        ids.sort_unstable();
        for canvas_id in ids {
            let class = self.heap.get(canvas_id)
                .map_err(|error| format!("Canvas object lookup failed: {:?}", error))?
                .class_descriptor().to_owned();
            if class != "Landroid/graphics/Canvas;" { continue; }
            let target = self.heap.get_instance_field(canvas_id, "mStrideBitmap")
                .ok().and_then(|value| value.as_object());
            let Some(bitmap_id) = target else { continue };
            let width = self.heap.get_instance_field(bitmap_id, "mStrideWidth")
                .map_err(|error| format!("Offscreen Bitmap width lookup failed: {:?}", error))?.as_int();
            let height = self.heap.get_instance_field(bitmap_id, "mStrideHeight")
                .map_err(|error| format!("Offscreen Bitmap height lookup failed: {:?}", error))?.as_int();
            let pixels_id = self.heap.get_instance_field(bitmap_id, "mStridePixels")
                .map_err(|error| format!("Offscreen Bitmap pixel buffer lookup failed: {:?}", error))?.as_object()
                .ok_or("Offscreen Bitmap pixel buffer is null")?;
            if width <= 0 || height <= 0 || (width as i64) * (height as i64) > 16_777_216 {
                return Err("Offscreen Bitmap dimensions are invalid".into());
            }
            let pixel_count = (width * height) as usize;
            let initial = match self.heap.get(pixels_id)
                .map_err(|error| format!("Offscreen Bitmap pixel array lookup failed: {:?}", error))? {
                crate::heap::HeapObject::Array(array) if array.elements.len() == pixel_count =>
                    array.elements.iter().map(|value| value.as_int() as u32).collect::<Vec<_>>(),
                _ => return Err("Offscreen Bitmap pixel array has an invalid shape".into()),
            };
            let mut surface = Surface::new(width as u32, height as u32);
            surface.pixels.copy_from_slice(&initial);
            let commands = Self::replay_guest_canvas_commands(&self.heap, canvas_id, &mut surface)?;
            for (index, pixel) in surface.pixels.into_iter().enumerate() {
                self.heap.set_array_element(pixels_id, index as i32, Value::Int(pixel as i32))
                    .map_err(|error| format!("Offscreen Bitmap pixel write failed: {:?}", error))?;
            }
            self.heap.set_instance_field(canvas_id, "mStrideCommandCount", Value::Int(0))
                .map_err(|error| format!("Offscreen Canvas queue reset failed: {:?}", error))?;
            if commands > 0 {
                self.diagnostics.record(
                    DiagnosticSeverity::Info, "Graphics", "OFFSCREEN_BITMAP_CANVAS_FLUSHED",
                    Some(&format!("bitmap={}x{} commands={}", width, height, commands)),
                );
            }
        }
        Ok(())
    }

    /// Executes app-owned custom View.onDraw(Canvas) and replays the guest Canvas
    /// command stream onto the iOS-backed Surface. Standard framework Views keep
    /// using the hierarchy renderer above; only classes declaring a real DEX
    /// onDraw implementation are invoked here.
    fn render_guest_custom_view_drawing(&mut self) -> Result<(), String> {
        let candidates = self.rendered_view_bounds.clone();
        for (view_id, _bounds) in candidates {
            let class_descriptor = self.heap.get(view_id)
                .map_err(|error| format!("Custom View class lookup failed: {:?}", error))?
                .class_descriptor().to_owned();
            if class_descriptor.starts_with("Landroid/") { continue; }
            let Some(method_id) = self.class_loader.find_method(
                &class_descriptor, "onDraw", "(Landroid/graphics/Canvas;)V"
            ) else { continue };
            // Deliver the first real size callback before drawing. This is where
            // custom Views commonly allocate their offscreen Bitmap/Canvas buffers.
            let width = self.surface.width as i32;
            let height = self.surface.height as i32;
            for (field, value) in [
                ("mWidth", Value::Int(width)), ("mHeight", Value::Int(height)),
                ("mRight", Value::Int(width)), ("mBottom", Value::Int(height)),
            ] {
                self.heap.set_instance_field(view_id, field, value)
                    .map_err(|error| format!("Custom View size state update failed: {:?}", error))?;
            }
            let size_delivered = self.heap.get_instance_field(view_id, "mStrideSizeChanged")
                .map(|value| value.as_int() != 0).unwrap_or(false);
            if !size_delivered {
                if let Some(size_method) = self.class_loader.find_method(
                    &class_descriptor, "onSizeChanged", "(IIII)V"
                ) {
                    let size_method = u16::try_from(size_method)
                        .map_err(|_| "Custom View onSizeChanged method ID exceeds interpreter limit".to_owned())?;
                    let method_map = self.class_loader.export_method_codes();
                    let metadata = self.class_loader.dex_inventories.first().cloned();
                    let strings = metadata.as_ref().map(|inventory| inventory.strings.as_slice()).unwrap_or(&[]);
                    let mut vm = ArtVm::new(
                        &mut self.heap, &mut self.hierarchy, &mut self.monitors,
                        &mut self.jni, &method_map, strings,
                    );
                    vm.attach_diagnostics(&self.diagnostics);
                    vm.attach_static_fields(&mut self.static_fields);
                    vm.attach_multi_dex_metadata(&self.class_loader.dex_inventories, &self.class_loader.method_dex_indices);
                    if let Some(inventory) = metadata.as_ref() {
                        vm.attach_dex_metadata(&inventory.type_descriptors, &inventory.field_references);
                        vm.attach_method_resolver(&inventory.method_references, &self.class_loader.method_lookup);
                    }
                    let size_args = [
                        Value::Object(view_id), Value::Int(width), Value::Int(height),
                        Value::Int(0), Value::Int(0),
                    ];
                    let size_report = vm.execute_method(size_method, &size_args, 250_000)
                        .map_err(|error| format!("Guest onSizeChanged interpreter error in {}: {}", class_descriptor, error))?;
                    match size_report.stop_reason {
                        ExecutionStopReason::UncaughtException(exception) =>
                            return Err(format!("Guest {}.onSizeChanged threw {}: {:?}", class_descriptor, exception.class_descriptor, exception.message)),
                        ExecutionStopReason::StepLimitExceeded =>
                            return Err(format!("Guest {}.onSizeChanged exceeded its instruction budget", class_descriptor)),
                        _ => {}
                    }
                    self.heap.set_instance_field(view_id, "mStrideSizeChanged", Value::Int(1))
                        .map_err(|error| format!("Custom View size state update failed: {:?}", error))?;
                }
            }

            let method_id = u16::try_from(method_id)
                .map_err(|_| "Custom View onDraw method ID exceeds interpreter limit".to_owned())?;
            let canvas_id = self.heap.allocate_instance("Landroid/graphics/Canvas;")
                .map_err(|error| format!("Guest Canvas allocation failed: {:?}", error))?;
            for (field, value) in [
                ("mStrideCommandCount", Value::Int(0)),
                ("mStrideSaveDepth", Value::Int(0)),
                ("mStrideWidth", Value::Int(self.surface.width as i32)),
                ("mStrideHeight", Value::Int(self.surface.height as i32)),
                ("mStrideBitmap", Value::Null),
            ] {
                self.heap.set_instance_field(canvas_id, field, value)
                    .map_err(|error| format!("Guest Canvas initialization failed: {:?}", error))?;
            }

            let method_map = self.class_loader.export_method_codes();
            let metadata = self.class_loader.dex_inventories.first().cloned();
            let strings = metadata.as_ref().map(|inventory| inventory.strings.as_slice()).unwrap_or(&[]);
            let mut vm = ArtVm::new(
                &mut self.heap, &mut self.hierarchy, &mut self.monitors,
                &mut self.jni, &method_map, strings,
            );
            vm.attach_diagnostics(&self.diagnostics);
            vm.attach_static_fields(&mut self.static_fields);
            vm.attach_multi_dex_metadata(&self.class_loader.dex_inventories, &self.class_loader.method_dex_indices);
            if let Some(inventory) = metadata.as_ref() {
                vm.attach_dex_metadata(&inventory.type_descriptors, &inventory.field_references);
                vm.attach_method_resolver(&inventory.method_references, &self.class_loader.method_lookup);
            }
            let report = vm.execute_method(
                method_id,
                &[Value::Object(view_id), Value::Object(canvas_id)],
                250_000,
            ).map_err(|error| format!("Guest onDraw interpreter error in {}: {}", class_descriptor, error))?;
            match report.stop_reason {
                ExecutionStopReason::UncaughtException(exception) => {
                    return Err(format!("Guest {}.onDraw threw {}: {:?}", class_descriptor, exception.class_descriptor, exception.message));
                }
                ExecutionStopReason::StepLimitExceeded => {
                    return Err(format!("Guest {}.onDraw exceeded its drawing instruction budget", class_descriptor));
                }
                _ => {}
            }

            self.flush_guest_bitmap_canvases()?;
            let command_count = crate::canvas_jni::recorded_command_count(&self.heap, canvas_id)?;
            Self::replay_guest_canvas_commands(&self.heap, canvas_id, &mut self.surface)?;
            self.heap.set_instance_field(canvas_id, "mStrideCommandCount", Value::Int(0))
                .map_err(|error| format!("Guest Canvas queue reset failed: {:?}", error))?;
            self.diagnostics.record_detailed(
                DiagnosticSeverity::Info, "Graphics", "GUEST_CUSTOM_VIEW_ONDRAW_EXECUTED",
                "App-owned custom View.onDraw(Canvas) executed and its drawing commands were replayed",
                Some(&format!("class={} instructions={} commands={}", class_descriptor, report.steps_executed, command_count)),
                None,
            );
        }
        Ok(())
    }

    fn hit_test_edit_text(&self, x: f32, y: f32) -> Option<u32> {
        self.rendered_view_bounds.iter().rev().find_map(|(view_id, bounds)| {
            let inside = x >= bounds.left && x <= bounds.right
                && y >= bounds.top && y <= bounds.bottom;
            if !inside { return None; }
            let enabled = self.heap.get_instance_field(*view_id, "mEnabled")
                .map(|value| value.as_int() != 0).unwrap_or(true);
            if !enabled { return None; }
            let descriptor = self.heap.get(*view_id).ok()?.class_descriptor();
            (descriptor == "Landroid/widget/EditText;" || descriptor.ends_with("/EditText;"))
                .then_some(*view_id)
        })
    }

    fn dispatch_text_watcher_callbacks(
        &mut self,
        view_id: u32,
        old_text_id: u32,
        new_text_id: u32,
        old_length: usize,
        new_length: usize,
        step_budget: usize,
    ) -> Result<(), String> {
        let watcher_id = self.heap.get_instance_field(view_id, "mTextWatcher")
            .ok().and_then(|value| value.as_object()).filter(|id| *id != 0);
        let Some(watcher_id) = watcher_id else { return Ok(()) };
        let watcher_class = self.heap.get(watcher_id)
            .map_err(|error| format!("TextWatcher lookup failed: {:?}", error))?
            .class_descriptor().to_owned();
        let callbacks = [
            ("beforeTextChanged", "(Ljava/lang/CharSequence;III)V", old_text_id, old_length, new_length),
            ("onTextChanged", "(Ljava/lang/CharSequence;III)V", new_text_id, old_length, new_length),
        ];
        let resolved: Vec<_> = callbacks.iter().filter_map(|(name, proto, text_id, before, after)| {
            self.class_loader.find_method(&watcher_class, name, proto)
                .map(|id| (*name, id, *text_id, *before, *after))
        }).collect();
        if resolved.is_empty() { return Ok(()) }
        let method_map = self.class_loader.export_method_codes();
        let dex_metadata = self.class_loader.dex_inventories.first().cloned();
        let dex_strings = dex_metadata.as_ref().map(|inventory| inventory.strings.as_slice()).unwrap_or(&[]);
        let mut vm = ArtVm::new(
            &mut self.heap, &mut self.hierarchy, &mut self.monitors,
            &mut self.jni, &method_map, dex_strings,
        );
        vm.attach_diagnostics(&self.diagnostics);
        vm.attach_static_fields(&mut self.static_fields);
        vm.attach_multi_dex_metadata(&self.class_loader.dex_inventories, &self.class_loader.method_dex_indices);
        if let Some(inventory) = dex_metadata.as_ref() {
            vm.attach_dex_metadata(&inventory.type_descriptors, &inventory.field_references);
            vm.attach_method_resolver(&inventory.method_references, &self.class_loader.method_lookup);
        }
        for (name, id, text_id, before, after) in resolved {
            let method_id = u16::try_from(id)
                .map_err(|_| format!("TextWatcher.{} method ID exceeds interpreter limit", name))?;
            let args = [
                Value::Object(watcher_id),
                Value::Object(text_id),
                Value::Int(0),
                Value::Int(before.min(i32::MAX as usize) as i32),
                Value::Int(after.min(i32::MAX as usize) as i32),
            ];
            let report = vm.execute_method(method_id, &args, step_budget.max(1))
                .map_err(|error| format!("TextWatcher.{} failed: {}", name, error))?;
            match report.stop_reason {
                ExecutionStopReason::UncaughtException(exception) =>
                    return Err(format!("TextWatcher.{} threw {}: {:?}", name, exception.class_descriptor, exception.message)),
                ExecutionStopReason::StepLimitExceeded =>
                    return Err(format!("TextWatcher.{} exceeded its instruction budget", name)),
                _ => {}
            }
        }
        Ok(())
    }

    /// Replaces the text in the EditText selected by the most recent guest tap.
    /// Text is stored in the guest heap and the next FFI render publishes the new frame.
    pub fn set_focused_edit_text(&mut self, text: &str) -> Result<bool, String> {
        if text.len() > 1_048_576 {
            return Err("Guest text input exceeds the 1 MiB safety limit".to_string());
        }
        let Some(view_id) = self.focused_edit_text else { return Ok(false) };
        let previous_value = self.heap.get_instance_field(view_id, "mText").unwrap_or(Value::Null);
        let old_text = previous_value.as_object()
            .and_then(|id| self.heap.get_string(id).ok().map(str::to_owned))
            .unwrap_or_default();
        let old_text_id = previous_value.as_object()
            .filter(|id| *id != 0)
            .unwrap_or(self.heap.allocate_string(&old_text)
                .map_err(|error| format!("Previous EditText string allocation failed: {:?}", error))?);
        let string_id = self.heap.allocate_string(text)
            .map_err(|error| format!("Guest input string allocation failed: {:?}", error))?;
        self.heap.set_instance_field(view_id, "mText", Value::Object(string_id))
            .map_err(|error| format!("EditText text update failed: {:?}", error))?;
        self.heap.set_instance_field(view_id, "mTextResourceId", Value::Int(0))
            .map_err(|error| format!("EditText resource reset failed: {:?}", error))?;
        self.heap.set_instance_field(view_id, "mSelectionStart", Value::Int(text.chars().count() as i32))
            .map_err(|error| format!("EditText selection update failed: {:?}", error))?;
        self.heap.set_instance_field(view_id, "mSelectionEnd", Value::Int(text.chars().count() as i32))
            .map_err(|error| format!("EditText selection update failed: {:?}", error))?;
        self.heap.set_instance_field(view_id, "mNeedsDraw", Value::Int(1))
            .map_err(|error| format!("EditText redraw flag update failed: {:?}", error))?;
        self.dispatch_text_watcher_callbacks(
            view_id, old_text_id, string_id, old_text.chars().count(),
            text.chars().count(), 20_000,
        )?;
        Ok(true)
    }

    fn hit_test_clickable_view(&self, x: f32, y: f32) -> Option<u32> {
        self.rendered_view_bounds.iter().rev().find_map(|(view_id, bounds)| {
            let inside = x >= bounds.left && x <= bounds.right
                && y >= bounds.top && y <= bounds.bottom;
            if !inside { return None; }
            let click_listener = self.heap.get_instance_field(*view_id, "mOnClickListener")
                .ok().and_then(|value| value.as_object()).filter(|id| *id != 0);
            let touch_listener = self.heap.get_instance_field(*view_id, "mOnTouchListener")
                .ok().and_then(|value| value.as_object()).filter(|id| *id != 0);
            let enabled = self.heap.get_instance_field(*view_id, "mEnabled")
                .map(|value| value.as_int() != 0).unwrap_or(true);
            if enabled && (click_listener.is_some() || touch_listener.is_some()) { Some(*view_id) } else { None }
        })
    }

    fn dispatch_view_touch(&mut self, view_id: u32, action: i32, x: f32, y: f32, step_budget: usize) -> Result<bool, String> {
        let listener_id = self.heap.get_instance_field(view_id, "mOnTouchListener")
            .map_err(|error| format!("View touch listener lookup failed: {:?}", error))?
            .as_object().filter(|id| *id != 0);
        let Some(listener_id) = listener_id else { return Ok(false) };
        let listener_class = self.heap.get(listener_id)
            .map_err(|error| format!("Touch listener object lookup failed: {:?}", error))?
            .class_descriptor().to_owned();
        let method_id = self.class_loader.find_method(&listener_class, "onTouch", "(Landroid/view/View;Landroid/view/MotionEvent;)Z");
        let Some(method_id) = method_id else { return Ok(false) };
        let method_id = u16::try_from(method_id).map_err(|_| "View touch listener method ID exceeds interpreter limit".to_string())?;
        let event_id = self.heap.allocate_instance("Landroid/view/MotionEvent;")
            .map_err(|error| format!("MotionEvent allocation failed: {:?}", error))?;
        self.heap.set_instance_field(event_id, "action", Value::Int(action)).map_err(|e| format!("MotionEvent action setup failed: {:?}", e))?;
        self.heap.set_instance_field(event_id, "x", Value::Float(x)).map_err(|e| format!("MotionEvent X setup failed: {:?}", e))?;
        self.heap.set_instance_field(event_id, "y", Value::Float(y)).map_err(|e| format!("MotionEvent Y setup failed: {:?}", e))?;
        let method_map = self.class_loader.export_method_codes();
        let metadata = self.class_loader.dex_inventories.first().cloned();
        let strings = metadata.as_ref().map(|i| i.strings.as_slice()).unwrap_or(&[]);
        let mut vm = ArtVm::new(&mut self.heap, &mut self.hierarchy, &mut self.monitors, &mut self.jni, &method_map, strings);
        vm.attach_diagnostics(&self.diagnostics);
        vm.attach_static_fields(&mut self.static_fields);
                vm.attach_multi_dex_metadata(&self.class_loader.dex_inventories, &self.class_loader.method_dex_indices);
        if let Some(inventory) = metadata.as_ref() {
            vm.attach_dex_metadata(&inventory.type_descriptors, &inventory.field_references);
            vm.attach_method_resolver(&inventory.method_references, &self.class_loader.method_lookup);
        }
        let args = [Value::Object(listener_id), Value::Object(view_id), Value::Object(event_id)];
        let report = vm.execute_method(method_id, &args, step_budget.max(1))
            .map_err(|error| format!("View touch listener interpreter error: {}", error))?;
        match report.stop_reason {
            ExecutionStopReason::ReturnedValue(value) => Ok(value.as_int() != 0),
            ExecutionStopReason::UncaughtException(exception) => Err(format!("View touch listener threw {}: {:?}", exception.class_descriptor, exception.message)),
            ExecutionStopReason::StepLimitExceeded => Err("View touch listener exceeded its per-event instruction budget".to_string()),
            ExecutionStopReason::ReturnedVoid => Ok(false),
        }
    }
    fn dispatch_view_click(&mut self, view_id: u32, step_budget: usize) -> Result<bool, String> {
        let listener_id = self.heap.get_instance_field(view_id, "mOnClickListener")
            .map_err(|error| format!("View click listener lookup failed: {:?}", error))?
            .as_object().filter(|id| *id != 0);
        let Some(listener_id) = listener_id else { return Ok(false) };
        let listener_class = self.heap.get(listener_id)
            .map_err(|error| format!("Click listener object lookup failed: {:?}", error))?
            .class_descriptor().to_owned();
        let Some(method_id) = self.class_loader.find_method(
            &listener_class, "onClick", "(Landroid/view/View;)V"
        ) else {
            self.diagnostics.record(
                DiagnosticSeverity::MissingApi,
                "Input",
                "VIEW_CLICK_LISTENER_METHOD_MISSING",
                Some(&format!("{}->onClick(Landroid/view/View;)V", listener_class)),
            );
            return Ok(false);
        };
        let method_id = u16::try_from(method_id)
            .map_err(|_| "View click listener method ID exceeds the interpreter limit".to_string())?;
        let method_map = self.class_loader.export_method_codes();
        let dex_metadata = self.class_loader.dex_inventories.first().cloned();
        let dex_strings = dex_metadata.as_ref()
            .map(|inventory| inventory.strings.as_slice()).unwrap_or(&[]);
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
                vm.attach_multi_dex_metadata(&self.class_loader.dex_inventories, &self.class_loader.method_dex_indices);
        if let Some(inventory) = dex_metadata.as_ref() {
            vm.attach_dex_metadata(&inventory.type_descriptors, &inventory.field_references);
            vm.attach_method_resolver(&inventory.method_references, &self.class_loader.method_lookup);
        }
        let args = [Value::Object(listener_id), Value::Object(view_id)];
        let report = vm.execute_method(method_id, &args, step_budget.max(1))
            .map_err(|error| format!("View click listener interpreter error: {}", error))?;
        match report.stop_reason {
            ExecutionStopReason::UncaughtException(exception) => Err(format!(
                "View click listener threw {}: {:?}", exception.class_descriptor, exception.message
            )),
            ExecutionStopReason::StepLimitExceeded => Err(
                "View click listener exceeded its per-event instruction budget".to_string()
            ),
            _ => Ok(true),
        }
    }

    /// Pumps queued Android touch events on the host display-link cadence.
    /// Activity touch overrides run first; when they do not consume the gesture,
    /// ordinary View.setOnClickListener callbacks receive matching down/up taps.
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
        let callback_method_id = callback.map(|id| u16::try_from(id)
            .map_err(|_| "Touch callback method ID exceeds the interpreter limit".to_string()))
            .transpose()?;
        let method_map = self.class_loader.export_method_codes();
        let dex_metadata = self.class_loader.dex_inventories.first().cloned();
        let dex_strings = dex_metadata.as_ref()
            .map(|inventory| inventory.strings.as_slice()).unwrap_or(&[]);
        let mut delivered = 0usize;
        let mut had_events = false;

        while delivered < max_events {
            let Some(event) = self.input_queue.poll_motion() else { break };
            had_events = true;
            let pointer = event.pointers.first();
            let x = pointer.map(|p| p.x).unwrap_or(0.0);
            let y = pointer.map(|p| p.y).unwrap_or(0.0);
            if event.action == ACTION_DOWN {
                self.focused_edit_text = self.hit_test_edit_text(x, y);
            }
            let mut activity_consumed = false;

            if let Some(method_id) = callback_method_id {
                let event_obj = self.heap.allocate_instance("Landroid/view/MotionEvent;")
                    .map_err(|error| format!("MotionEvent allocation failed: {:?}", error))?;
                self.heap.set_instance_field(event_obj, "action", Value::Int(event.action))
                    .map_err(|error| format!("MotionEvent action setup failed: {:?}", error))?;
                self.heap.set_instance_field(event_obj, "x", Value::Float(x))
                    .map_err(|error| format!("MotionEvent X setup failed: {:?}", error))?;
                self.heap.set_instance_field(event_obj, "y", Value::Float(y))
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
                vm.attach_multi_dex_metadata(&self.class_loader.dex_inventories, &self.class_loader.method_dex_indices);
                if let Some(inventory) = dex_metadata.as_ref() {
                    vm.attach_dex_metadata(&inventory.type_descriptors, &inventory.field_references);
                    vm.attach_method_resolver(&inventory.method_references, &self.class_loader.method_lookup);
                }
                let args = [Value::Object(activity_obj), Value::Object(event_obj)];
                let report = vm.execute_method(method_id, &args, step_budget.max(1))
                    .map_err(|error| format!("Touch callback interpreter error: {}", error))?;
                match report.stop_reason {
                    ExecutionStopReason::UncaughtException(exception) => return Err(format!(
                        "Touch callback threw {}: {:?}", exception.class_descriptor, exception.message
                    )),
                    ExecutionStopReason::StepLimitExceeded => return Err(
                        "Touch callback exceeded its per-event instruction budget".to_string()
                    ),
                    ExecutionStopReason::ReturnedValue(value) => activity_consumed = value.as_int() != 0,
                    ExecutionStopReason::ReturnedVoid => activity_consumed = false,
                }
            }

            if !activity_consumed {
                let touch_target = self.pressed_view.or_else(|| self.hit_test_clickable_view(x, y));
                if let Some(view_id) = touch_target {
                    activity_consumed = self.dispatch_view_touch(view_id, event.action, x, y, step_budget)?;
                }
            }

            match event.action {
                ACTION_DOWN => {
                    self.pressed_view = if activity_consumed { None } else { self.hit_test_clickable_view(x, y) };
                }
                ACTION_UP => {
                    let released_view = self.hit_test_clickable_view(x, y);
                    let pressed_view = self.pressed_view.take();
                    if !activity_consumed && pressed_view.is_some() && pressed_view == released_view {
                        if let Some(view_id) = released_view {
                            if self.dispatch_view_click(view_id, step_budget)? {
                                delivered += 1;
                                continue;
                            }
                        }
                    }
                }
                ACTION_CANCEL => self.pressed_view = None,
                _ => {}
            }

            if callback_method_id.is_some() {
                delivered += 1;
            }
        }
        if had_events {
            self.render_active_activity_surface()
                .map_err(|error| format!("Guest redraw after input failed: {error}"))?;
        }
        Ok(delivered)
    }
}


#[cfg(test)]
mod framework_bridge_tests {
    use super::*;

    #[test]
    fn focused_edit_text_receives_native_keyboard_updates() {
        let mut runtime = AndroidRuntime::new("com.stride.test");
        let edit = runtime.heap.allocate_instance("Landroid/widget/EditText;").unwrap();
        runtime.heap.set_instance_field(edit, "mEnabled", Value::Int(1)).unwrap();
        runtime.rendered_view_bounds = vec![(edit, RectF::new(100.0, 200.0, 500.0, 280.0))];

        assert_eq!(runtime.hit_test_edit_text(250.0, 240.0), Some(edit));
        assert_eq!(runtime.hit_test_edit_text(20.0, 20.0), None);
        runtime.focused_edit_text = runtime.hit_test_edit_text(250.0, 240.0);
        assert!(runtime.set_focused_edit_text("42 + 7").unwrap());

        let text_id = runtime.heap.get_instance_field(edit, "mText").unwrap().as_object().unwrap();
        assert_eq!(runtime.heap.get_string(text_id).unwrap(), "42 + 7");
        assert_eq!(runtime.heap.get_instance_field(edit, "mSelectionEnd").unwrap().as_int(), 6);

        runtime.focused_edit_text = None;
        assert!(!runtime.set_focused_edit_text("ignored").unwrap());
    }

    #[test]
    fn managed_text_view_renders_initial_display_pixels_on_black_surface() {
        let mut runtime = AndroidRuntime::new("com.stride.test");
        let activity = runtime.heap.allocate_instance("Ltest/MainActivity;").unwrap();
        let root = runtime.heap.allocate_instance("Landroid/widget/LinearLayout;").unwrap();
        let display = runtime.heap.allocate_instance("Landroid/widget/TextView;").unwrap();
        let zero = runtime.heap.allocate_string("0").unwrap();

        runtime.heap.set_instance_field(activity, "mContentView", Value::Object(root)).unwrap();
        runtime.heap.set_instance_field(root, "mVisibility", Value::Int(0)).unwrap();
        runtime.heap.set_instance_field(root, "mBackgroundColor", Value::Int(0xff000000u32 as i32)).unwrap();
        runtime.heap.set_instance_field(root, "mOrientation", Value::Int(1)).unwrap();
        runtime.heap.set_instance_field(root, "mChildCount", Value::Int(6)).unwrap();
        runtime.heap.set_instance_field(root, "mChild0", Value::Object(display)).unwrap();
        // Match the calculator's six vertical segments: one display plus five key rows.
        for index in 0..5 {
            let row = runtime.heap.allocate_instance("Landroid/widget/LinearLayout;").unwrap();
            runtime.heap.set_instance_field(row, "mVisibility", Value::Int(0)).unwrap();
            runtime.heap.set_instance_field(row, "mOrientation", Value::Int(0)).unwrap();
            runtime.heap.set_instance_field(row, "mChildCount", Value::Int(0)).unwrap();
            runtime.heap.set_instance_field(root, &format!("mChild{}", index + 1), Value::Object(row)).unwrap();
        }
        runtime.heap.set_instance_field(display, "mVisibility", Value::Int(0)).unwrap();
        runtime.heap.set_instance_field(display, "mBackgroundColor", Value::Int(0xff000000u32 as i32)).unwrap();
        runtime.heap.set_instance_field(display, "mText", Value::Object(zero)).unwrap();
        runtime.heap.set_instance_field(display, "mTextColor", Value::Int(0xffffffffu32 as i32)).unwrap();
        runtime.heap.set_instance_field(display, "mTextSize", Value::Float(42.0)).unwrap();
        runtime.active_activity_obj = Some(activity);

        assert!(runtime.render_active_activity_surface().unwrap());
        let white_display_pixels = runtime.surface.as_slice()
            .iter()
            .take(1080 * 320)
            .filter(|pixel| **pixel == 0xffffffff)
            .count();
        assert!(white_display_pixels > 0, "the white initial value must be rasterized in the display region");

        let diagnostics = runtime.diagnostics.export_text();
        assert!(diagnostics.contains("ACTIVITY_TEXT_RENDER_SUMMARY"), "{diagnostics}");
        assert!(diagnostics.contains("text_views_with_text=1"), "{diagnostics}");
    }

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


#[cfg(test)]
mod managed_view_input_tests {
    use super::*;

    #[test]
    fn hit_testing_finds_topmost_enabled_view_with_click_listener() {
        let mut runtime = AndroidRuntime::new("com.stride.test");
        let root = runtime.heap.allocate_instance("Landroid/widget/LinearLayout;").unwrap();
        let button = runtime.heap.allocate_instance("Landroid/widget/TextView;").unwrap();
        let listener = runtime.heap.allocate_instance("Ltest/CalculatorClickListener;").unwrap();
        runtime.heap.set_instance_field(button, "mOnClickListener", Value::Object(listener)).unwrap();
        runtime.heap.set_instance_field(button, "mEnabled", Value::Int(1)).unwrap();
        runtime.rendered_view_bounds = vec![
            (root, RectF::new(0.0, 0.0, 1080.0, 1920.0)),
            (button, RectF::new(100.0, 300.0, 500.0, 600.0)),
        ];

        assert_eq!(runtime.hit_test_clickable_view(250.0, 450.0), Some(button));
        assert_eq!(runtime.hit_test_clickable_view(700.0, 450.0), None);
    }

    #[test]
    fn disabled_view_is_not_a_click_target() {
        let mut runtime = AndroidRuntime::new("com.stride.test");
        let button = runtime.heap.allocate_instance("Landroid/widget/TextView;").unwrap();
        let listener = runtime.heap.allocate_instance("Ltest/CalculatorClickListener;").unwrap();
        runtime.heap.set_instance_field(button, "mOnClickListener", Value::Object(listener)).unwrap();
        runtime.heap.set_instance_field(button, "mEnabled", Value::Int(0)).unwrap();
        runtime.rendered_view_bounds = vec![(button, RectF::new(0.0, 0.0, 100.0, 100.0))];

        assert_eq!(runtime.hit_test_clickable_view(50.0, 50.0), None);
    }
}
