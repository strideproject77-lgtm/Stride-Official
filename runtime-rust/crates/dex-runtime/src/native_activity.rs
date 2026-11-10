//! NativeActivity launch pipeline for Unity and C++ Android applications.
//!
//! Implements the Android NativeActivity lifecycle, surfacing the C/C++ application
//! entry point (ANativeActivity_onCreate) and coordinating with the Unity player bridge
//! and OpenGL ES rendering engine.

use crate::unity::UnityPlayerBridge;

/// EGL display configuration for the guest application surface.
#[derive(Debug, Clone)]
pub struct EglConfig {
    pub red_size: u8,
    pub green_size: u8,
    pub blue_size: u8,
    pub alpha_size: u8,
    pub depth_size: u8,
    pub stencil_size: u8,
    pub samples: u8,
    pub surface_width: u32,
    pub surface_height: u32,
    pub gles_version: u8,
}

impl Default for EglConfig {
    fn default() -> Self {
        Self {
            red_size: 8,
            green_size: 8,
            blue_size: 8,
            alpha_size: 8,
            depth_size: 24,
            stencil_size: 8,
            samples: 4,
            surface_width: 1080,
            surface_height: 1920,
            gles_version: 3,
        }
    }
}

/// Status returned by the NativeActivity launch pipeline.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NativeActivityStatus {
    /// Application initialized and running.
    Running,
    /// Application paused (background).
    Paused,
    /// Launch failed — missing required library.
    MissingLibrary(String),
    /// Launch failed — missing or malformed entry point.
    MissingEntryPoint(String),
    /// Runtime error during launch.
    RuntimeError(String),
}

/// Result of launching a NativeActivity.
#[derive(Debug, Clone)]
pub struct NativeActivityLaunchResult {
    pub status: NativeActivityStatus,
    pub egl_config: EglConfig,
    pub activity_class: String,
    pub native_library: Option<String>,
    pub frames_rendered: u64,
}

/// Launch coordinator for NativeActivity and Unity-based Android applications.
#[derive(Debug, Default)]
pub struct NativeActivityLauncher {
    pub egl_config: EglConfig,
    pub status: Option<NativeActivityStatus>,
    pub activity_class: String,
    pub native_library_name: Option<String>,
    pub frames_rendered: u64,
}

impl NativeActivityLauncher {
    pub fn new() -> Self {
        Self {
            egl_config: EglConfig::default(),
            status: None,
            activity_class: String::new(),
            native_library_name: None,
            frames_rendered: 0,
        }
    }

    /// Configures the EGL surface size from the guest display metrics.
    pub fn configure_surface(&mut self, width: u32, height: u32, gles_version: u8) {
        self.egl_config.surface_width = width;
        self.egl_config.surface_height = height;
        self.egl_config.gles_version = gles_version;
    }

    /// Sets the activity class name and native library to load.
    pub fn configure_activity(&mut self, activity_class: &str, native_library: Option<&str>) {
        self.activity_class = activity_class.to_string();
        self.native_library_name = native_library.map(|s| s.to_string());
    }

    /// Refuses to report a Unity launch until a native linker, JNI_OnLoad,
    /// Unity initialization and a real render callback are connected.
    ///
    /// A state transition in UnityPlayerBridge is not evidence that libunity.so
    /// or libil2cpp.so has been loaded or that the game is executing.
    pub fn launch_unity(
        &mut self,
        _unity_bridge: &mut UnityPlayerBridge,
    ) -> NativeActivityLaunchResult {
        let lib_name = self.native_library_name.clone()
            .unwrap_or_else(|| "libunity.so".to_string());
        let status = NativeActivityStatus::RuntimeError(format!(
            "Unity launch is not executable: {} has not been loaded and invoked by the native linker; JNI_OnLoad, Unity initialization and guest rendering are not connected",
            lib_name
        ));

        self.status = Some(status.clone());
        self.frames_rendered = 0;

        NativeActivityLaunchResult {
            status,
            egl_config: self.egl_config.clone(),
            activity_class: self.activity_class.clone(),
            native_library: Some(lib_name),
            frames_rendered: 0,
        }
    }

