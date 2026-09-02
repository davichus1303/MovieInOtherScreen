/*
 * Application entry point.
 *
 * Responsibilities:
 * 1. Verify the mandatory Wayland requirement (under X11 it reports and
 *    exits safely).
 * 2. Initialize GTK/libadwaita.
 * 3. Delegate the construction of the interface to `app`.
 */

mod app;
mod audio;
mod constants;
mod engine;
mod events;
mod hwaccel;
mod logging;
mod mirror;
mod monitor_widget;
mod playback;
mod player;
mod player_area;
mod reporting;
mod sidebar;
mod wayland;

use gtk::glib;

use gtk::prelude::*;

use libadwaita as adw;

use crate::constants::{app::APPLICATION_ID, main_app};

unsafe extern "C" {
    fn setlocale(category: i32, locale: *const u8) -> *mut u8;
}

fn main() -> glib::ExitCode {
    install_panic_hook();
    logging::info(main_app::messages::LOG_STARTING.replace("{}", &std::process::id().to_string()));

    if !require_wayland() {
        logging::warn(main_app::messages::WARN_NON_WAYLAND);
        return show_requirement_message_and_exit();
    }
    logging::info(main_app::messages::LOG_WAYLAND_OK);

    init_locale_for_mpv();
    let application = adw::Application::builder()
        .application_id(APPLICATION_ID)
        .build();

    application.connect_activate(|application| {
        let window = app::build_main_window(application);
        window.present();
    });

    application.run()
}

/**
 * libmpv requires `LC_NUMERIC` to be `"C"`; otherwise `mpv_create()` fails
 * with `MPV_ERROR_NOMEM` (misleading message, see the `mpv` crate docs).
 * In a session with a non-C locale, it must be adjusted before initializing mpv.
 */
fn init_locale_for_mpv() {
    unsafe {
        setlocale(main_app::LC_NUMERIC, b"C\0".as_ptr());
    }
}

/** Returns `true` if execution should continue (Wayland environment). */
fn require_wayland() -> bool {
    matches!(wayland::detect_backend(), wayland::GraphicsBackend::Wayland)
}

/** Shows the requirement message and exits with a clear exit code. */
fn show_requirement_message_and_exit() -> glib::ExitCode {
    eprintln!("{}", wayland::REQUIREMENT_MESSAGE);
    glib::ExitCode::from(main_app::EXIT_CODE_REQUIREMENT)
}

/**
 * Installs a global panic handler that logs and notifies any unexpected
 * failure (e.g. the unexpected close when deselecting a monitor) in the logs
 * and in the interface, instead of the app closing silently.
 */
fn install_panic_hook() {
    let default_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        default_hook(info);
        let payload = info
            .payload()
            .downcast_ref::<&str>()
            .map(|s| s.to_string())
            .or_else(|| info.payload().downcast_ref::<String>().cloned())
            .unwrap_or_else(|| main_app::MSG_UNKNOWN_PANIC.to_string());
        crate::reporting::report(crate::reporting::ErrorKind::Internal, format!("{payload}"));
    }));
}
