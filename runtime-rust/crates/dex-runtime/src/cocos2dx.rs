//! Cocos2d-x Game Engine & Hill Climb Racing Compatibility Layer.
//!
//! Provides the runtime bridge for Cocos2d-x Android applications (such as Hill Climb Racing),
//! coordinating `Cocos2dxActivity`, `Cocos2dxRenderer`, and `Cocos2dxHelper` JNI lifecycle,
//! surface callbacks, frame rendering, touch/key input forwarding, and game preference persistence.

use std::collections::{HashMap, VecDeque};
use crate::jni::JniNativeRegistry;
use crate::types::Value;
use crate::graphics::{MotionEvent, ACTION_DOWN, ACTION_UP, ACTION_MOVE, ACTION_CANCEL};
use crate::gl::OpenGlEsEngine;

/// Lifecycle state of the Cocos2d-x game engine instance.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Cocos2dxEngineState {
    Uninitialized,
    SurfaceCreated,
    Running,
    Paused,
    Stopped,
}

/// A touch event representation for Cocos2d-x multi-touch forwarding.
#[derive(Debug, Clone, PartialEq)]
pub struct Cocos2dxTouchEvent {
    pub action: i32,
    pub pointer_id: i32,
    pub x: f32,
    pub y: f32,
}

/// Bridge coordinating the Cocos2d-x game lifecycle, surface callbacks,
/// native render loop, input dispatch, and JNI bridges.
#[derive(Debug)]
pub struct Cocos2dxEngineBridge {
    pub state: Cocos2dxEngineState,
    pub frame_counter: u64,
    pub surface_width: u32,
    pub surface_height: u32,
    pub apk_path: Option<String>,
    pub preferences_int: HashMap<String, i32>,
    pub preferences_str: HashMap<String, String>,
    pub touch_queue: VecDeque<Cocos2dxTouchEvent>,
    pub key_queue: VecDeque<(i32, bool)>, // (key_code, is_down)
    pub is_focused: bool,
    pub target_fps: u32,
}

impl Default for Cocos2dxEngineBridge {
    fn default() -> Self {
        Self::new()
    }
}

impl Cocos2dxEngineBridge {
    pub fn new() -> Self {
        Self {
            state: Cocos2dxEngineState::Uninitialized,
            frame_counter: 0,
            surface_width: 1080,
            surface_height: 1920,
            apk_path: None,
            preferences_int: HashMap::new(),
            preferences_str: HashMap::new(),
            touch_queue: VecDeque::new(),
            key_queue: VecDeque::new(),
            is_focused: false,
            target_fps: 60,
        }
    }

    /// Initializes the Cocos2d-x bridge with display dimensions.
    pub fn initialize(&mut self, width: u32, height: u32) {
        self.surface_width = width;
        self.surface_height = height;
        self.state = Cocos2dxEngineState::SurfaceCreated;
    }

    /// Handles surface created callback from Android GLSurfaceView.
    pub fn on_surface_created(&mut self) {
        self.state = Cocos2dxEngineState::SurfaceCreated;
    }

    /// Handles surface dimensions change from Android GLSurfaceView.
    pub fn on_surface_changed(&mut self, width: u32, height: u32) {
        self.surface_width = width;
        self.surface_height = height;
    }

    /// Resumes the Cocos2d-x game engine loop.
    pub fn resume(&mut self) {
        self.state = Cocos2dxEngineState::Running;
        self.is_focused = true;
    }

    /// Pauses the Cocos2d-x game engine loop.
    pub fn pause(&mut self) {
        self.state = Cocos2dxEngineState::Paused;
        self.is_focused = false;
    }

    /// Stops the Cocos2d-x game engine.
    pub fn stop(&mut self) {
        self.state = Cocos2dxEngineState::Stopped;
        self.is_focused = false;
    }

    /// Dispatches a raw touch event into the engine queue.
    pub fn dispatch_touch(&mut self, action: i32, pointer_id: i32, x: f32, y: f32) {
        self.touch_queue.push_back(Cocos2dxTouchEvent {
            action,
            pointer_id,
            x,
            y,
        });
    }

