//! C ABI for passing DEX inventory summaries and ART runtime execution to Swift.

use std::ffi::CStr;
use std::os::raw::c_char;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::slice;

use stride_dex_runtime::{
    AndroidRuntime, LaunchReport, LaunchStatus, ActivityLifecycleState,
    MotionEvent, PointerCoords, ACTION_DOWN, ACTION_UP, ACTION_MOVE, ACTION_CANCEL,
    GL_COLOR_BUFFER_BIT,
};

/// Summary returned after a successful DEX metadata scan.
#[repr(C)]
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct StrideDexSummary {
    pub class_count: u32,
    pub method_count: u32,
}

/// Execution report returned from the ART runtime launch pipeline.
#[repr(C)]
#[derive(Debug, Default, Clone, Copy, PartialEq)]
pub struct StrideLaunchReport {
    /// 0: running, 1: finished normally, 2: uncaught exception, 3: activity not found, 4: manifest error
    pub status_code: i32,
    pub total_classes_loaded: u32,
    pub instructions_executed: u32,
    /// 0: not started, 1: created, 2: started, 3: resumed, 4: paused, 5: stopped, 6: destroyed
    pub lifecycle_state: i32,
    pub missing_apis_count: u32,
    pub average_fps: f32,
    pub heap_allocated_bytes: u32,
}

/// Inspect one DEX buffer and return the total class and encoded method counts.
#[no_mangle]
pub unsafe extern "C" fn stride_dex_inventory_counts(
    data: *const u8,
    len: usize,
    out: *mut StrideDexSummary,
) -> i32 {
    if data.is_null() || out.is_null() {
        return 1;
    }
    if len > isize::MAX as usize {
        return 2;
    }

    let result = catch_unwind(AssertUnwindSafe(|| {
        let bytes = unsafe { slice::from_raw_parts(data, len) };
        let inventory = stride_dex_inspector::classes::inspect_classes(bytes)
            .map_err(|_| 3i32)?;
        let class_count = u32::try_from(inventory.classes.len()).map_err(|_| 5i32)?;
        let method_count = inventory.classes.iter().try_fold(0u32, |total, class| {
            total.checked_add(u32::try_from(class.methods.len()).ok()?)
        }).ok_or(5i32)?;
        Ok::<_, i32>(StrideDexSummary { class_count, method_count })
    }));

    match result {
        Ok(Ok(summary)) => {
            unsafe { out.write(summary) };
            0
        }
        Ok(Err(code)) => {
            unsafe { out.write(StrideDexSummary::default()) };
            code
        }
        Err(_) => {
            unsafe { out.write(StrideDexSummary::default()) };
            4
        }
    }
}

/// Creates a new ART-compatible Android runtime instance.
#[no_mangle]
pub unsafe extern "C" fn stride_runtime_create(package_name: *const c_char) -> *mut AndroidRuntime {
    let pkg = if !package_name.is_null() {
        unsafe { CStr::from_ptr(package_name).to_str().unwrap_or("unknown") }
    } else {
        "unknown"
    };

    let runtime = Box::new(AndroidRuntime::new(pkg));
    Box::into_raw(runtime)
}



/// Copy resource file entries as packed (id, UTF-8 path) records.
#[no_mangle]
pub unsafe extern "C" fn stride_runtime_resource_file_count(runtime:*const AndroidRuntime)->usize{
 if runtime.is_null(){return 0;} unsafe{(&*runtime).resources.file_entries().len()}
}
#[no_mangle]
pub unsafe extern "C" fn stride_runtime_resource_file_entry(runtime:*const AndroidRuntime,index:usize,out_id:*mut u32,out_path:*mut u8,capacity:usize)->usize{
 if runtime.is_null()||out_id.is_null(){return 0;}let entries=unsafe{(&*runtime).resources.file_entries()};
 let Some((id,path))=entries.get(index)else{return 0};let bytes=path.as_bytes();unsafe{out_id.write(*id)};
 if !out_path.is_null()&&capacity>0{let n=bytes.len().min(capacity-1);unsafe{std::ptr::copy_nonoverlapping(bytes.as_ptr(),out_path,n);out_path.add(n).write(0);}}
 bytes.len()+1
}
#[no_mangle]
pub unsafe extern "C" fn stride_runtime_set_resource_image(runtime:*mut AndroidRuntime,id:u32,width:u32,height:u32,pixels:*const u32,count:usize)->i32{
 if runtime.is_null()||pixels.is_null()||count>16_777_216{return 1;}let data=unsafe{slice::from_raw_parts(pixels,count)};
 match catch_unwind(AssertUnwindSafe(||unsafe{(&mut *runtime).set_resource_image(id,width,height,data)})){Ok(Ok(()))=>0,Ok(Err(_))=>2,Err(_)=>3}
}

