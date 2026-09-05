/*!
 * Playback layer: a single logical player, running on its own thread,
 * wrapping libmpv.
 *
 * Design goals:
 * - **Single logical player**: only one instance of libmpv exists, hence one
 *   decoding process and one audio stream, with no duplication.
 * - **No UI blocking**: mpv lives on a dedicated thread; the UI communicates
 *   via channels (`mpsc`).
 * - **Loose coupling with the UI**: the UI sends `PlayerCommand` and receives
 *   `PlayerEvent`; it never touches libmpv directly.
 */

pub mod embed;
pub mod ffi;
pub mod mpv_engine;

use std::sync::mpsc::{Receiver, Sender};

/** Commands sent by the UI to the player thread. */
#[derive(Debug, Clone, PartialEq)]
pub enum PlayerCommand {
    /** Loads a new video by its path. */
    Load(String),
    Play,
    Pause,
    Stop,
    /** Fully unloads the current video (leaves the player without a file). */
    Unload,
    /** Seeks to a position in seconds. */
    Seek(f64),
    /**
     * Sets the playback volume (0-100). Software volume: independent of the
     * system mixer and capped so it never exceeds the system limits.
     */
    Volume(f64),
    /** Mutes or unmutes the audio. */
    Mute(bool),
    /** Requests thread termination. */
    Shutdown,
}

/** Events the player publishes to the UI. */
#[derive(Debug, Clone, PartialEq)]
pub enum PlayerEvent {
    Position(f64),
    Duration(f64),
    Paused(bool),
    Ended,
    PlaybackError(String),
}

/**
 * Creates the communication channels with the player.
 *
 * Returns `(commands, events)` and the thread that owns the single instance
 * of mpv already started.
 */
pub fn spawn_player() -> (Sender<PlayerCommand>, Receiver<PlayerEvent>) {
    let (cmd_tx, cmd_rx) = std::sync::mpsc::channel();
    let (ev_tx, ev_rx) = std::sync::mpsc::channel();
    mpv_engine::spawn(cmd_rx, ev_tx);
    (cmd_tx, ev_rx)
}
