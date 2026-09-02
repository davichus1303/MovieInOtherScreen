/*! Application sidebar: video list and its buttons (add / clear). */

use std::cell::RefCell;
use std::rc::Rc;

use gtk::glib;
use gtk::prelude::*;

use libadwaita as adw;

use mos_core::monitors::MonitorSet;
use mos_core::video_list::VideoList;

use crate::events::mirror_on_play;
use crate::events::AppState;
use crate::player::{PlayerCommand, PlayerEvent};

/** State needed to build the sidebar. */
pub struct SidebarDeps {
    pub videos: Rc<RefCell<VideoList>>,
    pub player: std::sync::mpsc::Sender<PlayerCommand>,
    pub mirror: Rc<RefCell<crate::mirror::MirrorController>>,
    pub monitors: Rc<RefCell<MonitorSet>>,
}

/** Builds the side bar: video list with its buttons. */
pub fn build_sidebar(deps: SidebarDeps) -> gtk::Box {
    let sidebar = gtk::Box::new(gtk::Orientation::Vertical, 8);
    sidebar.set_margin_top(12);
    sidebar.set_margin_bottom(12);
    sidebar.set_margin_start(12);
    sidebar.set_margin_end(12);
    sidebar.set_width_request(280);

    // --- Sección de videos ---
    let videos_header = gtk::Label::new(Some("Archivos de video"));
    videos_header.add_css_class("title-4");
    videos_header.set_halign(gtk::Align::Start);
    sidebar.append(&videos_header);

    let buttons_row = gtk::Box::new(gtk::Orientation::Horizontal, 6);
    buttons_row.set_halign(gtk::Align::Start);

    let add_button = gtk::Button::with_label("＋ Agregar videos");
    buttons_row.append(&add_button);

    let clear_button = gtk::Button::with_label("Limpiar");
    buttons_row.append(&clear_button);

    sidebar.append(&buttons_row);

    let list = gtk::ListBox::new();
    list.set_vexpand(true);
    list.set_selection_mode(gtk::SelectionMode::Single);
    sidebar.append(&list);

    // Conexiones de la lista de videos
    connect_video_list(&list, &add_button, &clear_button, &deps);

    sidebar
}

/** Connects the video list: add, play, clear. */
fn connect_video_list(
    list: &gtk::ListBox,
    add_button: &gtk::Button,
    clear_button: &gtk::Button,
    deps: &SidebarDeps,
) {
    // ============================================================
    // CRITICAL: Clone EVERYTHING from deps UPFRONT before ANY closures
    // This avoids capturing `deps` (a reference) in 'static closures
    // ============================================================

    // Pre-clone everything from deps
    let videos = deps.videos.clone();
    let player = deps.player.clone();
    let mirror = deps.mirror.clone();
    let monitors = deps.monitors.clone();
    let list_clone = list.clone();

    // --- For add_button closure ---
    let videos_for_add = videos.clone();
    let list_for_add = list_clone.clone();

    // --- For row_activated closure ---
    let videos_for_row = videos.clone();
    let player_for_row = player.clone();
    let mirror_for_row = mirror.clone();
    let monitors_for_row = monitors.clone();
    let list_for_row = list_clone.clone();

    // --- For clear_button closure ---
    let videos_for_clear = videos.clone();
    let list_for_clear = list_clone.clone();
    let player_for_clear = player.clone();
    let mirror_for_clear = mirror.clone();

    // --- add_button closure ---
    add_button.connect_clicked(move |btn| {
        let Some(window) = btn.root().and_downcast::<gtk::Window>() else {
            return;
        };

        let filters = gtk::gio::ListStore::new::<gtk::FileFilter>();
        let filter = gtk::FileFilter::new();
        filter.set_name(Some("Vídeos"));
        for pattern in [
            "*.mp4", "*.mkv", "*.webm", "*.avi", "*.mov", "*.m4v", "*.ogv", "*.ts", "*.m2ts",
        ] {
            filter.add_pattern(pattern);
        }
        filters.append(&filter);

        // Use PRE-CLONED values, NOT deps
        let videos_for_dialog = videos_for_add.clone();
        let list_for_dialog = list_for_add.clone();
        let list_for_add_closure = list_for_add.clone();

        let dialog = gtk::FileDialog::builder()
            .title("Seleccionar vídeos")
            .modal(true)
            .accept_label("Seleccionar")
            .build();
        dialog.set_filters(Some(&filters));

        dialog.open_multiple(Some(&window), None::<&gtk::gio::Cancellable>, {
            let videos_for_add_closure = videos_for_add.clone();
            let list_for_add_closure_inner = list_for_add_closure.clone();
            move |result| {
                let Ok(model) = result else {
                    return;
                };
                let new_videos: Vec<_> = model
                    .iter::<gtk::gio::File>()
                    .filter_map(Result::ok)
                    .filter_map(|f| f.path())
                    .map(mos_core::video_list::Video::new)
                    .collect();
                if !new_videos.is_empty() {
                    let value = videos_for_add_closure.clone();
                    value.borrow_mut().add(new_videos);
                    rebuild_list(&list_for_add_closure_inner, &value);
                }
            }
        });
    });

    // --- row_activated closure ---
    let list_for_row = list_clone.clone();

    list_for_row.connect_row_activated(move |_list, row| {
        let path = {
            let borrowed = videos_for_row.borrow();
            match borrowed.get(row.index() as usize) {
                Some(v) => Some(v.path().to_string_lossy().into_owned()),
                None => None,
            }
        };
        let Some(path) = path else {
            return;
        };
        // Use the cloned player, mirror, monitors
        let _ = player_for_row.send(crate::player::PlayerCommand::Load(path.clone()));
        let st = AppState {
            player: player_for_row.clone(),
            monitors: monitors.clone(),
            mirror: mirror_for_row.clone(),
        };
        mirror_on_play(&st, path);
    });

    // --- clear_button closure ---
    let videos_for_clear_closure = videos_for_clear.clone();
    clear_button.connect_clicked(move |_| {
        // Vacía la lista de vídeos y refresca las filas de la UI.
        let value = videos_for_clear_closure.clone();
        value.borrow_mut().clear();
        rebuild_list(&list_for_clear, &value);
        // Descarga por completo el vídeo del reproductor original (deja la
        // GLArea sin archivo cargado, por lo que ya no se puede reproducir).
        let _ = player_for_clear.send(PlayerCommand::Unload);
        // Detiene (cierra) todas las ventanas espejo y reinicia su estado.
        mirror_for_clear.borrow_mut().reset();
    });

    rebuild_list(&list_clone, &videos_for_clear);
}

/** Refreshes the list rows with the videos from the state. */
fn rebuild_list(list: &gtk::ListBox, videos: &Rc<RefCell<VideoList>>) {
    while let Some(row) = list.row_at_index(0) {
        list.remove(&row);
    }
    for video in videos.borrow().iter() {
        let row = gtk::ListBoxRow::new();
        let label = gtk::Label::new(Some(video.name()));
        label.set_halign(gtk::Align::Start);
        label.set_ellipsize(gtk::pango::EllipsizeMode::End);
        row.set_child(Some(&label));
        list.append(&row);
    }
}
