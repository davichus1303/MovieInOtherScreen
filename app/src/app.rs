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

use crate::player::{PlayerCommand, PlayerEvent};

/// Estado compartido por toda la interfaz.
#[derive(Clone)]
struct AppState {
    /// Comandos hacia el hilo del reproductor.
    player: std::sync::mpsc::Sender<PlayerCommand>,
    /// Monitores detectados y su selección.
    monitors: Rc<RefCell<MonitorSet>>,
}

/// Puerto de entrada de la interfaz.
pub fn build_main_window(application: &adw::Application) -> adw::ApplicationWindow {
    let (cmd_tx, ev_rx) = crate::player::spawn_player();

    let state = AppState {
        player: cmd_tx.clone(),
        monitors: Rc::new(RefCell::new(MonitorSet::new())),
    };

    let toolbar = adw::ToolbarView::new();
    let header = adw::HeaderBar::new();
    header.set_title_widget(Some(&gtk::Label::new(Some("Movies on Other Screens"))));
    toolbar.add_top_bar(&header);

    let video_list = Rc::new(RefCell::new(VideoList::new()));
    let content = build_layout(&video_list, &state);
    toolbar.set_content(Some(&content));

    let window = adw::ApplicationWindow::builder()
        .application(application)
        .title("Movies on Other Screens")
        .default_width(1200)
        .default_height(760)
        .content(&toolbar)
        .build();

    // Lleva los eventos del reproductor al hilo principal.
    bridge_events_to_gtk(ev_rx);

    let player_cmd = cmd_tx;
    window.connect_close_request(move |_| {
        let _ = player_cmd.send(PlayerCommand::Shutdown);
        glib::Propagation::Proceed
    });

    window
}

/// Layout principal: sidebar (30%) | área de reproducción (70%).
fn build_layout(
    video_list: &Rc<RefCell<VideoList>>,
    state: &AppState,
) -> gtk::Box {
    let paned = gtk::Paned::new(gtk::Orientation::Horizontal);

    let sidebar = Sidebar::new(video_list, state).build();
    paned.set_start_child(Some(&sidebar));

    let area = PlayerArea::new(state).build();
    paned.set_end_child(Some(&area));
    paned.set_position(360);

    let root = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    root.append(&paned);
    root
}

/// Consume los eventos del reproductor en el hilo principal, sondeando el
/// canal `std::mpsc` con un temporizador ligero para no bloquear la UI.
///
/// Por ahora la reproducción se maneja solo por comandos (play, pause, load);
/// los eventos de posicion no se reflejan aún en la interfaz.
fn bridge_events_to_gtk(rx: std::sync::mpsc::Receiver<PlayerEvent>) {
    glib::timeout_add_local(std::time::Duration::from_millis(50), move || {
        while rx.try_recv().is_ok() {}
        glib::ControlFlow::Continue
    });
}

/// --- Sidebar ---
struct Sidebar<'a> {
    videos: &'a Rc<RefCell<VideoList>>,
    player: std::sync::mpsc::Sender<PlayerCommand>,
}

impl<'a> Sidebar<'a> {
    fn new(videos: &'a Rc<RefCell<VideoList>>, state: &'a AppState) -> Self {
        Self {
            videos,
            player: state.player.clone(),
        }
    }

