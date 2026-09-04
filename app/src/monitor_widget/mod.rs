/*! Widgets for monitor selection. */

use gtk::prelude::*;

use libadwaita as adw;

use crate::constants::monitors;
use crate::events::AppState;
use crate::mirror::MirrorController;
use mos_core::monitors::{Monitor, MonitorKind, MonitorSet};

/** State needed to build monitor widgets. */
pub struct MonitorDeps {
    pub mirror: std::rc::Rc<std::cell::RefCell<MirrorController>>,
    pub monitors: std::rc::Rc<std::cell::RefCell<MonitorSet>>,
    pub application: adw::Application,
}

/** Builds the complete monitors section (title + hint + cards + actions). */
pub fn build_monitors_section(deps: &MonitorDeps) -> gtk::Box {
    detect_monitors(&deps.monitors);

    let section = gtk::Box::new(
        gtk::Orientation::Vertical,
        monitors::layout::SECTION_SPACING,
    );
    section.set_margin_top(monitors::layout::MARGIN_TOP);
    section.set_margin_bottom(monitors::layout::MARGIN_BOTTOM);
    section.set_margin_start(monitors::layout::MARGIN_START);
    section.set_margin_end(monitors::layout::MARGIN_END);

    let title = gtk::Label::new(Some(monitors::LABEL_SECTION_TITLE));
    title.set_halign(gtk::Align::Start);
    title.add_css_class("title-4");
    section.append(&title);

    let hint = gtk::Label::new(Some(
        "Selecciona uno o varios monitores para reproducir en pantalla completa. \
         El principal queda fijo.",
    ));
    hint.set_halign(gtk::Align::Start);
    hint.set_wrap(true);
    hint.set_max_width_chars(monitors::layout::HINT_MAX_WIDTH_CHARS);
    section.append(&hint);

    // Selection cards (rebuilt when the principal monitor changes).
    let cards_box = gtk::Box::new(
        gtk::Orientation::Vertical,
        monitors::layout::CARD_ROW_SPACING,
    );
    cards_box.set_halign(gtk::Align::Start);
    refresh_cards(&cards_box, deps);
    section.append(&cards_box);

    // Actions row: principal selector + identify screens.
    let actions = gtk::Box::new(
        gtk::Orientation::Horizontal,
        monitors::layout::ACTIONS_ROW_SPACING,
    );
    actions.set_halign(gtk::Align::Start);
    actions.set_valign(gtk::Align::Center);
    actions.append(&primary_selector(deps, &cards_box));
    actions.append(&identify_button(deps));
    section.append(&actions);

    section
}

/** Rebuilds the cards inside `cards_box` from the current logical set. */
pub fn refresh_cards(cards_box: &gtk::Box, deps: &MonitorDeps) {
    while let Some(child) = cards_box.first_child() {
        cards_box.remove(&child);
    }
    let row = monitors_row(deps);
    row.set_halign(gtk::Align::Start);
    cards_box.append(&row);
}

/** Dropdown listing every monitor; picking one makes it the principal. */
fn primary_selector(deps: &MonitorDeps, cards_box: &gtk::Box) -> gtk::DropDown {
    let set = deps.monitors.borrow();
    let items: Vec<String> = set
        .iter()
        .map(|m| {
            let kind = if m.is_primary() {
                monitors::LABEL_MONITOR_PRIMARY
            } else {
                monitors::LABEL_MONITOR_SECONDARY
            };
            monitors::PRIMARY_SELECTOR_FORMAT
                .replace("{kind}", kind)
                .replace("{label}", m.label())
        })
        .collect();

    if items.is_empty() {
        drop(set);
        let model = gtk::StringList::new(&[]);
        let dropdown = gtk::DropDown::new(Some(model), None::<gtk::Expression>);
        dropdown.set_sensitive(false);
        return dropdown;
    }

    let entries: Vec<&str> = items.iter().map(String::as_str).collect();
    let model = gtk::StringList::new(&entries);
    let dropdown = gtk::DropDown::new(Some(model), None::<gtk::Expression>);
    dropdown.set_halign(gtk::Align::Start);
    if let Some((idx, _)) = set.iter().enumerate().find(|(_, m)| m.is_primary()) {
        dropdown.set_selected(idx as u32);
    }
    drop(set);

    let monitor_label = monitors::LABEL_PRIMARY_SELECTOR;
    dropdown.set_tooltip_text(Some(monitor_label));

    let monitors = deps.monitors.clone();
    let mirror = deps.mirror.clone();
    let application = deps.application.clone();
    let cards = cards_box.clone();
    dropdown.connect_selected_notify(move |dd| {
        let pos = dd.selected();
        if pos == gtk::INVALID_LIST_POSITION {
            return;
        }
        let id = format!("{}{}", monitors::ID_PREFIX, pos);
        {
            let mut s = monitors.borrow_mut();
            if !s.set_primary(&id) {
                return;
            }
        }
        let owned = MonitorDeps {
            mirror: mirror.clone(),
            monitors: monitors.clone(),
            application: application.clone(),
        };
        refresh_cards(&cards, &owned);
        let st = AppState {
            monitors: monitors.clone(),
            mirror: mirror.clone(),
        };
        crate::events::mirror_reconcile(&st);
    });
    dropdown
}