/// Loads Android resources.arsc into the runtime resource catalog.
#[no_mangle]
pub unsafe extern "C" fn stride_runtime_load_resource_table(runtime:*mut AndroidRuntime,data:*const u8,len:usize)->i32{
 if runtime.is_null()||data.is_null()||len==0||len>isize::MAX as usize{return 1;}
 let rt=unsafe{&mut *runtime};let bytes=unsafe{slice::from_raw_parts(data,len)};
 match catch_unwind(AssertUnwindSafe(||rt.load_resource_table(bytes))){Ok(Ok(()))=>0,Ok(Err(_))=>2,Err(_)=>3}
}

/// Loads a DEX binary into an active Android runtime instance.
/// Returns 0 on success, 1 on null pointer, 2 on invalid DEX.
#[no_mangle]
pub unsafe extern "C" fn stride_runtime_load_dex(
    runtime: *mut AndroidRuntime,
    data: *const u8,
    len: usize,
) -> i32 {
    if runtime.is_null() || data.is_null() || len == 0 {
        return 1;
    }

    let rt = unsafe { &mut *runtime };
    let bytes = unsafe { slice::from_raw_parts(data, len) };

    match rt.load_dex(bytes) {
        Ok(_) => 0,
        Err(_) => 2,
    }
}

/// Loads and links an ELF shared library (.so) into the active Android runtime.
#[no_mangle]
pub unsafe extern "C" fn stride_runtime_load_so(
    runtime: *mut AndroidRuntime,
    soname: *const c_char,
    data: *const u8,
    len: usize,
) -> i32 {
    if runtime.is_null() || data.is_null() || len == 0 {
        return 1;
    }
    let name = if !soname.is_null() {
        CStr::from_ptr(soname).to_str().unwrap_or("libnative.so")
    } else {
        "libnative.so"
    };

    let rt = unsafe { &mut *runtime };
    let bytes = unsafe { slice::from_raw_parts(data, len) };

    match rt.load_native_library(name, bytes) {
        Ok(_) => 0,
        Err(_) => 2,
    }
}


#[no_mangle]
pub unsafe extern "C" fn stride_manifest_screen_orientation(
    manifest_data: *const u8, manifest_len: usize, out_orientation: *mut i32,
) -> i32 {
    if manifest_data.is_null() || out_orientation.is_null() || manifest_len == 0
        || manifest_len > isize::MAX as usize { return 1; }
    let bytes = unsafe { slice::from_raw_parts(manifest_data, manifest_len) };
    match stride_dex_runtime::parse_manifest(bytes) {
        Ok(manifest) => { unsafe { out_orientation.write(manifest.screen_orientation.unwrap_or(-1)) }; 0 }
        Err(_) => 2,
    }
}