    fn build(self) -> gtk::Box {
        let videos = self.videos.clone();
        let player = self.player.clone();

        let sidebar = gtk::Box::new(gtk::Orientation::Vertical, 8);
        sidebar.set_margin_top(12);
        sidebar.set_margin_bottom(12);
        sidebar.set_margin_start(12);
        sidebar.set_margin_end(12);
        sidebar.set_width_request(280);

        let add_button = gtk::Button::with_label("＋ Agregar videos");
        add_button.set_halign(gtk::Align::Start);
        sidebar.append(&add_button);

        let list = gtk::ListBox::new();
        list.set_vexpand(true);
        list.set_selection_mode(gtk::SelectionMode::Single);
        sidebar.append(&list);

        let separator = gtk::Separator::new(gtk::Orientation::Horizontal);
        sidebar.append(&separator);

        let clear_button = gtk::Button::with_label("Limpiar selección");
        clear_button.set_halign(gtk::Align::Start);
        sidebar.append(&clear_button);

        // Agregar vídeos desde un selector de archivos (API GTK 4.10+).
        let videos_add = videos.clone();
        let list_btn = list.clone();
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

            let dialog = gtk::FileDialog::builder()
                .title("Seleccionar vídeos")
                .modal(true)
                .accept_label("Seleccionar")
                .build();
            dialog.set_filters(Some(&filters));

            let videos_v = videos_add.clone();
            let list_add = list_btn.clone();
            dialog.open_multiple(
                Some(&window),
                None::<&gtk::gio::Cancellable>,
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
                        videos_v.borrow_mut().add(new_videos);
                        rebuild_list(&list_add, &videos_v);
                    }
                },
            );
        });

        // Reproducir el vídeo con doble clic (o Enter) sobre una fila.
        let videos_sel = videos.clone();
        let player_sel = player.clone();
        list.connect_row_activated(move |_list, row| {
            if let Some(video) = videos_sel.borrow().get(row.index() as usize) {
                let path = video.path().to_string_lossy().into_owned();
                let _ = player_sel.send(PlayerCommand::Load(path));
            }
        });

        // Limpiar la selección, sin borrar los vídeos de la lista.
        let videos_clear = videos.clone();
        let list_clear = list.clone();
        clear_button.connect_clicked(move |_| {
            videos_clear.borrow_mut().clear_selection();
            list_clear.unselect_all();
        });

        rebuild_list(&list, &videos);
        sidebar
    }
}

