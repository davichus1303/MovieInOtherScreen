/*! Identification of the screens (monitor overlays).
 *
 * This module is isolated: it only shows a temporary identifier on each
 * secondary monitor and has no relation to playback or mirrors. The set of
 * monitors to label comes from the shared `MonitorSet` (single source of
 * truth), so the numbers shown match the selection cards exactly. When
 * `show_all` is called, every secondary monitor displays its identifier in
 * the top-right corner for a few seconds and then the overlays close by
 * themselves. The primary monitor (where the interface lives) is never
 * covered because the domain never marks it as a destination.
 */

use std::rc::Rc;
use std::sync::OnceLock;

use gtk::prelude::*;

use libadwaita as adw;

use mos_core::monitors::MonitorSet;

use crate::constants::monitors;

/** Shows, on every secondary monitor of `set`, its identifier in the top-right corner. */
pub fn show_all(application: &adw::Application, set: &Rc<std::cell::RefCell<MonitorSet>>) {
    for mon in set.borrow().secondaries() {
        let index = mon
            .id()
            .strip_prefix(monitors::ID_PREFIX)
            .and_then(|s| s.parse::<usize>().ok())
            .unwrap_or_default();
        if let Some(display_monitor) = resolve_display_monitor(mon.id()) {
            show_on(application, &display_monitor, index);
        }
    }
}

/** Resolves the real `gdk::Monitor` from a logical id `gdk-{i}`. */
fn resolve_display_monitor(id: &str) -> Option<gtk::gdk::Monitor> {
    let idx = id
        .strip_prefix(monitors::ID_PREFIX)?
        .parse::<usize>()
        .ok()?;
    let display = gtk::gdk::Display::default()?;
    display
        .monitors()
        .iter::<gtk::gdk::Monitor>()
        .filter_map(Result::ok)
        .nth(idx)
}

/** Shows one identifier badge (its index) on `monitor` for a limited time. */
fn show_on(application: &adw::Application, monitor: &gtk::gdk::Monitor, index: usize) {
    register_style();

    let badge = gtk::Label::new(Some(&index.to_string()));
    badge.set_halign(gtk::Align::End);
    badge.set_valign(gtk::Align::Start);
    badge.set_margin_top(monitors::identify::MARGIN_TOP);
    badge.set_margin_end(monitors::identify::MARGIN_END);
    badge.add_css_class(monitors::identify::CSS_LABEL);

    let window = gtk::ApplicationWindow::builder()
        .application(application)
        .decorated(false)
        .build();
    window.set_child(Some(&badge));
    window.fullscreen_on_monitor(monitor);
    window.present();

    let win = window.clone();
    gtk::glib::timeout_add_local(
        std::time::Duration::from_millis(monitors::identify::DURATION_MS as u64),
        move || {
            win.close();
            gtk::glib::ControlFlow::Break
        },
    );
}

/** Registers, once, the CSS that styles the identifier badge. */
fn register_style() {
    static INIT: OnceLock<()> = OnceLock::new();
    let Some(display) = gtk::gdk::Display::default() else {
        return;
    };
    INIT.get_or_init(|| {
        let provider = gtk::CssProvider::new();
        // CSS externo (identify.css), enrutado en tiempo de compilación.
        let _ = provider.load_from_string(include_str!("identify.css"));
        gtk::style_context_add_provider_for_display(
            &display,
            &provider,
            gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
        );
    });
}
