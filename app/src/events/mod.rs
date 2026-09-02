//! Puente de eventos entre el reproductor (hilo background) y la UI (hilo principal).

use std::cell::RefCell;
use std::rc::Rc;

use gtk::glib;

use crate::player::{PlayerCommand, PlayerEvent};

use crate::mirror::MirrorController;
use mos_core::monitors::MonitorSet;

/// Conecta el receptor de eventos del reproductor al timeline de la UI.
pub fn bridge_events_to_gtk(
    rx: std::sync::mpsc::Receiver<PlayerEvent>,
    timeline: Rc<RefCell<crate::player_area::Timeline>>,
) {
    glib::timeout_add_local(std::time::Duration::from_millis(33), move || {
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

/// Reproduce un vídeo en el reproductor principal y abre espejos en monitores seleccionados.
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

/// Reconcilla los espejos con la selección de monitores actual.
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

/// Envía una orden de control a todos los espejos abiertos.
pub fn mirror_control(state: &AppState, cmd: crate::mirror::MirrorCmd) {
    state.mirror.borrow_mut().control(cmd);
}

/// Estado compartido por toda la interfaz (re-export para uso en eventos).
#[derive(Clone)]
pub struct AppState {
    pub player: std::sync::mpsc::Sender<crate::player::PlayerCommand>,
    pub monitors: std::rc::Rc<std::cell::RefCell<MonitorSet>>,
    pub mirror: std::rc::Rc<std::cell::RefCell<crate::mirror::MirrorController>>,
}