/// Refresca las filas de la lista con los vídeos del estado.
fn rebuild_list(list: &gtk::ListBox, videos: &RefCell<VideoList>) {
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

/// --- Área de reproducción ---
struct PlayerArea {
    state: AppState,
}

impl PlayerArea {
    fn new(state: &AppState) -> Self {
        Self {
            state: state.clone(),
        }
    }

    fn build(&self) -> gtk::Box {
        let player = self.state.player.clone();

        let column = gtk::Box::new(gtk::Orientation::Vertical, 4);

        // Vídeo (50%): salida de mpv embebida en un GLArea (Celluloid-style).
        let video = gtk::Frame::new(Some("Vídeo"));
        video.set_vexpand(true);
        video.set_valign(gtk::Align::Fill);
        let embedded = crate::player::embed::EmbeddedVideo::new();
        video.set_child(Some(embedded.widget()));
        column.append(&video);

        // Controles (~10%).
        let controls = gtk::Box::new(gtk::Orientation::Horizontal, 6);
        controls.set_halign(gtk::Align::Center);
        controls.set_margin_top(6);
        controls.set_margin_bottom(6);
        for (label, command) in [
            ("⏮", PlayerCommand::Stop),
            ("▶", PlayerCommand::Play),
            ("⏸", PlayerCommand::Pause),
            ("⏹", PlayerCommand::Stop),
        ] {
            let send = player.clone();
            let button = gtk::Button::with_label(label);
            button.connect_clicked(move |_| {
                let _ = send.send(command.clone());
            });
            controls.append(&button);
        }
        column.append(&controls);

        // Barra de progreso (10%).
        let progress = gtk::Scale::with_range(gtk::Orientation::Horizontal, 0.0, 100.0, 1.0);
        progress.set_draw_value(false);
        progress.set_hexpand(true);
        let send = player.clone();
        progress.connect_change_value(move |_, _, value| {
            let _ = send.send(PlayerCommand::Seek(value * 100.0));
            glib::Propagation::Proceed
        });
        column.append(&progress);

        // Monitores (~30%).
        let monitors = build_monitors(&self.state);
        monitors.set_vexpand(true);
        column.append(&monitors);

        column
    }
}

/// Sección de monitores y salida de audio.
fn build_monitors(state: &AppState) -> gtk::Box {
    detect_monitors(&state.monitors);

    let section = gtk::Box::new(gtk::Orientation::Vertical, 8);
    section.set_margin_top(8);
    section.set_margin_bottom(12);
    section.set_margin_start(12);
    section.set_margin_end(12);

    let title = gtk::Label::new(Some("Monitores"));
    title.set_halign(gtk::Align::Start);
    title.add_css_class("title-4");
    section.append(&title);

    let hint = gtk::Label::new(Some(
        "Selecciona uno o varios monitores para reproducir en pantalla completa. \
         El principal queda fijo.",
    ));
    hint.set_halign(gtk::Align::Start);
    hint.set_wrap(true);
    hint.set_max_width_chars(60);
    section.append(&hint);

    // Fila horizontal de monitores.
    let mirrors = monitors_row(&state.monitors);
    mirrors.set_halign(gtk::Align::Start);
    section.append(&mirrors);

    section
}

/// Detecta los monitores del sistema (GDK) y actualiza el conjunto lógico.
fn detect_monitors(monitors: &Rc<RefCell<MonitorSet>>) {
    let mut found: Vec<(gtk::gdk::Rectangle, String)> = Vec::new();
    let display = match gtk::gdk::Display::default() {
        Some(d) => d,
        None => {
            monitors.borrow_mut().update_from_detected(Vec::new());
            return;
        }
    };
    for item in display.monitors().iter::<gtk::gdk::Monitor>().filter_map(Result::ok) {
        let label = item
            .model()
            .map(|s| s.to_string())
            .unwrap_or_else(|| "Monitor".to_string());
        found.push((item.geometry(), label));
    }

    // El principal es el que contiene el origen (0,0); si no hay ninguno,
    // se toma el primero.
    let primary_index = found
        .iter()
        .position(|(g, _)| g.x() == 0 && g.y() == 0)
        .unwrap_or(0);

    let detected: Vec<Monitor> = found
        .into_iter()
        .enumerate()
        .map(|(i, (_g, label))| {
            let kind = if i == primary_index {
                MonitorKind::Primary
            } else {
                MonitorKind::Secondary
            };
            Monitor::new(format!("gdk-{i}"), label, kind)
        })
        .collect();

    monitors.borrow_mut().update_from_detected(detected);
}

/// Fila horizontal con una tarjeta por monitor.
fn monitors_row(monitors: &Rc<RefCell<MonitorSet>>) -> gtk::Box {
    let row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    row.set_halign(gtk::Align::Start);

    let set = monitors.borrow();
    if set.is_empty() {
        let label = gtk::Label::new(Some("No se detectó ningún monitor."));
        label.set_halign(gtk::Align::Start);
        row.append(&label);
        return row;
    }
    for mon in set.iter() {
        row.append(&monitor_card(mon, monitors));
    }
    row
}

/// Tarjeta seleccionable de un monitor.
fn monitor_card(mon: &Monitor, monitors: &Rc<RefCell<MonitorSet>>) -> gtk::ToggleButton {
    let kind = if mon.is_primary() {
        "Principal"
    } else {
        "Secundario"
    };
    let id = mon.id().to_string();
    let label = mon.label().to_string();

    let button = gtk::ToggleButton::with_label(&format!("{kind}\n{label}"));
    button.set_size_request(150, 70);
    button.set_halign(gtk::Align::Start);
    button.add_css_class("card");
    button.set_active(mon.is_selected());

    if mon.is_primary() {
        button.set_sensitive(false);
    } else {
        let state = monitors.clone();
        button.connect_toggled(move |_| {
            let _ = state.borrow_mut().toggle(&id);
        });
    }
    button
}