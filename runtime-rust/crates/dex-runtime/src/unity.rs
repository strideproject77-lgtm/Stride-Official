//! Unity Engine & Hill Climb Racing Game Compatibility Layer.
//!
//! Provides the runtime bridge for Unity-based Android applications (such as Hill Climb Racing)
//! implementing the `UnityPlayer` lifecycle, JNI callback interface, game loop tick coordinator,
//! and native touch event forwarding.

use std::collections::VecDeque;
use crate::jni::JniNativeRegistry;
use crate::types::Value;
use crate::graphics::{MotionEvent, Surface, ACTION_DOWN, ACTION_UP, ACTION_MOVE};
use crate::gl::OpenGlEsEngine;

/// State of the Unity game engine instance.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnityEngineState {
    Uninitialized,
    Initialized,
    Running,
    Paused,
    Stopped,
}

/// A message queued via `UnityPlayer.UnitySendMessage(gameObject, method, message)`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnityMessage {
    pub game_object: String,
    pub method_name: String,
    pub message_payload: String,
}

/// Unity Player Bridge coordinating game loop rendering, touch events, and JNI bindings.
#[derive(Debug)]
pub struct UnityPlayerBridge {
    pub state: UnityEngineState,
    pub frame_counter: u64,
    pub message_queue: VecDeque<UnityMessage>,
    pub is_focused: bool,
    pub target_fps: u32,
}

impl Default for UnityPlayerBridge {
    fn default() -> Self {
        Self::new()
    }
}

impl UnityPlayerBridge {
    pub fn new() -> Self {
        Self {
            state: UnityEngineState::Uninitialized,
            frame_counter: 0,
            message_queue: VecDeque::new(),
            is_focused: false,
            target_fps: 60,
        }
    }

    /// Initializes the Unity engine state.
    pub fn initialize(&mut self) {
        self.state = UnityEngineState::Initialized;
    }

    /// Resumes the game loop.
    pub fn resume(&mut self) {
        self.state = UnityEngineState::Running;
        self.is_focused = true;
    }

    /// Pauses the game loop.
    pub fn pause(&mut self) {
        self.state = UnityEngineState::Paused;
        self.is_focused = false;
    }

    /// Queues a message to a Unity GameObject.
    pub fn send_message(&mut self, game_object: &str, method: &str, payload: &str) {
        self.message_queue.push_back(UnityMessage {
            game_object: game_object.to_string(),
            method_name: method.to_string(),
            message_payload: payload.to_string(),
        });
    }

    /// Pulls the next message destined for Unity scripts.
    pub fn poll_message(&mut self) -> Option<UnityMessage> {
        self.message_queue.pop_front()
    }

    /// Dispatches a single game loop frame render tick.
    /// Returns true if rendering succeeded and a frame was produced.
    pub fn step_frame(&mut self, gl_engine: &mut OpenGlEsEngine, target_surface: &mut Surface) -> bool {
        if self.state != UnityEngineState::Running {
            return false;
        }

        self.frame_counter += 1;
        // Trigger EGL swap buffers to present the rendered backbuffer to the Stride surface
        gl_engine.egl_swap_buffers(target_surface);
        true
    }

    /// Translates an Android MotionEvent into Unity touch inputs.
    pub fn dispatch_touch(&mut self, motion: &MotionEvent) -> bool {
        if self.state != UnityEngineState::Running {
            return false;
        }

        for ptr in &motion.pointers {
            let action_str = match motion.action {
                ACTION_DOWN => "Began",
                ACTION_MOVE => "Moved",
                ACTION_UP => "Ended",
                _ => "Canceled",
            };

            // Notify Unity touch subsystem
            let payload = format!("{}:{}:{}:{}", ptr.id, action_str, ptr.x, ptr.y);
            self.send_message("InputManager", "OnAndroidTouchEvent", &payload);
        }

        true
    }

    /// Registers all standard `com.unity3d.player.UnityPlayer` native methods into the JNI registry.
    pub fn register_unity_jni(registry: &mut JniNativeRegistry) {
        let ok_unit = |_env: &mut crate::jni::JniEnv, _args: &[Value]| Ok(None);
        let ok_bool = |_env: &mut crate::jni::JniEnv, _args: &[Value]| Ok(Some(Value::Boolean(true)));

        // nativeInit(II)V
        registry.register("Lcom/unity3d/player/UnityPlayer;", "nativeInit", "(II)V", ok_unit);

        // nativeResume()V
        registry.register("Lcom/unity3d/player/UnityPlayer;", "nativeResume", "()V", ok_unit);

        // nativePause()V
        registry.register("Lcom/unity3d/player/UnityPlayer;", "nativePause", "()V", ok_unit);

        // nativeFocusChanged(Z)V
        registry.register("Lcom/unity3d/player/UnityPlayer;", "nativeFocusChanged", "(Z)V", ok_unit);

        // nativeRecreateGfxState()V
        registry.register("Lcom/unity3d/player/UnityPlayer;", "nativeRecreateGfxState", "()V", ok_unit);

        // nativeRender()Z
        registry.register("Lcom/unity3d/player/UnityPlayer;", "nativeRender", "()Z", ok_bool);

        // nativeDone()V
        registry.register("Lcom/unity3d/player/UnityPlayer;", "nativeDone", "()V", ok_unit);

        // UnitySendMessage(Ljava/lang/String;Ljava/lang/String;Ljava/lang/String;)V
        registry.register("Lcom/unity3d/player/UnityPlayer;", "UnitySendMessage", "(Ljava/lang/String;Ljava/lang/String;Ljava/lang/String;)V", ok_unit);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unity_player_lifecycle_and_messaging() {
        let mut bridge = UnityPlayerBridge::new();
        assert_eq!(bridge.state, UnityEngineState::Uninitialized);

        bridge.initialize();
        assert_eq!(bridge.state, UnityEngineState::Initialized);

        bridge.resume();
        assert_eq!(bridge.state, UnityEngineState::Running);
        assert!(bridge.is_focused);

        bridge.send_message("GameController", "StartRace", "track=desert");
        let msg = bridge.poll_message().expect("should have queued message");
        assert_eq!(msg.game_object, "GameController");
        assert_eq!(msg.method_name, "StartRace");
        assert_eq!(msg.message_payload, "track=desert");

        bridge.pause();
        assert_eq!(bridge.state, UnityEngineState::Paused);
        assert!(!bridge.is_focused);
    }

    #[test]
    fn unity_touch_dispatch() {
        let mut bridge = UnityPlayerBridge::new();
        bridge.initialize();
        bridge.resume();

        let motion = MotionEvent {
            action: ACTION_DOWN,
            event_time_ms: 100,
            pointers: vec![crate::graphics::PointerCoords {
                id: 0,
                x: 300.0,
                y: 500.0,
                pressure: 1.0,
            }],
        };

        assert!(bridge.dispatch_touch(&motion));
        let touch_msg = bridge.poll_message().expect("touch event sent to InputManager");
        assert_eq!(touch_msg.game_object, "InputManager");
        assert_eq!(touch_msg.method_name, "OnAndroidTouchEvent");
        assert_eq!(touch_msg.message_payload, "0:Began:300:500");
    }
}
