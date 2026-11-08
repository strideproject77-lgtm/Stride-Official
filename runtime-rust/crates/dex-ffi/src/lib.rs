//! C ABI for passing DEX inventory summaries and ART runtime execution to Swift.

use std::ffi::CStr;
use std::os::raw::c_char;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::slice;

use stride_dex_runtime::{
    AndroidRuntime, LaunchReport, LaunchStatus, ActivityLifecycleState,
    MotionEvent, PointerCoords, ACTION_DOWN, ACTION_UP, ACTION_MOVE, ACTION_CANCEL,
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
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct StrideLaunchReport {
    /// 0: running, 1: finished normally, 2: uncaught exception, 3: activity not found, 4: manifest error
    pub status_code: i32,
    pub total_classes_loaded: u32,
    pub instructions_executed: u32,
    /// 0: not started, 1: created, 2: started, 3: resumed, 4: paused, 5: stopped, 6: destroyed
    pub lifecycle_state: i32,
    pub missing_apis_count: u32,
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
        LaunchStatus::Running => 0,
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
    0
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
    fn creates_and_destroys_runtime() {
        unsafe {
            let rt = stride_runtime_create(b"com.example.test\0".as_ptr() as *const c_char);
            assert!(!rt.is_null());
            assert_eq!(stride_runtime_dispatch_touch(rt, 0, 100.0, 200.0), 0);
            stride_runtime_destroy(rt);
        }
    }
}
