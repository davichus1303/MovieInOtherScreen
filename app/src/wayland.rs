//! Comprobación del entorno gráfico obligatorio: Wayland.
//!
//! La aplicación solo es compatible con Wayland. Bajo X11 muestra un mensaje
//! claro y sale de forma segura, sin intentar una compatibilidad parcial.

/// Fuente de verdad sobre el backend gráfico.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GraphicsBackend {
    Wayland,
    X11,
    Unknown,
}

/// Infiere el backend a partir de las variables de entorno.
///
/// Preferimos `XDG_SESSION_TYPE` cuando está disponible y es explícito; en
/// caso de duda consultamos las variables propias de cada protocolo.
pub fn detect_backend() -> GraphicsBackend {
    if let Ok(session) = std::env::var("XDG_SESSION_TYPE") {
        if session.eq_ignore_ascii_case("wayland") {
            return GraphicsBackend::Wayland;
        }
        if session.eq_ignore_ascii_case("x11") {
            // Podría estar en una sesión X11 con Wayland disponible; se
            // resuelve con las variables de Wayland de forma prioritaria.
            return resolve_protocol_variables();
        }
    }
    resolve_protocol_variables()
}

fn resolve_protocol_variables() -> GraphicsBackend {
    let has_wayland = std::env::var("WAYLAND_DISPLAY").is_ok();
    let has_x11 = std::env::var("DISPLAY").is_ok();
    match (has_wayland, has_x11) {
        (true, _) => GraphicsBackend::Wayland,
        (false, true) => GraphicsBackend::X11,
        (false, false) => GraphicsBackend::Unknown,
    }
}

/// Mensaje que se muestra al usuario cuando el entorno no es compatible.
pub const REQUIREMENT_MESSAGE: &str = "\
Movies on Other Screens requiere Wayland para funcionar.

Se ha detectado que la aplicación se está ejecutando bajo X11 o en un \
entorno sin Wayland. No se ofrece compatibilidad parcial con X11.

Inicia una sesión de GNOME sobre Wayland y vuelve a intentarlo.";

#[cfg(test)]
mod tests {
    use super::*;

    fn with_env(vars: &[(&str, &str)]) {
        for (k, v) in vars {
            std::env::set_var(k, v);
        }
    }

    fn clean_env() {
        std::env::remove_var("XDG_SESSION_TYPE");
        std::env::remove_var("WAYLAND_DISPLAY");
        std::env::remove_var("DISPLAY");
    }

    #[test]
    fn sesion_wayland_detecta_wayland() {
        clean_env();
        with_env(&[("XDG_SESSION_TYPE", "wayland")]);
        assert_eq!(detect_backend(), GraphicsBackend::Wayland);
    }

    #[test]
    fn x11_sin_wayland_detecta_x11() {
        clean_env();
        with_env(&[("XDG_SESSION_TYPE", "x11"), ("DISPLAY", ":0")]);
        assert_eq!(detect_backend(), GraphicsBackend::X11);
    }

    #[test]
    fn x11_con_wayland_distinto_prefiere_wayland() {
        clean_env();
        with_env(&[
            ("XDG_SESSION_TYPE", "x11"),
            ("DISPLAY", ":0"),
            ("WAYLAND_DISPLAY", "wayland-0"),
        ]);
        assert_eq!(detect_backend(), GraphicsBackend::Wayland);
    }

    #[test]
    fn x11_con_wayland_pero_sin_display_detecta_wayland() {
        clean_env();
        with_env(&[("XDG_SESSION_TYPE", "x11"), ("WAYLAND_DISPLAY", "wayland-0")]);
        assert_eq!(detect_backend(), GraphicsBackend::Wayland);
    }

    #[test]
    fn solo_display_detecta_x11() {
        clean_env();
        with_env(&[("DISPLAY", ":0")]);
        assert_eq!(detect_backend(), GraphicsBackend::X11);
    }

    #[test]
    fn sin_variables_detecta_desconocido() {
        clean_env();
        assert_eq!(detect_backend(), GraphicsBackend::Unknown);
    }
}
