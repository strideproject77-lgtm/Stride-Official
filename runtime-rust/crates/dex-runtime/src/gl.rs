//! OpenGL ES and EGL Graphics translation and context management layer.
//!
//! Provides the EGL state machine, GLES2/GLES3 command stream translation,
//! and SurfaceView backbuffer swap presentation for Android 3D applications and games.

use std::collections::HashMap;
use crate::graphics::Surface;

pub const EGL_NO_CONTEXT: u32 = 0;
pub const EGL_NO_SURFACE: u32 = 0;
pub const EGL_SUCCESS: i32 = 0x3000;
pub const EGL_NOT_INITIALIZED: i32 = 0x3001;

pub const GL_COLOR_BUFFER_BIT: u32 = 0x00004000;
pub const GL_DEPTH_BUFFER_BIT: u32 = 0x00000100;
pub const GL_STENCIL_BUFFER_BIT: u32 = 0x00000400;

pub const GL_TRIANGLES: u32 = 0x0004;
pub const GL_TRIANGLE_STRIP: u32 = 0x0005;
pub const GL_TRIANGLE_FAN: u32 = 0x0006;
pub const GL_LINES: u32 = 0x0001;
pub const GL_POINTS: u32 = 0x0000;

/// An EGL Context representing an OpenGL ES rendering state.
#[derive(Debug, Clone)]
pub struct EglContext {
    pub context_id: u32,
    pub gles_version: u32, // 2 for GLES 2.0, 3 for GLES 3.0
    pub current_surface_id: Option<u32>,
}

/// An EGL window surface backed by a color buffer.
#[derive(Debug, Clone)]
pub struct EglWindowSurface {
    pub surface_id: u32,
    pub width: u32,
    pub height: u32,
    pub color_buffer: Vec<u32>, // 32-bit RGBA backbuffer
}

/// Active OpenGL ES 2.0 / 3.0 pipeline state.
#[derive(Debug, Clone)]
pub struct GlesState {
    pub viewport_x: i32,
    pub viewport_y: i32,
    pub viewport_width: i32,
    pub viewport_height: i32,
    pub clear_color: [f32; 4], // RGBA in 0.0..=1.0
    pub current_program: u32,
    pub active_texture: u32,
    pub depth_test_enabled: bool,
    pub cull_face_enabled: bool,
    pub blend_enabled: bool,
    pub total_draw_calls: usize,
}

impl Default for GlesState {
    fn default() -> Self {
        Self {
            viewport_x: 0,
            viewport_y: 0,
            viewport_width: 1080,
            viewport_height: 1920,
            clear_color: [0.0, 0.0, 0.0, 1.0],
            current_program: 0,
            active_texture: 0,
            depth_test_enabled: false,
            cull_face_enabled: false,
            blend_enabled: false,
            total_draw_calls: 0,
        }
    }
}

/// EGL and OpenGL ES translation coordinator for Android games and SurfaceViews.
pub struct OpenGlEsEngine {
    contexts: HashMap<u32, EglContext>,
    surfaces: HashMap<u32, EglWindowSurface>,
    next_id: u32,
    pub active_context_id: Option<u32>,
    pub active_surface_id: Option<u32>,
    pub gles_state: GlesState,
    pub swap_count: usize,
}

impl Default for OpenGlEsEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl OpenGlEsEngine {
    pub fn new() -> Self {
        Self {
            contexts: HashMap::new(),
            surfaces: HashMap::new(),
            next_id: 1,
            active_context_id: None,
            active_surface_id: None,
            gles_state: GlesState::default(),
            swap_count: 0,
        }
    }

    pub fn egl_create_context(&mut self, gles_version: u32) -> u32 {
        let cid = self.next_id;
        self.next_id += 1;
        self.contexts.insert(
            cid,
            EglContext {
                context_id: cid,
                gles_version,
                current_surface_id: None,
            },
        );
        cid
    }

    pub fn egl_create_window_surface(&mut self, width: u32, height: u32) -> u32 {
        let sid = self.next_id;
        self.next_id += 1;
        let size = (width as usize).saturating_mul(height as usize);
        self.surfaces.insert(
            sid,
            EglWindowSurface {
                surface_id: sid,
                width,
                height,
                color_buffer: vec![0xff000000; size],
            },
        );
        sid
    }

