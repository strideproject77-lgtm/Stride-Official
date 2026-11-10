//! Android audio subsystem emulation.
//!
//! Provides SoundPool, MediaPlayer, and AudioSession bridges for Android applications
//! running within the Stride runtime. All audio operations are tracked and their state
//! is maintained for compatibility without requiring native platform audio APIs.

use std::collections::HashMap;

/// Playback state of a sound stream.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StreamState {
    Idle,
    Playing,
    Paused,
    Stopped,
}

/// A sound buffer registered via SoundPool.load().
#[derive(Debug, Clone)]
pub struct SoundBuffer {
    pub sound_id: u32,
    pub source_path: String,
    pub loaded: bool,
}

/// An active sound stream managed by SoundPool.
#[derive(Debug, Clone)]
pub struct SoundStream {
    pub stream_id: u32,
    pub sound_id: u32,
    pub left_volume: f32,
    pub right_volume: f32,
    pub priority: i32,
    pub loop_count: i32,
    pub rate: f32,
    pub state: StreamState,
}

/// SoundPool emulation layer tracking sounds and streams.
#[derive(Debug, Default)]
pub struct SoundPoolSession {
    pub next_sound_id: u32,
    pub next_stream_id: u32,
    pub sounds: HashMap<u32, SoundBuffer>,
    pub streams: HashMap<u32, SoundStream>,
}

impl SoundPoolSession {
    pub fn new() -> Self {
        Self {
            next_sound_id: 1,
            next_stream_id: 1,
            sounds: HashMap::new(),
            streams: HashMap::new(),
        }
    }

    /// Registers a sound from a path, returning a new sound ID.
    pub fn load(&mut self, path: &str) -> u32 {
        let id = self.next_sound_id;
        self.next_sound_id += 1;
        self.sounds.insert(id, SoundBuffer {
            sound_id: id,
            source_path: path.to_string(),
            loaded: true,
        });
        id
    }

    /// Plays a loaded sound, returning a stream ID.
    pub fn play(
        &mut self,
        sound_id: u32,
        left_volume: f32,
        right_volume: f32,
        priority: i32,
        loop_count: i32,
        rate: f32,
    ) -> u32 {
        if !self.sounds.contains_key(&sound_id) {
            return 0;
        }
        let stream_id = self.next_stream_id;
        self.next_stream_id += 1;
        self.streams.insert(stream_id, SoundStream {
            stream_id,
            sound_id,
            left_volume: left_volume.clamp(0.0, 1.0),
            right_volume: right_volume.clamp(0.0, 1.0),
            priority,
            loop_count,
            rate: rate.clamp(0.5, 2.0),
            state: StreamState::Playing,
        });
        stream_id
    }

    pub fn stop(&mut self, stream_id: u32) {
        if let Some(s) = self.streams.get_mut(&stream_id) {
            s.state = StreamState::Stopped;
        }
    }

    pub fn pause_stream(&mut self, stream_id: u32) {
        if let Some(s) = self.streams.get_mut(&stream_id) {
            if s.state == StreamState::Playing {
                s.state = StreamState::Paused;
            }
        }
    }

    pub fn resume_stream(&mut self, stream_id: u32) {
        if let Some(s) = self.streams.get_mut(&stream_id) {
            if s.state == StreamState::Paused {
                s.state = StreamState::Playing;
            }
        }
    }

    pub fn set_volume(&mut self, stream_id: u32, left: f32, right: f32) {
        if let Some(s) = self.streams.get_mut(&stream_id) {
            s.left_volume = left.clamp(0.0, 1.0);
            s.right_volume = right.clamp(0.0, 1.0);
        }
    }

    pub fn pause_all(&mut self) {
        for s in self.streams.values_mut() {
            if s.state == StreamState::Playing {
                s.state = StreamState::Paused;
            }
        }
    }

    pub fn resume_all(&mut self) {
        for s in self.streams.values_mut() {
            if s.state == StreamState::Paused {
                s.state = StreamState::Playing;
            }
        }
    }

    pub fn active_stream_count(&self) -> usize {
        self.streams.values().filter(|s| s.state == StreamState::Playing).count()
    }
}

/// MediaPlayer state machine.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MediaPlayerState {
    Idle,
    Initialized,
    Prepared,
    Started,
    Paused,
    Stopped,
    PlaybackCompleted,
    End,
    Error,
}

/// A single MediaPlayer instance.
#[derive(Debug, Clone)]
pub struct MediaPlayerInstance {
    pub id: u32,
    pub source: Option<String>,
    pub state: MediaPlayerState,
    pub looping: bool,
    pub left_volume: f32,
    pub right_volume: f32,
    pub duration_ms: u32,
    pub position_ms: u32,
}

impl MediaPlayerInstance {
    pub fn new(id: u32) -> Self {
        Self {
            id,
            source: None,
            state: MediaPlayerState::Idle,
            looping: false,
            left_volume: 1.0,
            right_volume: 1.0,
            duration_ms: 0,
            position_ms: 0,
        }
    }

    pub fn set_data_source(&mut self, path: String) {
        self.source = Some(path);
        self.state = MediaPlayerState::Initialized;
    }

    pub fn prepare(&mut self) {
        if matches!(
            self.state,
            MediaPlayerState::Initialized | MediaPlayerState::Stopped
        ) {
            self.state = MediaPlayerState::Prepared;
        }
    }

