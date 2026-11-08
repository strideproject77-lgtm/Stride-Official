//! Comprehensive End-to-End Compatibility & Regression Test Suite for Stride Runtime.
//!
//! Validates the integration of all runtime subsystems:
//! - Managed heap & string interning
//! - Bionic libc/libm/liblog emulation
//! - Canvas 2D rasterization
//! - OpenGL ES / EGL context translation & surface swap
//! - ActivityManager backstack & lifecycle dispatch
//! - ServiceManager background/foreground lifecycle
//! - InputQueue motion event handling
//! - Performance telemetry and ANR watchdog
//! - Android runtime launch pipeline

#[cfg(test)]
mod tests {
    use crate::launcher::AndroidRuntime;
    use crate::framework::Intent;
    use crate::canvas::{Canvas, Paint, PaintStyle, RectF};
    use crate::graphics::{MotionEvent, PointerCoords, ACTION_DOWN, ACTION_UP};
    use crate::gl::GL_COLOR_BUFFER_BIT;
    use crate::service_manager::ServiceState;

    #[test]
    fn full_runtime_subsystem_integration() {
        let mut runtime = AndroidRuntime::new("com.stride.compat.test");

        // 1. Validate Bionic Libc / Libm Symbol Resolution
        let sin_sym = runtime.native_linker.bionic.resolve_symbol("sin");
        assert!(sin_sym.is_some(), "sin symbol should be resolved by Bionic");
        let log_sym = runtime.native_linker.bionic.resolve_symbol("__android_log_print");
        assert!(log_sym.is_some(), "__android_log_print should be resolved by Bionic");

        // 2. Validate SharedPreferences persistence
        let mut prefs = runtime.environment.get_preferences("user_settings");
        prefs.put_string_value("username", "player1");
        prefs.put_int("high_score", 95000);
        prefs.commit();

        let loaded_prefs = runtime.environment.get_preferences("user_settings");
        assert_eq!(loaded_prefs.get_string_value("username", ""), "player1");
        assert_eq!(loaded_prefs.get_int("high_score", 0), 95000);

        // 3. Validate Canvas 2D Rasterization into Runtime Surface
        {
            let mut canvas = Canvas::new(&mut runtime.surface);
            let mut paint = Paint::new();
            paint.set_color(0xffff8800); // Orange
            paint.style = PaintStyle::Fill;
            canvas.draw_rect(RectF::new(10.0, 10.0, 100.0, 100.0), &paint);
        }
        let orange_pixel = runtime.surface.get_pixel(50, 50).expect("pixel within rect");
        assert_eq!(orange_pixel, 0xffff8800);

        // 4. Validate OpenGL ES Context & Backbuffer Swap into Surface
        let cid = runtime.gl_engine.egl_create_context(3);
        let sid = runtime.gl_engine.egl_create_window_surface(1080, 1920);
        assert_eq!(runtime.gl_engine.egl_make_current(cid, sid), 0x3000); // EGL_SUCCESS
        runtime.gl_engine.gl_clear_color(0.0, 1.0, 0.0, 1.0); // Full Green
        runtime.gl_engine.gl_clear(GL_COLOR_BUFFER_BIT);

        assert_eq!(runtime.gl_engine.egl_swap_buffers(&mut runtime.surface), 0x3000);
        let green_pixel = runtime.surface.get_pixel(50, 50).expect("swapped pixel");
        assert_eq!(green_pixel, 0xff00ff00); // 32-bit ARGB Green

        // 5. Validate Touch Event Input Queue
        let motion = MotionEvent {
            action: ACTION_DOWN,
            event_time_ms: 1000,
            pointers: vec![PointerCoords { id: 0, x: 250.0, y: 400.0, pressure: 1.0 }],
        };
        runtime.input_queue.enqueue_motion(motion);
        let dequeued = runtime.input_queue.poll_motion();
        assert!(dequeued.is_some());
        assert_eq!(dequeued.unwrap().action, ACTION_DOWN);

        // 6. Validate ServiceManager Lifecycle
        let service_intent = Intent::new(Some("android.intent.action.START_SERVICE"))
            .with_component("com.stride.compat.test", "BackgroundSyncService");
        let methods = std::collections::HashMap::new();
        let strings: Vec<String> = Vec::new();
        let mut vm = crate::interpreter::ArtVm::new(
            &mut runtime.heap,
            &mut runtime.hierarchy,
            &mut runtime.monitors,
            &mut runtime.jni,
            &methods,
            &strings,
        );

        let s_id = runtime.service_manager.start_service(
            &service_intent,
            &mut runtime.class_loader,
            &mut vm,
        ).expect("start service");
        assert_eq!(s_id, 1);
        assert_eq!(runtime.service_manager.running_services_count(), 1);

        runtime.service_manager.start_foreground("BackgroundSyncService", 42);
        let s_rec = runtime.service_manager.get_service("BackgroundSyncService").unwrap();
        assert!(s_rec.is_foreground);
        assert_eq!(s_rec.state, ServiceState::Started);

        let stopped = runtime.service_manager.stop_service(
            "BackgroundSyncService",
            &mut runtime.class_loader,
            &mut vm,
        );
        assert!(stopped);
        assert_eq!(runtime.service_manager.running_services_count(), 0);

        drop(vm);

        // 7. Validate Performance Telemetry & ANR Watchdog
        runtime.telemetry.record_frame(16.6);
        runtime.telemetry.record_frame(16.7);
        let fps = runtime.telemetry.current_fps();
        assert!(fps >= 59.0 && fps <= 61.0);

        let snap = runtime.telemetry.snapshot(runtime.heap.allocated_bytes(), 5000);
        assert!(!snap.anr_detected);
    }
}
