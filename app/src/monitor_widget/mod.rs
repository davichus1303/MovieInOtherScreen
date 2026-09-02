//! Widgets para selección de monitores.

use gtk::prelude::*;

use mos_core::monitors::{Monitor, MonitorKind, MonitorSet};
use crate::mirror::MirrorController;
use crate::player::PlayerCommand;
use crate::events::AppState;

/// Estado necesario para construir widgets de monitores.
pub struct MonitorDeps {
    pub player: std::sync::mpsc::Sender<PlayerCommand>,
    pub mirror: std::rc::Rc<std::cell::RefCell<MirrorController>>,
    pub monitors: std::rc::Rc<std::cell::RefCell<MonitorSet>>,
}

/// Construye la sección completa de monitores (título + hint + fila).
pub fn build_monitors_section(deps: &MonitorDeps) -> gtk::Box {
    detect_monitors(&deps.monitors);

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

    let mirrors = monitors_row(deps);
    mirrors.set_halign(gtk::Align::Start);
    section.append(&mirrors);

    section
}

/// Detecta los monitores del sistema (GDK) y actualiza el conjunto lógico.
fn detect_monitors(monitors: &std::rc::Rc<std::cell::RefCell<MonitorSet>>) {
    let mut found: Vec<(gtk::gdk::Rectangle, String)> = Vec::new();
    let display = match gtk::gdk::Display::default() {
        Some(d) => d,
        None => {
            crate::reporting::report(
                crate::reporting::ErrorKind::Monitors,
                "No se pudo acceder a la pantalla (GDK) para detectar monitores",
            );
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
pub fn monitors_row(deps: &MonitorDeps) -> gtk::Box {
    let row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    row.set_halign(gtk::Align::Start);

    let set = deps.monitors.borrow();
    if set.is_empty() {
        let label = gtk::Label::new(Some("No se detectó ningún monitor."));
        label.set_halign(gtk::Align::Start);
        row.append(&label);
        return row;
    }
    for mon in set.iter() {
        row.append(&monitor_card(mon, deps));
    }
    row
}

/// Tarjeta seleccionable de un monitor.
pub fn monitor_card(mon: &Monitor, deps: &MonitorDeps) -> gtk::ToggleButton {
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
        let monitors = deps.monitors.clone();
        let mirror = deps.mirror.clone();
        let player = deps.player.clone();
        button.connect_toggled(move |_| {
            let _ = monitors.borrow_mut().toggle(&id);
            let st = AppState {
                player: player.clone(),
                monitors: monitors.clone(),
                mirror: mirror.clone(),
            };
            crate::events::mirror_reconcile(&st);
        });
    }
    button
}