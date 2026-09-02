/*
 * Reusable playback of a video from the list.
 *
 * Centralizes the only way to ask the player to play a video from the list:
 * [`play_index`][`play_from_list`] resolves the video by its index and sends
 * `PlayerCommand::Load(path)` to the engine thread. The visual output is
 * painted independently by the embedded mpv engine
 * ([`crate::player::embed`]) in the app's playback area.
 *
 * This layer **does not** contain playback logic: it only translates a UI
 * selection into the corresponding load command, and logs the result (start
 * or error) so failures can be diagnosed.
 *
 * It is meant to be reused from anywhere in the interface (double click on
 * the list, the "Play" button, drag and drop, etc.); every call converges on
 * the same `PlayerCommand::Load`.
 */

use std::cell::RefCell;
use std::rc::Rc;

use gtk::prelude::*;

use mos_core::video_list::VideoList;

use crate::player::PlayerCommand;

/** Records success/error marks of playback in the log file. */
mod playback_log {
    use crate::logging;
    use crate::reporting::{self, ErrorKind};

    /** Success message after the video load was queued. */
    pub(super) fn started(path: &str) -> bool {
        logging::info(format!("Reproducción encolada: {path}"))
    }

    /** Message when the list does not contain the requested index. */
    pub(super) fn missing_index(index: usize) -> bool {
        logging::warn(format!("Índice fuera de rango al reproducir: {index}"))
    }

    /** Message when the channel to the engine does not accept the command (engine down or closed). */
    pub(super) fn send_failed(path: &str) -> bool {
        let msg = format!("No se pudo enviar la orden de carga al motor mpv: {path}");
        logging::error(&msg);
        reporting::report(ErrorKind::Player, &msg);
        false
    }

    /** Message when there is no active selection (only applies to variants that require it, such as the "Play" button). */
    pub(super) fn no_selection() -> bool {
        logging::warn("Reproducir solicitado sin vídeo seleccionado")
    }
}

/**
 * Loads the video with index `index` from the list into the player.
 *
 * It is the central and reusable function: it touches nothing in the UI
 * except reading shared state. Returns `true` if the load command was sent to
 * the engine. Logs the start or the error depending on the result.
 */
pub fn play_from_list(
    videos: &Rc<RefCell<VideoList>>,
    index: usize,
    player: &std::sync::mpsc::Sender<PlayerCommand>,
) -> bool {
    let path = {
        let borrowed = videos.borrow();
        match borrowed.get(index) {
            Some(v) => v.path().to_string_lossy().into_owned(),
            None => {
                playback_log::missing_index(index);
                return false;
            }
        }
    };

    let sent = player.send(PlayerCommand::Load(path.clone())).is_ok();
    if sent {
        playback_log::started(&path);
    } else {
        playback_log::send_failed(&path);
    }
    sent
}

/**
 * Plays the currently selected video in the list.
 *
 * Reusable variant for selection-based flows (e.g. the "Play" button).
 * Returns `true` if there was a selection and the command was sent.
 */
pub fn play_selected(
    videos: &Rc<RefCell<VideoList>>,
    player: &std::sync::mpsc::Sender<PlayerCommand>,
) -> bool {
    match videos.borrow().selected_index() {
        Some(i) => play_from_list(videos, i, player),
        None => {
            playback_log::no_selection();
            false
        }
    }
}

/**
 * Connects playback to double click (or Enter) on a row of the list.
 *
 * It is the currently active behavior: play by clicking a video in the side
 * list. Reuses [`play_from_list`].
 */
pub fn connect_double_click(
    list: &gtk::ListBox,
    videos: &Rc<RefCell<VideoList>>,
    player: &std::sync::mpsc::Sender<PlayerCommand>,
) {
    let videos = videos.clone();
    let player = player.clone();
    list.connect_row_activated(move |_list, row| {
        play_from_list(&videos, row.index() as usize, &player);
    });
}

/**
 * Connects a button to launch the selected video in the list.
 *
 * Takes the active selection (the highlighted row) each time the button is
 * pressed. Reuses [`play_selected`].
 */
pub fn connect_play_button(
    button: &gtk::Button,
    videos: &Rc<RefCell<VideoList>>,
    player: &std::sync::mpsc::Sender<PlayerCommand>,
) {
    let videos = videos.clone();
    let player = player.clone();
    button.connect_clicked(move |_| {
        play_selected(&videos, &player);
    });
}
