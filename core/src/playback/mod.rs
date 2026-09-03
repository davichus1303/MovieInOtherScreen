/*! Playback engine: pure logic for controlling playback.
 *
 * Abstracts the player (mpv or other) behind a trait so that the UI
 * and tests do not depend on libmpv directly.
 */

use std::sync::mpsc::{Receiver, Sender};

use crate::constants::playback::ENGINE_THREAD_NAME;

/** High-level playback commands (domain language). */
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

/** Events published by the engine. */
#[derive(Debug, Clone)]
pub enum PlaybackEvent {
    Position(f64),
    Duration(f64),
    Ended,
    Paused(bool),
    Error(String),
}

/** Trait defining the playback engine contract. */
pub trait PlaybackEngine: Send + Sync {
    fn send(&self, cmd: PlaybackCmd) -> Result<(), String>;
    fn is_paused(&self) -> bool;
    fn duration(&self) -> f64;
    fn position(&self) -> f64;
}

/** Observable playback state. */
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

/** Internal engine events (for UI ↔ engine thread communication). */
pub enum EngineEvent {
    Position(f64),
    Duration(f64),
    Ended,
    Paused(bool),
    Error(String),
}

/** Starts the playback engine on a dedicated thread. */
pub fn spawn_playback_engine(
    cmd_rx: Receiver<PlaybackCmd>,
    _event_tx: Sender<PlaybackEvent>,
) -> std::thread::JoinHandle<()> {
    std::thread::Builder::new()
        .name(ENGINE_THREAD_NAME.into())
        .spawn(move || {
            // Implementación concreta (mpv) iría aquí
            // Por ahora es un stub
            loop {
                if let Ok(cmd) = cmd_rx.recv() {
                    if cmd == PlaybackCmd::Shutdown {
                        break;
                    }
                    // Resto de comandos se reenviarían al manejador de mpv
                }
            }
        })
        .expect("playback engine thread must start")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn estado_inicial_sin_reproduccion() {
        let s = PlaybackState::default();
        assert_eq!(s.position, 0.0);
        assert_eq!(s.duration, 0.0);
        assert!(!s.paused);
        assert_eq!(s.current_path, None);
        assert_eq!(s.audio_device, None);
        assert_eq!(s.progress(), 0.0);
    }

    #[test]
    fn posicion_se_clampea_a_la_duracion() {
        let mut s = PlaybackState::default();
        s.update_duration(100.0);
        s.update_position(150.0);
        assert_eq!(s.position, 100.0);
        s.update_position(-5.0);
        assert_eq!(s.position, 0.0);
    }

    #[test]
    fn duracion_negativa_se_recorta_a_cero() {
        let mut s = PlaybackState::default();
        s.update_duration(-10.0);
        assert_eq!(s.duration, 0.0);
    }

    #[test]
    fn progress_mide_el_avance_normalizado() {
        let mut s = PlaybackState::default();
        s.update_duration(200.0);
        s.update_position(100.0);
        assert!((s.progress() - 0.5).abs() < f64::EPSILON);
    }

    #[test]
    fn progress_se_clampea_al_rango() {
        let mut s = PlaybackState::default();
        s.update_duration(10.0);
        s.update_position(100.0);
        assert_eq!(s.progress(), 1.0);
        s.update_position(0.0);
        assert_eq!(s.progress(), 0.0);
    }

    #[test]
    fn progress_es_cero_sin_duracion() {
        let mut s = PlaybackState::default();
        s.update_position(5.0);
        assert_eq!(s.progress(), 0.0);
    }

    #[test]
    fn set_paused_y_estado_de_archivo() {
        let mut s = PlaybackState::default();
        s.set_paused(true);
        assert!(s.paused);
        s.set_path(Some("video.mp4".to_string()));
        assert_eq!(s.current_path.as_deref(), Some("video.mp4"));
        s.set_audio_device(Some("hdmi".to_string()));
        assert_eq!(s.audio_device.as_deref(), Some("hdmi"));
        s.set_path(None);
        assert_eq!(s.current_path, None);
    }
}