/// Launches the main Activity from AndroidManifest.xml and executes its lifecycle.
#[no_mangle]
pub unsafe extern "C" fn stride_runtime_launch_apk(
    runtime: *mut AndroidRuntime,
    manifest_data: *const u8,
    manifest_len: usize,
    step_budget: usize,
    out_report: *mut StrideLaunchReport,
) -> i32 {
    if runtime.is_null() || manifest_data.is_null() || manifest_len == 0 || out_report.is_null() {
        return 1;
    }

    let rt = unsafe { &mut *runtime };
    let manifest_bytes = unsafe { slice::from_raw_parts(manifest_data, manifest_len) };

    let report: LaunchReport = rt.launch_application(manifest_bytes, step_budget);

    let status_code = match report.status {
        // Lifecycle dispatch completed; this is not a claim that guest frames are running.
        LaunchStatus::LifecycleReady => 5,
        LaunchStatus::FinishedNormally => 1,
        LaunchStatus::UncaughtException { .. } => 2,
        LaunchStatus::ActivityNotFound(_) => 3,
        LaunchStatus::ManifestError(_) => 4,
    };

    let lifecycle_state = match report.lifecycle_state {
        ActivityLifecycleState::NotStarted => 0,
        ActivityLifecycleState::Created => 1,
        ActivityLifecycleState::Started => 2,
        ActivityLifecycleState::Resumed => 3,
        ActivityLifecycleState::Paused => 4,
        ActivityLifecycleState::Stopped => 5,
        ActivityLifecycleState::Destroyed => 6,
    };

    let missing_apis_count = rt.diagnostics.count_by_severity(stride_dex_runtime::DiagnosticSeverity::MissingApi) as u32;

    let summary = StrideLaunchReport {
        status_code,
        total_classes_loaded: report.total_classes_loaded as u32,
        instructions_executed: report.instructions_executed as u32,
        lifecycle_state,
        missing_apis_count,
        average_fps: rt.telemetry.current_fps(),
        heap_allocated_bytes: rt.heap.allocated_bytes() as u32,
    };

    unsafe { out_report.write(summary) };
    0
}

/// Dispatches a touch event to the active runtime input queue.
#[no_mangle]
pub unsafe extern "C" fn stride_runtime_dispatch_touch(
    runtime: *mut AndroidRuntime,
    action: i32,
    x: f32,
    y: f32,
) -> i32 {
    if runtime.is_null() {
        return 1;
    }

    let rt = unsafe { &*runtime };
    let motion_action = match action {
        0 => ACTION_DOWN,
        1 => ACTION_UP,
        2 => ACTION_MOVE,
        _ => ACTION_CANCEL,
    };

    let event = MotionEvent {
        action: motion_action,
        event_time_ms: 0,
        pointers: vec![PointerCoords {
            id: 0,
            x,
            y,
            pressure: 1.0,
        }],
    };

    rt.input_queue.enqueue_motion(event);
    // Record discrete input transitions; high-frequency MOVE events remain
    // in the input queue and are deliberately not emitted as individual logs.
    if motion_action != ACTION_MOVE {
        let action_name = match motion_action {
            ACTION_DOWN => "DOWN",
            ACTION_UP => "UP",
            ACTION_CANCEL => "CANCEL",
            _ => "OTHER",
        };
        rt.diagnostics.record(
            stride_dex_runtime::DiagnosticSeverity::Info,
            "Input",
            &format!("Touch {}", action_name),
            Some(&format!("x={:.1}, y={:.1}", x, y)),
        );
    }
    0
}

/// Delivers queued Android touch events to the active Activity callback.
/// Returns 0 on success, 1 for invalid arguments, 2 for a guest callback error,
/// and 3 if the runtime panics while processing the event batch.
#[no_mangle]
pub unsafe extern "C" fn stride_runtime_pump_guest_events(
    runtime: *mut AndroidRuntime,
    max_events: u32,
    step_budget: usize,
    out_delivered: *mut u32,
) -> i32 {
    if runtime.is_null() || out_delivered.is_null() {
        return 1;
    }
    let rt = unsafe { &mut *runtime };
    let result = catch_unwind(AssertUnwindSafe(|| {
        let delivered=rt.pump_guest_events(max_events as usize,step_budget)?;
        // Re-render after every guest event batch so View mutations reach the host framebuffer.
        rt.render_active_activity_surface()?;
        Ok::<usize,String>(delivered)
    }));
    match result {
        Ok(Ok(delivered)) => {
            unsafe { out_delivered.write(delivered.min(u32::MAX as usize) as u32) };
            0
        }
        Ok(Err(error)) => {
            rt.diagnostics.record(
                stride_dex_runtime::DiagnosticSeverity::RuntimeError,
                "Input",
                "GUEST_TOUCH_DISPATCH_FAILED",
                Some(&error),
            );
            unsafe { out_delivered.write(0) };
            2
        }
        Err(_) => {
            rt.diagnostics.record(
                stride_dex_runtime::DiagnosticSeverity::FatalCrash,
                "Input",
                "GUEST_EVENT_PUMP_PANIC",
                Some("Rust runtime panicked while pumping guest input events"),
            );
            unsafe { out_delivered.write(0) };
            3
        }
    }
}

