//! Android Graphics, Input Events, and Audio subsystem architectures.

use std::collections::VecDeque;
use std::sync::Mutex;

/// Screen display metrics matching Android android.util.DisplayMetrics.
#[derive(Debug, Clone, PartialEq)]
pub struct DisplayMetrics {
    pub width_pixels: u32,
    pub height_pixels: u32,
    pub density: f32,
    pub density_dpi: u32,
    pub scaled_density: f32,
    pub xdpi: f32,
    pub ydpi: f32,
}

impl Default for DisplayMetrics {
    fn default() -> Self {
        Self {
            width_pixels: 1080,
            height_pixels: 1920,
            density: 3.0,
            density_dpi: 480,
            scaled_density: 3.0,
            xdpi: 480.0,
            ydpi: 480.0,
        }
    }
}

/// A 32-bit RGBA surface framebuffer for Android window drawing.
#[derive(Debug, Clone)]
pub struct Surface {
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<u32>,
}

impl Surface {
    pub fn new(width: u32, height: u32) -> Self {
        let size = (width as usize).saturating_mul(height as usize);
        Self {
            width,
            height,
            pixels: vec![0xff000000; size], // opaque black
        }
    }

    pub fn clear(&mut self, color: u32) {
        self.pixels.fill(color);
    }

    pub fn set_pixel(&mut self, x: u32, y: u32, color: u32) {
        if x < self.width && y < self.height {
            let idx = (y as usize) * (self.width as usize) + (x as usize);
            if idx < self.pixels.len() {
                self.pixels[idx] = color;
            }
        }
    }

    pub fn get_pixel(&self, x: u32, y: u32) -> Option<u32> {
        if x < self.width && y < self.height {
            let idx = (y as usize) * (self.width as usize) + (x as usize);
            self.pixels.get(idx).copied()
        } else {
            None
        }
    }
}

/// Motion event action codes matching android.view.MotionEvent.
pub const ACTION_DOWN: i32 = 0;
pub const ACTION_UP: i32 = 1;
pub const ACTION_MOVE: i32 = 2;
pub const ACTION_CANCEL: i32 = 3;

/// Pointer position and attributes in a motion event.
#[derive(Debug, Clone, PartialEq)]
pub struct PointerCoords {
    pub id: i32,
    pub x: f32,
    pub y: f32,
    pub pressure: f32,
}

/// Touch / motion event matching Android MotionEvent.
#[derive(Debug, Clone, PartialEq)]
pub struct MotionEvent {
    pub action: i32,
    pub event_time_ms: u64,
    pub pointers: Vec<PointerCoords>,
}

/// Key event action codes matching android.view.KeyEvent.
pub const KEY_ACTION_DOWN: i32 = 0;
pub const KEY_ACTION_UP: i32 = 1;

pub const KEYCODE_BACK: i32 = 4;
pub const KEYCODE_HOME: i32 = 3;
pub const KEYCODE_MENU: i32 = 82;
pub const KEYCODE_VOLUME_UP: i32 = 24;
pub const KEYCODE_VOLUME_DOWN: i32 = 25;

/// Hardware or software keyboard event matching Android KeyEvent.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeyEvent {
    pub action: i32,
    pub key_code: i32,
    pub repeat_count: i32,
    pub event_time_ms: u64,
}

/// Input event queue managing incoming touch and key events for an active Activity.
#[derive(Debug, Default)]
pub struct InputQueue {
    pub motion_events: Mutex<VecDeque<MotionEvent>>,
    pub key_events: Mutex<VecDeque<KeyEvent>>,
}

impl InputQueue {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn enqueue_motion(&self, event: MotionEvent) {
        if let Ok(mut q) = self.motion_events.lock() {
            q.push_back(event);
        }
    }

    pub fn poll_motion(&self) -> Option<MotionEvent> {
        if let Ok(mut q) = self.motion_events.lock() {
            q.pop_front()
        } else {
            None
        }
    }

    pub fn enqueue_key(&self, event: KeyEvent) {
        if let Ok(mut q) = self.key_events.lock() {
            q.push_back(event);
        }
    }

    pub fn poll_key(&self) -> Option<KeyEvent> {
        if let Ok(mut q) = self.key_events.lock() {
            q.pop_front()
        } else {
            None
        }
    }
}

/// Audio playback state matching android.media.AudioTrack.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AudioTrackState {
    Uninitialized,
    Stopped,
    Playing,
    Paused,
}

/// Audio playback buffer matching Android AudioTrack.
#[derive(Debug, Clone)]
pub struct AudioTrack {
    pub sample_rate_hz: u32,
    pub channel_count: u32,
    pub state: AudioTrackState,
    pub buffer: Vec<i16>,
}

impl AudioTrack {
    pub fn new(sample_rate_hz: u32, channel_count: u32) -> Self {
        Self {
            sample_rate_hz,
            channel_count,
            state: AudioTrackState::Stopped,
            buffer: Vec::new(),
        }
    }

    pub fn play(&mut self) {
        self.state = AudioTrackState::Playing;
    }

    pub fn pause(&mut self) {
        self.state = AudioTrackState::Paused;
    }

    pub fn stop(&mut self) {
        self.state = AudioTrackState::Stopped;
        self.buffer.clear();
    }

    pub fn write(&mut self, audio_data: &[i16]) -> usize {
        self.buffer.extend_from_slice(audio_data);
        audio_data.len()
    }
}
