/*! Identification of the screens (monitor overlays).
 *
 * This module is isolated: it only shows a temporary identifier on each
 * secondary monitor and has no relation to playback or mirrors. When `show_all`
 * is called, every secondary monitor displays its identifier in the top-right
 * corner for a few seconds and then the overlays close by themselves. The
 * primary monitor (where the interface lives) is never covered.
 */

use std::sync::OnceLock;

use gtk::prelude::*;

use libadwaita as adw;

use crate::constants::monitors;

/** Shows, on every secondary monitor, its identifier in the top-right corner. */
pub fn show_all(application: &adw::Application) {
    let Some(display) = gtk::gdk::Display::default() else {
        crate::reporting::report(
            crate::reporting::ErrorKind::Monitors,
            "No se pudo acceder a la pantalla (GDK) para identificar los monitores",
        );
        return;
    };

    let monitors: Vec<gtk::gdk::Monitor> = display
        .monitors()
        .iter::<gtk::gdk::Monitor>()
        .filter_map(Result::ok)
        .collect();

    // El principal es el que contiene el origen (0,0); nunca se tapa ni se
    // etiqueta, para no cubrir la interfaz donde vive la app.
    let primary = monitors
        .iter()
        .position(|g| g.geometry().x() == 0 && g.geometry().y() == 0)
        .unwrap_or(monitors::DEFAULT_PRIMARY_INDEX);

    for (i, item) in monitors.into_iter().enumerate() {
        if i != primary {
            show_on(application, &item, i);
        }
    }
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
        let css = format!(
            ".{} {{\n  font-size: {}pt;\n  font-weight: bold;\n\
             \x20 color: {};\n  background-color: {};\n\
             \x20 border-radius: {}px;\n  padding: {}px;\n}}",
            monitors::identify::CSS_LABEL,
            monitors::identify::FONT_SIZE,
            monitors::identify::FG_COLOR,
            monitors::identify::BG_COLOR,
            monitors::identify::RADIUS,
            monitors::identify::PADDING,
        );
        let _ = provider.load_from_string(&css);
        gtk::style_context_add_provider_for_display(
            &display,
            &provider,
            gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
        );
    });
}