/// Records a completed frame time and returns the updated rolling FPS.
#[no_mangle]
pub unsafe extern "C" fn stride_runtime_record_frame(
    runtime: *mut AndroidRuntime,
    duration_ms: f32,
) -> f32 {
    if runtime.is_null() {
        return 0.0;
    }
    let rt = unsafe { &mut *runtime };
    rt.telemetry.record_frame(duration_ms);
    rt.telemetry.current_fps()
}

/// Swaps the active OpenGL ES / EGL buffer to the runtime surface.
#[no_mangle]
pub unsafe extern "C" fn stride_runtime_gl_swap_buffers(runtime: *mut AndroidRuntime) -> i32 {
    if runtime.is_null() {
        return 1;
    }
    let rt = unsafe { &mut *runtime };
    let target = &mut rt.surface;
    rt.gl_engine.egl_swap_buffers(target)
}

/// Sets the OpenGL ES viewport.
#[no_mangle]
pub unsafe extern "C" fn stride_runtime_gl_viewport(
    runtime: *mut AndroidRuntime,
    x: i32,
    y: i32,
    width: i32,
    height: i32,
) -> i32 {
    if runtime.is_null() {
        return 1;
    }
    let rt = unsafe { &mut *runtime };
    rt.gl_engine.gl_viewport(x, y, width, height);
    0
}

/// Clears the OpenGL ES color buffer.
#[no_mangle]
pub unsafe extern "C" fn stride_runtime_gl_clear(
    runtime: *mut AndroidRuntime,
    r: f32,
    g: f32,
    b: f32,
    a: f32,
) -> i32 {
    if runtime.is_null() {
        return 1;
    }
    let rt = unsafe { &mut *runtime };
    rt.gl_engine.gl_clear_color(r, g, b, a);
    rt.gl_engine.gl_clear(GL_COLOR_BUFFER_BIT);
    0
}

/// Copies the guest Surface into caller-owned ARGB8888 pixels. Query with null pixels first.
#[no_mangle]
pub unsafe extern "C" fn stride_runtime_copy_framebuffer(runtime:*const AndroidRuntime,out_width:*mut u32,out_height:*mut u32,out_pixels:*mut u32,capacity_pixels:usize)->i32{
 if runtime.is_null()||out_width.is_null()||out_height.is_null(){return 1;}
 let surface=&unsafe{&*runtime}.surface;
 let Some(required)=(surface.width as usize).checked_mul(surface.height as usize)else{return 2;};
 unsafe{out_width.write(surface.width);out_height.write(surface.height);}
 if out_pixels.is_null()||capacity_pixels<required{return 3;}
 unsafe{std::ptr::copy_nonoverlapping(surface.pixels.as_ptr(),out_pixels,required);}0
}

/// Copies the current diagnostic report into a caller-owned UTF-8 buffer.
/// Returns the required byte count including the trailing NUL. If out is null
/// or capacity is too small, no bytes are written; call once for size, then again.
#[no_mangle]
pub unsafe extern "C" fn stride_runtime_diagnostics_export(
    runtime: *const AndroidRuntime,
    out: *mut c_char,
    capacity: usize,
) -> usize {
    if runtime.is_null() {
        return 0;
    }
    let report = unsafe { &*runtime }.diagnostics.export_text();
    let bytes = report.as_bytes();
    let required = bytes.len().saturating_add(1);
    if out.is_null() || capacity < required {
        return required;
    }
    unsafe {
        std::ptr::copy_nonoverlapping(bytes.as_ptr(), out.cast::<u8>(), bytes.len());
        out.add(bytes.len()).write(0);
    }
    required
}

