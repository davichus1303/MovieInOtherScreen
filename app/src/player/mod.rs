//! Capa de reproducción: un reproductor lógico único, ejecutado en su propio
//! hilo, que envuelve libmpv.
//!
//! Objetivos de diseño:
//! - **Un único reproductor lógico**: existe una sola instancia de libmpv, por
//!   tanto un único proceso de decodificación y un único flujo de audio, sin
//!   duplicaciones.
//! - **Sin bloqueos de la UI**: mpv vive en un hilo dedicado; la UI se
//!   comunica por canales (`mpsc`).
//! - **Bajo acoplamiento con la UI**: la UI envía `PlayerCommand` y recibe
//!   `PlayerEvent`; nunca toca libmpv directamente.

pub mod embed;
pub mod ffi;
pub mod mpv_engine;

use std::sync::mpsc::{Receiver, Sender};

/// Comandos enviados por la UI al hilo del reproductor.
#[derive(Debug, Clone, PartialEq)]
pub enum PlayerCommand {
    /// Carga un vídeo nuevo por su ruta.
    Load(String),
    Play,
    Pause,
    Stop,
    /// Busca a una posición en segundos.
    Seek(f64),
    /// Conmuta play/pausa según el estado actual.
    TogglePause,
    /// Establece el dispositivo de salida por su id.
    SetAudioDevice(String),
    /// Pide la lista actual de dispositivos de salida (`AudioDevices`).
    ///
    /// La UI lo envía cuando el selector está construido (pull bajo demanda,
    /// como hacen fono/termixer), para no depender de un push inicial que
    /// podría ocurrir antes de que exista el widget.
    ListAudioDevices,
    /// Solicita terminar el hilo.
    Shutdown,
}

/// Eventos que el reproductor publica hacia la UI.
#[derive(Debug, Clone, PartialEq)]
pub enum PlayerEvent {
    Position(f64),
    Duration(f64),
    Paused(bool),
    Ended,
    /// Lista de dispositivos de audio `(id, descripción)` disponibles.
    AudioDevices(Vec<(String, String)>),
    PlaybackError(String),
}

/// Crea los canales de comunicación con el reproductor.
///
/// Devuelve `(comandos, eventos)` y el hilo que posee la única instancia de
/// mpv ya arrancado.
pub fn spawn_player() -> (Sender<PlayerCommand>, Receiver<PlayerEvent>) {
    let (cmd_tx, cmd_rx) = std::sync::mpsc::channel();
    let (ev_tx, ev_rx) = std::sync::mpsc::channel();
    mpv_engine::spawn(cmd_rx, ev_tx);
    (cmd_tx, ev_rx)
}
