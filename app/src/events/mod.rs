/*! Event bridge between the player (background thread) and the UI (main thread). */

use std::cell::RefCell;
use std::rc::Rc;

use gtk::glib;

use crate::constants::events::BRIDGE_INTERVAL_MS;
use crate::player::{PlayerCommand, PlayerEvent};

use crate::mirror::MirrorController;
use mos_core::monitors::MonitorSet;

/** Connects the player event receiver to the UI timeline. */
pub fn bridge_events_to_gtk(
    rx: std::sync::mpsc::Receiver<PlayerEvent>,
    timeline: Rc<RefCell<crate::player_area::Timeline>>,
) {
    glib::timeout_add_local(std::time::Duration::from_millis(BRIDGE_INTERVAL_MS as u64), move || {
        while let Ok(event) = rx.try_recv() {
            match event {
                PlayerEvent::Position(pos) => timeline.borrow().update_position(pos),
                PlayerEvent::Duration(dur) => timeline.borrow_mut().update_duration(dur),
                PlayerEvent::Ended => timeline.borrow().update_position(0.0),
                PlayerEvent::PlaybackError(msg) => {
                    crate::reporting::report(crate::reporting::ErrorKind::Player, msg);
                }
                _ => {}
            }
        }
        glib::ControlFlow::Continue
    });
}

/** Plays a video on the main player and opens mirrors on selected monitors. */
pub fn mirror_on_play(state: &AppState, path: String) {
    state.mirror.borrow_mut().set_playing(path.clone());
    let selected: Vec<String> = state
        .monitors
        .borrow()
        .selected()
        .map(|m| m.id().to_string())
        .collect();
    let mut mirror = state.mirror.borrow_mut();
    mirror.reconfigure(&selected, None);
    mirror.control(crate::mirror::MirrorCmd::Play);
}

/** Reconciles the mirrors with the current monitor selection. */
pub fn mirror_reconcile(state: &AppState) {
    let has_playback = !state.mirror.borrow().is_idle();
    let pos = if has_playback {
        crate::mirror::main_time_pos()
    } else {
        None
    };
    let selected: Vec<String> = state
        .monitors
        .borrow()
        .selected()
        .map(|m| m.id().to_string())
        .collect();
    state.mirror.borrow_mut().reconfigure(&selected, pos);
}

/** Sends a control command to all open mirrors. */
pub fn mirror_control(state: &AppState, cmd: crate::mirror::MirrorCmd) {
    state.mirror.borrow_mut().control(cmd);
}

/** Shared state for the whole interface (re-export for use in events). */
#[derive(Clone)]
pub struct AppState {
    pub player: std::sync::mpsc::Sender<crate::player::PlayerCommand>,
    pub monitors: std::rc::Rc<std::cell::RefCell<MonitorSet>>,
    pub mirror: std::rc::Rc<std::cell::RefCell<crate::mirror::MirrorController>>,
}