/// Destroys an Android runtime instance and frees its resources.
#[no_mangle]
pub unsafe extern "C" fn stride_runtime_destroy(runtime: *mut AndroidRuntime) {
    if !runtime.is_null() {
        drop(unsafe { Box::from_raw(runtime) });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_null_pointers() {
        let mut summary = StrideDexSummary::default();
        assert_eq!(unsafe {
            stride_dex_inventory_counts(std::ptr::null(), 0, &mut summary)
        }, 1);
        assert_eq!(unsafe {
            stride_dex_inventory_counts([0u8; 1].as_ptr(), 1, std::ptr::null_mut())
        }, 1);
    }

    #[test]
    fn reports_invalid_dex_without_exposing_rust_errors() {
        let bytes = [0u8; 8];
        let mut summary = StrideDexSummary { class_count: 99, method_count: 99 };
        let status = unsafe {
            stride_dex_inventory_counts(bytes.as_ptr(), bytes.len(), &mut summary)
        };
        assert_eq!(status, 3);
        assert_eq!(summary, StrideDexSummary::default());
    }

    #[test]
    fn exports_exact_dex_loader_failure_to_session_diagnostics() {
        unsafe {
            let rt = stride_runtime_create(b"com.stride.diagnostic.test\\0".as_ptr() as *const c_char);
            assert!(!rt.is_null());
            let invalid_dex = [0u8; 8];
            assert_eq!(stride_runtime_load_dex(rt, invalid_dex.as_ptr(), invalid_dex.len()), 2);

            let required = stride_runtime_diagnostics_export(rt, std::ptr::null_mut(), 0);
            assert!(required > 1);
            let mut buffer = vec![0i8; required];
            assert_eq!(
                stride_runtime_diagnostics_export(rt, buffer.as_mut_ptr(), buffer.len()),
                required
            );
            let report = CStr::from_ptr(buffer.as_ptr()).to_string_lossy();
            assert!(report.contains("DEX load failed"), "{report}");
            assert!(report.contains("DEX class inspection failed: TruncatedHeader"), "{report}");
            stride_runtime_destroy(rt);
        }
    }

    #[test]
    fn manifest_orientation_reader_rejects_invalid_input() {
        let mut orientation = 99;
        assert_eq!(unsafe { stride_manifest_screen_orientation(std::ptr::null(), 0, &mut orientation) }, 1);
        assert_eq!(unsafe { stride_manifest_screen_orientation([0u8; 8].as_ptr(), 8, &mut orientation) }, 2);
        assert_eq!(orientation, 99);
    }

    #[test]
    fn manifest_orientation_reader_reads_real_fixture_declaration() {
        let bytes = include_bytes!("../../axml/tests/fixtures/pocket-ridge-manifest.axml");
        let mut orientation = 99;
        assert_eq!(unsafe { stride_manifest_screen_orientation(bytes.as_ptr(), bytes.len(), &mut orientation) }, 0);
        assert_eq!(orientation, 0); // Fixture declares Android landscape.
    }

    #[test]
    fn creates_and_destroys_runtime() {
        unsafe {
            let rt = stride_runtime_create(b"com.example.test\0".as_ptr() as *const c_char);
            assert!(!rt.is_null());
            assert_eq!(stride_runtime_dispatch_touch(rt, 0, 100.0, 200.0), 0);
            assert_eq!(stride_runtime_gl_viewport(rt, 0, 0, 800, 600), 0);
            assert_eq!(stride_runtime_gl_clear(rt, 0.1, 0.2, 0.3, 1.0), 0);
            let required = stride_runtime_diagnostics_export(rt, std::ptr::null_mut(), 0);
            assert!(required > 1);
            let mut buffer = vec![0i8; required];
            assert_eq!(stride_runtime_diagnostics_export(rt, buffer.as_mut_ptr(), buffer.len()), required);
            assert!(CStr::from_ptr(buffer.as_ptr()).to_string_lossy().contains("STRIDE SESSION DIAGNOSTICS"));
            stride_runtime_destroy(rt);
        }
    }
}
