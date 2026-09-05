/*
 * GTK4/libadwaita interface layer.
 *
 * Builds the main window and connects the widgets with the domain logic
 * (`mos_core`) and with the player (`player`). This layer **does not**
 * contain playback or business logic: it interprets user commands and
 * reflects state.
 *
 * Build note: this module depends on GTK4/libadwaita and is compiled
 * inside the GNOME SDK (Flatpak) or in CI; the domain logic is not.
 */

use std::cell::RefCell;
use std::rc::Rc;

use gtk::glib;
use gtk::prelude::*;

use libadwaita as adw;

use mos_core::monitors::MonitorSet;
use mos_core::video_list::VideoList;

use crate::constants::app;
use crate::mirror;
use crate::monitor_widget;
use crate::player::PlayerCommand;
use crate::player_area::{self, Timeline};
use crate::sidebar;

/** Shared state for the whole interface. */
#[derive(Clone)]
struct AppState {
    /** Commands to the player thread. */
    player: std::sync::mpsc::Sender<PlayerCommand>,
    /** Detected monitors and their selection. */
    monitors: Rc<RefCell<MonitorSet>>,
    /** Playback mirrors to the selected monitors. */
    mirror: Rc<RefCell<mirror::MirrorController>>,
}

/** Interface entry point. */
pub fn build_main_window(application: &adw::Application) -> adw::ApplicationWindow {
    let (cmd_tx, ev_rx) = crate::player::spawn_player();

    let state = AppState {
        player: cmd_tx.clone(),
        monitors: Rc::new(RefCell::new(MonitorSet::new())),
        mirror: Rc::new(RefCell::new(mirror::MirrorController::new(application))),
    };

    let toolbar = adw::ToolbarView::new();
    let header = adw::HeaderBar::new();
    header.set_title_widget(Some(&gtk::Label::new(Some(app::APP_TITLE))));
    toolbar.add_top_bar(&header);

    let video_list = Rc::new(RefCell::new(VideoList::new()));
    let (content, timeline) = build_layout(application, &video_list, &state);

    // Overlay de toasts: los errores de la app se muestran aquí como mensajes
    // emergentes (el canal de reporting se conecta al mismo tiempo).
    let toast_overlay = adw::ToastOverlay::new();
    toast_overlay.set_child(Some(&content));
    crate::reporting::attach(&toast_overlay);

    toolbar.set_content(Some(&toast_overlay));

    let window = adw::ApplicationWindow::builder()
        .application(application)
        .title(app::APP_TITLE)
        .default_width(app::WINDOW_DEFAULT_WIDTH)
        .default_height(app::WINDOW_DEFAULT_HEIGHT)
        .content(&toolbar)
        .build();

    // Lleva los eventos del reproductor al hilo principal y los refleja en la
    // barra de progreso (posición / duración).
    crate::events::bridge_events_to_gtk(ev_rx, timeline);

    let mirror_state = state.mirror.clone();
    window.connect_close_request(move |_| {
        // Mirrors and the engine shut down through the `unrealize` of their
        // GLAreas, once the `mpv_render_context` is freed (avoids libmpv's
        // `queue_dtor` `assert` when destroying the `mpv_handle`s).
        mirror_state.borrow_mut().clear();
        glib::Propagation::Proceed
    });

    window
}

/** Main layout: sidebar (30%) | playback area (70%). */
fn build_layout(
    application: &adw::Application,
    video_list: &Rc<RefCell<VideoList>>,
    state: &AppState,
) -> (gtk::Box, Rc<RefCell<crate::player_area::Timeline>>) {
    let paned = gtk::Paned::new(gtk::Orientation::Horizontal);

    // Sidebar con videos, monitores y audio
    let sidebar_deps = sidebar::SidebarDeps {
        videos: video_list.clone(),
        player: state.player.clone(),
        mirror: state.mirror.clone(),
        monitors: state.monitors.clone(),
    };
    let sidebar_widget = sidebar::build_sidebar(sidebar_deps);
    paned.set_start_child(Some(&sidebar_widget));

    // Área de reproducción: vídeo + controles + timeline + monitores
    let (area, timeline) = build_player_area(application, state);
    paned.set_end_child(Some(&area));
    paned.set_position(app::SIDEBAR_INITIAL_POSITION);

    let root = gtk::Box::new(gtk::Orientation::Horizontal, app::ROOT_BOX_SPACING);
    root.append(&paned);
    (root, timeline)
}

fn build_player_area(
    application: &adw::Application,
    state: &AppState,
) -> (gtk::Box, Rc<RefCell<crate::player_area::Timeline>>) {
    let video = gtk::Frame::new(Some(app::LABEL_VIDEO_FRAME));
    video.set_vexpand(true);
    video.set_valign(gtk::Align::Fill);
    video.set_hexpand(true);
    let embedded = crate::player::embed::EmbeddedVideo::new();
    video.set_child(Some(embedded.widget()));
    // The engine shuts down when the main GLArea is destroyed (`unrealize`),
    // after its `mpv_render_context` is freed: libmpv requires that order
    // before destroying the `mpv_handle` (avoids the `queue_dtor` `assert`).
    let player_for_shutdown = state.player.clone();
    embedded.widget().connect_unrealize(move |_| {
        let _ = player_for_shutdown.send(PlayerCommand::Shutdown);
    });

    let controls = player_area::build_controls(&state.player, state.mirror.clone());

    let (timeline_row, timeline) = player_area::Timeline::new();
    Timeline::connect_seek(&timeline, state.player.clone(), state.mirror.clone());

    let column = gtk::Box::new(gtk::Orientation::Vertical, app::MAIN_COLUMN_SPACING);
    column.append(&video);
    column.append(&controls);
    column.append(&timeline_row);

    // Monitores en el área de reproducción (lado derecho)
    let monitors = monitor_widget::build_monitors_section(&monitor_widget::MonitorDeps {
        mirror: state.mirror.clone(),
        monitors: state.monitors.clone(),
        application: application.clone(),
    });
    column.append(&monitors);

    (column, timeline)
}