/** Button that identifies every secondary screen with a temporary badge. */
fn identify_button(deps: &MonitorDeps) -> gtk::Button {
    let identify = gtk::Button::with_label(monitors::LABEL_IDENTIFY_BUTTON);
    identify.set_halign(gtk::Align::Start);
    let application = deps.application.clone();
    let monitor_set = deps.monitors.clone();
    identify.connect_clicked(move |_| {
        crate::identify::show_all(&application, &monitor_set);
    });
    identify
}

/** Detects the system monitors (GDK) and updates the logical set. */
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
    for item in display
        .monitors()
        .iter::<gtk::gdk::Monitor>()
        .filter_map(Result::ok)
    {
        let label = item
            .model()
            .map(|s| s.to_string())
            .unwrap_or_else(|| monitors::LABEL_MONITOR_DEFAULT.to_string());
        found.push((item.geometry(), label));
    }

    // El principal es el que contiene el origen (0,0); si no hay ninguno,
    // se toma el primero.
    let primary_index = found
        .iter()
        .position(|(g, _)| g.x() == 0 && g.y() == 0)
        .unwrap_or(monitors::DEFAULT_PRIMARY_INDEX);

    let detected: Vec<Monitor> = found
        .into_iter()
        .enumerate()
        .map(|(i, (_g, label))| {
            let kind = if i == primary_index {
                MonitorKind::Primary
            } else {
                MonitorKind::Secondary
            };
            Monitor::new(format!("{}{}", monitors::ID_PREFIX, i), label, kind)
        })
        .collect();

    monitors.borrow_mut().update_from_detected(detected);
}

/** Horizontal row with one card per monitor. */
pub fn monitors_row(deps: &MonitorDeps) -> gtk::Box {
    let row = gtk::Box::new(
        gtk::Orientation::Horizontal,
        monitors::layout::CARD_ROW_SPACING,
    );
    row.set_halign(gtk::Align::Start);

    let set = deps.monitors.borrow();
    if set.is_empty() {
        let label = gtk::Label::new(Some(monitors::LABEL_NO_MONITORS));
        label.set_halign(gtk::Align::Start);
        row.append(&label);
        return row;
    }
    for mon in set.iter() {
        row.append(&monitor_card(mon, deps));
    }
    row
}

/** Selectable card for a monitor. */
pub fn monitor_card(mon: &Monitor, deps: &MonitorDeps) -> gtk::ToggleButton {
    let kind = if mon.is_primary() {
        monitors::LABEL_MONITOR_PRIMARY
    } else {
        monitors::LABEL_MONITOR_SECONDARY
    };
    let id = mon.id().to_string();
    let id_num = id
        .strip_prefix(monitors::ID_PREFIX)
        .map(str::to_owned)
        .unwrap_or_else(|| id.clone());
    let label = mon.label().to_string();

    let button = gtk::ToggleButton::with_label(
        &monitors::CARD_FORMAT
            .replace("{id}", &id_num)
            .replace("{kind}", kind)
            .replace("{label}", &label),
    );
    button.set_size_request(monitors::layout::CARD_WIDTH, monitors::layout::CARD_HEIGHT);
    button.set_halign(gtk::Align::Start);
    button.add_css_class(monitors::CSS_CARD);
    button.set_active(mon.is_selected());

    if mon.is_primary() {
        button.set_sensitive(false);
    } else {
        let monitors = deps.monitors.clone();
        let mirror = deps.mirror.clone();
        button.connect_toggled(move |_| {
            let _ = monitors.borrow_mut().toggle(&id);
            let st = AppState {
                monitors: monitors.clone(),
                mirror: mirror.clone(),
            };
            crate::events::mirror_reconcile(&st);
        });
    }
    button
}