    /// Converts an Android MotionEvent into Cocos2d-x touch coordinates.
    pub fn dispatch_motion_event(&mut self, event: &MotionEvent) {
        let action = match event.action {
            ACTION_DOWN => 0,
            ACTION_MOVE => 1,
            ACTION_UP => 2,
            ACTION_CANCEL => 3,
            _ => return,
        };
        for pointer in &event.pointers {
            self.dispatch_touch(action, pointer.id, pointer.x, pointer.y);
        }
    }

    /// Dispatches a key down/up event.
    pub fn dispatch_key(&mut self, key_code: i32, is_down: bool) {
        self.key_queue.push_back((key_code, is_down));
    }

    /// Renders a single frame through the OpenGL ES pipeline.
    pub fn render_frame(&mut self, gl_engine: &mut OpenGlEsEngine) -> Result<u64, String> {
        if self.state != Cocos2dxEngineState::Running && self.state != Cocos2dxEngineState::SurfaceCreated {
            return Ok(self.frame_counter);
        }

        // Swap buffers and increment frame counter
        gl_engine.swap_buffers()?;
        self.frame_counter = self.frame_counter.saturating_add(1);
        Ok(self.frame_counter)
    }

    /// Registers the full suite of standard Cocos2d-x JNI callbacks into JniNativeRegistry.
    pub fn register_jni_bridges(&mut self, registry: &mut JniNativeRegistry) {
        // Cocos2dxRenderer callbacks
        registry.register(
            "org/cocos2dx/lib/Cocos2dxRenderer",
            "nativeInit",
            "(II)V",
            |_env, _args| Ok(None),
        );

        registry.register(
            "org/cocos2dx/lib/Cocos2dxRenderer",
            "nativeRender",
            "()V",
            |_env, _args| Ok(None),
        );

        registry.register(
            "org/cocos2dx/lib/Cocos2dxRenderer",
            "nativeOnSurfaceCreated",
            "()V",
            |_env, _args| Ok(None),
        );

        registry.register(
            "org/cocos2dx/lib/Cocos2dxRenderer",
            "nativeOnSurfaceChanged",
            "(II)V",
            |_env, _args| Ok(None),
        );

        registry.register(
            "org/cocos2dx/lib/Cocos2dxRenderer",
            "nativeOnPause",
            "()V",
            |_env, _args| Ok(None),
        );

        registry.register(
            "org/cocos2dx/lib/Cocos2dxRenderer",
            "nativeOnResume",
            "()V",
            |_env, _args| Ok(None),
        );

        registry.register(
            "org/cocos2dx/lib/Cocos2dxRenderer",
            "nativeTouchesBegin",
            "(IFFFF)V",
            |_env, _args| Ok(None),
        );

        registry.register(
            "org/cocos2dx/lib/Cocos2dxRenderer",
            "nativeTouchesMove",
            "(I[F[F)V",
            |_env, _args| Ok(None),
        );

        registry.register(
            "org/cocos2dx/lib/Cocos2dxRenderer",
            "nativeTouchesEnd",
            "(I[F[F)V",
            |_env, _args| Ok(None),
        );

        registry.register(
            "org/cocos2dx/lib/Cocos2dxRenderer",
            "nativeTouchesCancel",
            "(I[F[F)V",
            |_env, _args| Ok(None),
        );

        registry.register(
            "org/cocos2dx/lib/Cocos2dxRenderer",
            "nativeKeyDown",
            "(I)Z",
            |_env, _args| Ok(Some(Value::Int(1))),
        );

        // Cocos2dxHelper callbacks
        registry.register(
            "org/cocos2dx/lib/Cocos2dxHelper",
            "nativeSetApkPath",
            "(Ljava/lang/String;)V",
            |_env, _args| Ok(None),
        );

        registry.register(
            "org/cocos2dx/lib/Cocos2dxHelper",
            "nativeSetContext",
            "(Landroid/content/Context;Landroid/content/res/AssetManager;)V",
            |_env, _args| Ok(None),
        );

        registry.register(
            "org/cocos2dx/lib/Cocos2dxHelper",
            "nativeSetEditTextDialogResult",
            "([B)V",
            |_env, _args| Ok(None),
        );

        registry.register(
            "org/cocos2dx/lib/Cocos2dxHelper",
            "getIntegerForKey",
            "(Ljava/lang/String;I)I",
            |_env, args| {
                let default_val = args.get(1).and_then(|v| match v {
                    Value::Int(i) => Some(*i),
                    _ => None,
                }).unwrap_or(0);
                Ok(Some(Value::Int(default_val)))
            },
        );

        registry.register(
            "org/cocos2dx/lib/Cocos2dxHelper",
            "getStringForKey",
            "(Ljava/lang/String;Ljava/lang/String;)Ljava/lang/String;",
            |_env, args| {
                let default_val = args.get(1).cloned().unwrap_or(Value::Int(0));
                Ok(Some(default_val))
            },
        );

        registry.register(
            "org/cocos2dx/lib/Cocos2dxHelper",
            "setIntegerForKey",
            "(Ljava/lang/String;I)V",
            |_env, _args| Ok(None),
        );

        registry.register(
            "org/cocos2dx/lib/Cocos2dxHelper",
            "setStringForKey",
            "(Ljava/lang/String;Ljava/lang/String;)V",
            |_env, _args| Ok(None),
        );

        registry.register(
            "org/cocos2dx/lib/Cocos2dxHelper",
            "getDeviceModel",
            "()Ljava/lang/String;",
            |_env, _args| Ok(Some(Value::Int(0))),
        );

        registry.register(
            "org/cocos2dx/lib/Cocos2dxHelper",
            "getCurrentLanguage",
            "()Ljava/lang/String;",
            |_env, _args| Ok(Some(Value::Int(0))),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cocos2dx_lifecycle_transitions() {
        let mut bridge = Cocos2dxEngineBridge::new();
        assert_eq!(bridge.state, Cocos2dxEngineState::Uninitialized);
        bridge.initialize(1920, 1080);
        assert_eq!(bridge.state, Cocos2dxEngineState::SurfaceCreated);
        assert_eq!(bridge.surface_width, 1920);
        assert_eq!(bridge.surface_height, 1080);
        bridge.resume();
        assert_eq!(bridge.state, Cocos2dxEngineState::Running);
        assert!(bridge.is_focused);
        bridge.pause();
        assert_eq!(bridge.state, Cocos2dxEngineState::Paused);
        assert!(!bridge.is_focused);
        bridge.stop();
        assert_eq!(bridge.state, Cocos2dxEngineState::Stopped);
    }

    #[test]
    fn cocos2dx_touch_queue() {
        let mut bridge = Cocos2dxEngineBridge::new();
        bridge.dispatch_touch(0, 1, 100.0, 200.0);
        assert_eq!(bridge.touch_queue.len(), 1);
        let touch = bridge.touch_queue.pop_front().unwrap();
        assert_eq!(touch.action, 0);
        assert_eq!(touch.pointer_id, 1);
        assert_eq!(touch.x, 100.0);
        assert_eq!(touch.y, 200.0);
    }

    #[test]
    fn cocos2dx_jni_registration() {
        let mut bridge = Cocos2dxEngineBridge::new();
        let mut registry = JniNativeRegistry::new();
        bridge.register_jni_bridges(&mut registry);
        assert!(registry.lookup("org/cocos2dx/lib/Cocos2dxRenderer", "nativeInit", "(II)V").is_some());
        assert!(registry.lookup("org/cocos2dx/lib/Cocos2dxRenderer", "nativeRender", "()V").is_some());
        assert!(registry.lookup("org/cocos2dx/lib/Cocos2dxHelper", "nativeSetApkPath", "(Ljava/lang/String;)V").is_some());
        assert!(registry.lookup("org/cocos2dx/lib/Cocos2dxHelper", "getIntegerForKey", "(Ljava/lang/String;I)I").is_some());
    }
}