    /// Refuses to report a generic NativeActivity launch based only on a symbol name.
    /// The entry point must be resolved from a loaded ELF image and actually invoked.
    pub fn launch_native_cpp(&mut self, entry_point: &str) -> NativeActivityLaunchResult {
        let status = if entry_point.is_empty() {
            NativeActivityStatus::MissingEntryPoint(
                "ANativeActivity_onCreate was not resolved from a loaded native library".to_string()
            )
        } else {
            NativeActivityStatus::RuntimeError(format!(
                "NativeActivity launch is not executable: entry point '{}' was named but no loaded ELF function was invoked",
                entry_point
            ))
        };

        self.status = Some(status.clone());
        self.frames_rendered = 0;

        NativeActivityLaunchResult {
            status,
            egl_config: self.egl_config.clone(),
            activity_class: self.activity_class.clone(),
            native_library: self.native_library_name.clone(),
            frames_rendered: 0,
        }
    }

    /// Records a rendered frame.
    pub fn record_frame(&mut self) {
        self.frames_rendered += 1;
    }

    /// Returns true if the activity is in Running state.
    pub fn is_running(&self) -> bool {
        matches!(self.status, Some(NativeActivityStatus::Running))
    }

    pub fn diagnostic_summary(&self) -> String {
        format!(
            "NativeActivity: class={}, lib={}, frames={}, status={:?}",
            self.activity_class,
            self.native_library_name.as_deref().unwrap_or("none"),
            self.frames_rendered,
            self.status,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unity_launch_fails_closed_until_native_execution_is_connected() {
        let mut launcher = NativeActivityLauncher::new();
        launcher.configure_activity(
            "com.unity3d.player.UnityPlayerActivity",
            Some("libunity.so"),
        );
        launcher.configure_surface(1080, 1920, 3);
        let mut bridge = UnityPlayerBridge::new();
        let result = launcher.launch_unity(&mut bridge);
        assert!(matches!(result.status, NativeActivityStatus::RuntimeError(_)));
        assert_eq!(bridge.state, crate::unity::UnityEngineState::Uninitialized);
        assert!(!launcher.is_running());
        assert_eq!(result.frames_rendered, 0);
    }

    #[test]
    fn native_cpp_launch_missing_entry_point() {
        let mut launcher = NativeActivityLauncher::new();
        launcher.configure_activity("com.example.NativeActivity", Some("libnative.so"));
        let result = launcher.launch_native_cpp("");
        assert!(matches!(result.status, NativeActivityStatus::MissingEntryPoint(_)));
        assert!(!launcher.is_running());
    }

    #[test]
    fn native_cpp_launch_does_not_treat_symbol_name_as_execution() {
        let mut launcher = NativeActivityLauncher::new();
        launcher.configure_activity("com.example.NativeActivity", Some("libnative.so"));
        let result = launcher.launch_native_cpp("ANativeActivity_onCreate");
        assert!(matches!(result.status, NativeActivityStatus::RuntimeError(_)));
        assert!(!launcher.is_running());
        assert_eq!(result.frames_rendered, 0);
    }

    #[test]
    fn egl_config_defaults_are_sensible() {
        let cfg = EglConfig::default();
        assert_eq!(cfg.gles_version, 3);
        assert_eq!(cfg.surface_width, 1080);
        assert_eq!(cfg.surface_height, 1920);
        assert_eq!(cfg.depth_size, 24);
        assert_eq!(cfg.alpha_size, 8);
    }

    #[test]
    fn frame_recording_increments_counter() {
        let mut launcher = NativeActivityLauncher::new();
        assert_eq!(launcher.frames_rendered, 0);
        launcher.record_frame();
        launcher.record_frame();
        assert_eq!(launcher.frames_rendered, 2);
    }
}
