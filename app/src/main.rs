//! Punto de entrada de la aplicación.
//!
//! Responsabilidades:
//! 1. Verificar el requisito obligatorio de Wayland (bajo X11 se informa y se
//!    sale de forma segura).
//! 2. Inicializar GTK/libadwaita.
//! 3. Delegar la construcción de la interfaz a `app`.

mod app;
mod logging;
mod playback;
mod player;
mod wayland;

use gtk::glib;

use gtk::prelude::*;

use libadwaita as adw;

const APPLICATION_ID: &str = "io.github.davichus1303.MoviesOnOtherScreens";

/// Categoría de locale `LC_NUMERIC` (definición POSIX).
const LC_NUMERIC: i32 = 1;

unsafe extern "C" {
    fn setlocale(category: i32, locale: *const u8) -> *mut u8;
}

fn main() -> glib::ExitCode {
    logging::info(format!(
        "Iniciando Movies on Other Screens (PID {})",
        std::process::id()
    ));

    if !require_wayland() {
        logging::warn("Entorno gráfico no compatible con Wayland; la app sale.");
        return show_requirement_message_and_exit();
    }
    logging::info("Entorno Wayland detectado.");

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

/// libmpv exige `LC_NUMERIC` en `"C"`; si no, `mpv_create()` falla con
/// `MPV_ERROR_NOMEM` (mensaje engañoso, véase documentación del crate `mpv`).
/// En una sesión con locale no-C hay que ajustarlo antes de inicializar mpv.
fn init_locale_for_mpv() {
    unsafe {
        setlocale(LC_NUMERIC, b"C\0".as_ptr());
    }
}

/// Devuelve `true` si debe continuar la ejecución (entorno Wayland).
fn require_wayland() -> bool {
    matches!(
        wayland::detect_backend(),
        wayland::GraphicsBackend::Wayland
    )
}

/// Muestra el mensaje de requisito y termina con un código de salida claro.
fn show_requirement_message_and_exit() -> glib::ExitCode {
    eprintln!("{}", wayland::REQUIREMENT_MESSAGE);
    glib::ExitCode::from(1)
}