    pub fn start(&mut self) {
        if matches!(
            self.state,
            MediaPlayerState::Prepared
                | MediaPlayerState::Paused
                | MediaPlayerState::PlaybackCompleted
        ) {
            self.state = MediaPlayerState::Started;
        }
    }

    pub fn pause_playback(&mut self) {
        if self.state == MediaPlayerState::Started {
            self.state = MediaPlayerState::Paused;
        }
    }

    pub fn stop(&mut self) {
        if !matches!(
            self.state,
            MediaPlayerState::Idle | MediaPlayerState::Initialized
        ) {
            self.state = MediaPlayerState::Stopped;
        }
    }

    pub fn reset(&mut self) {
        self.state = MediaPlayerState::Idle;
        self.source = None;
        self.position_ms = 0;
    }

    pub fn release(&mut self) {
        self.state = MediaPlayerState::End;
    }

    pub fn is_playing(&self) -> bool {
        self.state == MediaPlayerState::Started
    }
}

/// Session managing all MediaPlayer instances in the guest process.
#[derive(Debug, Default)]
pub struct MediaPlayerSession {
    pub next_id: u32,
    pub players: HashMap<u32, MediaPlayerInstance>,
}

impl MediaPlayerSession {
    pub fn new() -> Self {
        Self { next_id: 1, players: HashMap::new() }
    }

    pub fn create(&mut self) -> u32 {
        let id = self.next_id;
        self.next_id += 1;
        self.players.insert(id, MediaPlayerInstance::new(id));
        id
    }

    pub fn get_mut(&mut self, id: u32) -> Option<&mut MediaPlayerInstance> {
        self.players.get_mut(&id)
    }

    pub fn active_count(&self) -> usize {
        self.players.values().filter(|p| p.is_playing()).count()
    }
}

/// Top-level audio session coordinating SoundPool and MediaPlayer subsystems.
#[derive(Debug, Default)]
pub struct AudioSession {
    pub sound_pool: SoundPoolSession,
    pub media_players: MediaPlayerSession,
    pub master_volume: f32,
    pub is_muted: bool,
}

impl AudioSession {
    pub fn new() -> Self {
        Self {
            sound_pool: SoundPoolSession::new(),
            media_players: MediaPlayerSession::new(),
            master_volume: 1.0,
            is_muted: false,
        }
    }

    pub fn on_app_pause(&mut self) {
        self.sound_pool.pause_all();
    }

    pub fn on_app_resume(&mut self) {
        if !self.is_muted {
            self.sound_pool.resume_all();
        }
    }

    pub fn set_master_volume(&mut self, volume: f32) {
        self.master_volume = volume.clamp(0.0, 1.0);
    }

    pub fn mute(&mut self) {
        self.is_muted = true;
    }

    pub fn unmute(&mut self) {
        self.is_muted = false;
    }

    pub fn diagnostic_summary(&self) -> String {
        format!(
            "AudioSession: {} active streams, {} active players, master_volume={:.2}, muted={}",
            self.sound_pool.active_stream_count(),
            self.media_players.active_count(),
            self.master_volume,
            self.is_muted,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sound_pool_load_and_play() {
        let mut pool = SoundPoolSession::new();
        let sid = pool.load("explosion.ogg");
        assert_eq!(sid, 1);
        let stream = pool.play(sid, 1.0, 1.0, 1, 0, 1.0);
        assert_eq!(stream, 1);
        assert_eq!(pool.active_stream_count(), 1);
        pool.stop(stream);
        assert_eq!(pool.active_stream_count(), 0);
    }

    #[test]
    fn sound_pool_pause_resume() {
        let mut pool = SoundPoolSession::new();
        let sid = pool.load("sfx.wav");
        let stream = pool.play(sid, 1.0, 1.0, 1, 0, 1.0);
        pool.pause_stream(stream);
        assert_eq!(pool.active_stream_count(), 0);
        pool.resume_stream(stream);
        assert_eq!(pool.active_stream_count(), 1);
    }

    #[test]
    fn media_player_lifecycle() {
        let mut session = MediaPlayerSession::new();
        let id = session.create();
        let p = session.get_mut(id).unwrap();
        p.set_data_source("bgm.mp3".to_string());
        p.prepare();
        p.start();
        assert!(p.is_playing());
        p.pause_playback();
        assert!(!p.is_playing());
        p.stop();
        p.reset();
        assert_eq!(p.state, MediaPlayerState::Idle);
    }

    #[test]
    fn audio_session_pause_resume_lifecycle() {
        let mut sess = AudioSession::new();
        let sid = sess.sound_pool.load("sfx.wav");
        sess.sound_pool.play(sid, 1.0, 1.0, 1, 0, 1.0);
        assert_eq!(sess.sound_pool.active_stream_count(), 1);
        sess.on_app_pause();
        assert_eq!(sess.sound_pool.active_stream_count(), 0);
        sess.on_app_resume();
        assert_eq!(sess.sound_pool.active_stream_count(), 1);
    }

    #[test]
    fn muted_session_does_not_resume_on_foreground() {
        let mut sess = AudioSession::new();
        let sid = sess.sound_pool.load("music.ogg");
        sess.sound_pool.play(sid, 1.0, 1.0, 1, 0, 1.0);
        sess.mute();
        sess.on_app_pause();
        sess.on_app_resume(); // muted, should not resume
        assert_eq!(sess.sound_pool.active_stream_count(), 0);
    }
}
