//! Capa de interfaz GTK4/libadwaita.
//!
//! Construye la ventana principal y conecta los widgets con la lógica de
//! dominio (`mos_core`) y con el reproductor (`player`). Esta capa **no**
//! contiene lógica de reproducción ni de negocio: interpreta comandos del
//! usuario y refleja el estado.
//!
//! Nota de construcción: este módulo depende de GTK4/libadwaita y se compila
//! dentro del SDK GNOME (Flatpak) o en el CI; la lógica de dominio no.

use std::cell::RefCell;
use std::rc::Rc;

use gtk::glib;
use gtk::prelude::*;

use libadwaita as adw;

use mos_core::monitors::{Monitor, MonitorKind, MonitorSet};
use mos_core::video_list::VideoList;

use crate::events;
use crate::mirror;
use crate::monitor_widget;
use crate::player::{PlayerCommand, PlayerEvent};
use crate::player_area::{self, Timeline};
use crate::sidebar;

/// Estado compartido por toda la interfaz.
#[derive(Clone)]
struct AppState {
    /// Comandos hacia el hilo del reproductor.
    player: std::sync::mpsc::Sender<PlayerCommand>,
    /// Monitores detectados y su selección.
    monitors: Rc<RefCell<MonitorSet>>,
    /// Espejos de reproducción hacia los monitores seleccionados.
    mirror: Rc<RefCell<mirror::MirrorController>>,
}

/// Puerto de entrada de la interfaz.
pub fn build_main_window(application: &adw::Application) -> adw::ApplicationWindow {
    let (cmd_tx, ev_rx) = crate::player::spawn_player();

    let state = AppState {
        player: cmd_tx.clone(),
        monitors: Rc::new(RefCell::new(MonitorSet::new())),
        mirror: Rc::new(RefCell::new(mirror::MirrorController::new(application))),
    };

    let toolbar = adw::ToolbarView::new();
    let header = adw::HeaderBar::new();
    header.set_title_widget(Some(&gtk::Label::new(Some("Movies on Other Screens"))));
    toolbar.add_top_bar(&header);

    let video_list = Rc::new(RefCell::new(VideoList::new()));
    let (content, timeline) = build_layout(&video_list, &state);

    // Overlay de toasts: los errores de la app se muestran aquí como mensajes
    // emergentes (el canal de reporting se conecta al mismo tiempo).
    let toast_overlay = adw::ToastOverlay::new();
    toast_overlay.set_child(Some(&content));
    crate::reporting::attach(&toast_overlay);

    toolbar.set_content(Some(&toast_overlay));

    let window = adw::ApplicationWindow::builder()
        .application(application)
        .title("Movies on Other Screens")
        .default_width(1200)
        .default_height(760)
        .content(&toolbar)
        .build();

    // Lleva los eventos del reproductor al hilo principal y los refleja en la
    // barra de progreso (posición / duración).
    crate::events::bridge_events_to_gtk(ev_rx, timeline);

    let player_cmd = state.player.clone();
    let mirror_state = state.mirror.clone();
    window.connect_close_request(move |_| {
        mirror_state.borrow_mut().clear();
        let _ = player_cmd.send(PlayerCommand::Shutdown);
        glib::Propagation::Proceed
    });

    window
}

/// Layout principal: sidebar (30%) | área de reproducción (70%).
fn build_layout(
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
    let (area, timeline) = build_player_area(state);
    paned.set_end_child(Some(&area));
    paned.set_position(360);

    let root = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    root.append(&paned);
    (root, timeline)
}

fn build_player_area(state: &AppState) -> (gtk::Box, Rc<RefCell<crate::player_area::Timeline>>) {
    let video = gtk::Frame::new(Some("Vídeo"));
    video.set_vexpand(true);
    video.set_valign(gtk::Align::Fill);
    video.set_hexpand(true);
    let embedded = crate::player::embed::EmbeddedVideo::new();
    video.set_child(Some(embedded.widget()));

    let controls = player_area::build_controls(&state.player, state.mirror.clone());

    let (timeline_row, timeline) = player_area::Timeline::new();
    Timeline::connect_seek(&timeline, state.player.clone(), state.mirror.clone());

    let column = gtk::Box::new(gtk::Orientation::Vertical, 4);
    column.append(&video);
    column.append(&controls);
    column.append(&timeline_row);

    // Monitores en el área de reproducción (lado derecho)
    let monitors = monitor_widget::build_monitors_section(&monitor_widget::MonitorDeps {
        player: state.player.clone(),
        mirror: state.mirror.clone(),
        monitors: state.monitors.clone(),
    });
    column.append(&monitors);

    (column, timeline)
}