    pub fn egl_make_current(&mut self, context_id: u32, surface_id: u32) -> i32 {
        if !self.contexts.contains_key(&context_id) || !self.surfaces.contains_key(&surface_id) {
            return EGL_NOT_INITIALIZED;
        }

        if let Some(surf) = self.surfaces.get(&surface_id) {
            self.gles_state.viewport_width = surf.width as i32;
            self.gles_state.viewport_height = surf.height as i32;
        }

        self.active_context_id = Some(context_id);
        self.active_surface_id = Some(surface_id);
        EGL_SUCCESS
    }

    // --- GLES Commands ---

    pub fn gl_viewport(&mut self, x: i32, y: i32, width: i32, height: i32) {
        self.gles_state.viewport_x = x;
        self.gles_state.viewport_y = y;
        self.gles_state.viewport_width = width;
        self.gles_state.viewport_height = height;
    }

    pub fn gl_clear_color(&mut self, r: f32, g: f32, b: f32, a: f32) {
        self.gles_state.clear_color = [r.clamp(0.0, 1.0), g.clamp(0.0, 1.0), b.clamp(0.0, 1.0), a.clamp(0.0, 1.0)];
    }

    pub fn gl_clear(&mut self, mask: u32) {
        if mask & GL_COLOR_BUFFER_BIT != 0 {
            if let Some(sid) = self.active_surface_id {
                if let Some(surface) = self.surfaces.get_mut(&sid) {
                    let c = self.gles_state.clear_color;
                    let r = (c[0] * 255.0) as u32;
                    let g = (c[1] * 255.0) as u32;
                    let b = (c[2] * 255.0) as u32;
                    let a = (c[3] * 255.0) as u32;
                    let argb = (a << 24) | (r << 16) | (g << 8) | b;
                    surface.color_buffer.fill(argb);
                }
            }
        }
    }

    pub fn gl_draw_arrays(&mut self, _mode: u32, _first: i32, count: i32) {
        if count > 0 {
            self.gles_state.total_draw_calls += 1;
        }
    }

    /// Swaps the EGL backbuffer into the target Stride Surface framebuffer.
    pub fn egl_swap_buffers(&mut self, target_surface: &mut Surface) -> i32 {
        let sid = match self.active_surface_id {
            Some(s) => s,
            None => return EGL_NOT_INITIALIZED,
        };

        if let Some(egl_surf) = self.surfaces.get(&sid) {
            if target_surface.width != egl_surf.width || target_surface.height != egl_surf.height {
                target_surface.resize(egl_surf.width, egl_surf.height);
            }
            let len = target_surface.pixels.len().min(egl_surf.color_buffer.len());
            target_surface.pixels[..len].copy_from_slice(&egl_surf.color_buffer[..len]);
            self.swap_count += 1;
            EGL_SUCCESS
        } else {
            EGL_NOT_INITIALIZED
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn egl_context_and_surface_lifecycle() {
        let mut engine = OpenGlEsEngine::new();
        let cid = engine.egl_create_context(3);
        let sid = engine.egl_create_window_surface(640, 480);

        assert_eq!(engine.egl_make_current(cid, sid), EGL_SUCCESS);
        assert_eq!(engine.gles_state.viewport_width, 640);
        assert_eq!(engine.gles_state.viewport_height, 480);

        engine.gl_clear_color(0.2, 0.4, 0.8, 1.0);
        engine.gl_clear(GL_COLOR_BUFFER_BIT);

        let mut out_surface = Surface::new(640, 480);
        assert_eq!(engine.egl_swap_buffers(&mut out_surface), EGL_SUCCESS);
        assert_eq!(engine.swap_count, 1);

        // Check clear color in swapped buffer: (0.2*255=51, 0.4*255=102, 0.8*255=204, 255)
        let pixel = out_surface.get_pixel(100, 100).unwrap();
        assert_eq!((pixel >> 24) & 0xff, 255); // Alpha
        assert_eq!((pixel >> 16) & 0xff, 51);  // Red
        assert_eq!((pixel >> 8) & 0xff, 102);  // Green
        assert_eq!(pixel & 0xff, 204);         // Blue
    }
}
