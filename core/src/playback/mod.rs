//! Motor de reproducción: lógica pura para controlar la reproducción.
//!
//! Abstrae el reproductor (mpv u otro) detrás de un trait para que la UI
//! y los tests no dependan de libmpv directamente.

use std::sync::mpsc::{Receiver, Sender};

/// Comandos de reproducción de alto nivel (lenguaje del dominio).
#[derive(Debug, Clone, PartialEq)]
pub enum PlaybackCmd {
    Load(String),
    Play,
    Pause,
    TogglePause,
    Seek(f64),
    Stop,
    SetAudioDevice(String),
    Shutdown,
}

/// Eventos que el motor publica.
#[derive(Debug, Clone)]
pub enum PlaybackEvent {
    Position(f64),
    Duration(f64),
    Ended,
    Paused(bool),
    Error(String),
}

/// Trait que define el contrato del motor de reproducción.
pub trait PlaybackEngine: Send + Sync {
    fn send(&self, cmd: PlaybackCmd) -> Result<(), String>;
    fn is_paused(&self) -> bool;
    fn duration(&self) -> f64;
    fn position(&self) -> f64;
}

/// Estado de reproducción observable.
#[derive(Debug, Default, Clone)]
pub struct PlaybackState {
    pub position: f64,
    pub duration: f64,
    pub paused: bool,
    pub current_path: Option<String>,
    pub audio_device: Option<String>,
}

impl PlaybackState {
    pub fn update_position(&mut self, pos: f64) {
        self.position = pos.clamp(0.0, self.duration);
    }

    pub fn update_duration(&mut self, dur: f64) {
        self.duration = dur.max(0.0);
    }

    pub fn set_paused(&mut self, paused: bool) {
        self.paused = paused;
    }

    pub fn set_path(&mut self, path: Option<String>) {
        self.current_path = path;
    }

    pub fn set_audio_device(&mut self, device: Option<String>) {
        self.audio_device = device;
    }

    pub fn progress(&self) -> f64 {
        if self.duration > 0.0 {
            (self.position / self.duration).clamp(0.0, 1.0)
        } else {
            0.0
        }
    }
}

/// Eventos internos del motor (para comunicación hilo UI ↔ motor).
pub enum EngineEvent {
    Position(f64),
    Duration(f64),
    Ended,
    Paused(bool),
    Error(String),
}

/// Inicia el motor de reproducción en un hilo dedicado.
pub fn spawn_playback_engine(
    cmd_rx: Receiver<PlaybackCmd>,
    event_tx: Sender<PlaybackEvent>,
) -> std::thread::JoinHandle<()> {
    std::thread::Builder::new()
        .name("playback-engine".into())
        .spawn(move || {
            // Implementación concreta (mpv) iría aquí
            // Por ahora es un stub
            loop {
                if let Ok(cmd) = cmd_rx.recv() {
                    match cmd {
                        PlaybackCmd::Shutdown => break,
                        _ => {} // Forward to mpv handler
                    }
                }
            }
        })
        .expect("playback engine thread must start")
}