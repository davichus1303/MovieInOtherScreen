//! Registro y notificación de errores de la aplicación.
//!
//! Provee una única puerta por la que todos los errores/fallos del programa
//! llegan a la interfaz (toast) y a los logs. Centraliza el mapeo entre un
//! error interno y un mensaje legible para el usuario, de modo que las
//! funciones que detectan un fallo no tengan que decidir cómo mostrarlo o
//! loguearlo: solo llaman a [`report`].
//!
//! Los mensajes viajan al hilo principal de GTK a través de un canal global;
//! la UI registra un receptor (véase [`attach`]) que los muestra como toasts
//! emergentes (libadwaita) y, en paralelo, todo se escribe en los logs.

use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::OnceLock;

use gtk::glib;
use gtk::prelude::*;

use libadwaita as adw;

use crate::logging;

/// Categorías de error conocidas de la aplicación.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorKind {
    /// Fallo al crear/cerrar un espejo de monitor.
    Mirror,
    /// Error del motor de reproducción mpv.
    Player,
    /// Fallo al cargar/abrir un archivo de vídeo.
    Video,
    /// Fallo al detectar o seleccionar monitores.
    Monitors,
    /// Error de audio.
    Audio,
    /// Error interno/inesperado sin clasificar.
    Internal,
}

impl ErrorKind {
    /// Mensaje legible en castellano, mostrado al usuario en el toast.
    fn user_message(self, detail: &str) -> String {
        let base = match self {
            ErrorKind::Mirror => "Error con un monitor espejo".to_string(),
            ErrorKind::Player => "Error de reproducción".to_string(),
            ErrorKind::Video => "No se pudo cargar el vídeo".to_string(),
            ErrorKind::Monitors => "Error con los monitores".to_string(),
            ErrorKind::Audio => "Error de audio".to_string(),
            ErrorKind::Internal => "Error interno".to_string(),
        };
        if detail.trim().is_empty() {
            base
        } else {
            format!("{base}: {detail}")
        }
    }

    /// Etiqueta de categoría para el log.
    fn tag(self) -> &'static str {
        match self {
            ErrorKind::Mirror => "monitor",
            ErrorKind::Player => "player",
            ErrorKind::Video => "video",
            ErrorKind::Monitors => "monitors",
            ErrorKind::Audio => "audio",
            ErrorKind::Internal => "internal",
        }
    }
}

/// Una notificación de error lista para mostrarse en la interfaz.
#[derive(Debug, Clone)]
struct Report {
    kind: ErrorKind,
    detail: String,
}

/// Canal global por el que los errores llegan al hilo principal de la UI.
static REPORT_TX: OnceLock<Sender<Report>> = OnceLock::new();

/// Registra un error, lo escribe en los logs y notifica a la interfaz
/// (toast emergente) para que el usuario lo vea.
///
/// Esta es la función que llaman todas las fuentes de error de la aplicación.
pub fn report(kind: ErrorKind, detail: impl AsRef<str>) {
    let report = Report {
        kind,
        detail: detail.as_ref().to_string(),
    };

    let user_msg = report.kind.user_message(&report.detail);
    let log_msg = format!("[{}] {}", report.kind.tag(), report.detail);

    // Los errores siempre se registran en logs (diagnóstico).
    match report.kind {
        ErrorKind::Internal | ErrorKind::Player | ErrorKind::Video => {
            logging::error(&log_msg);
        }
        _ => {
            logging::warn(&log_msg);
        }
    }

    // Y se muestran en la interfaz si la UI ya está escuchando.
    if let Some(tx) = REPORT_TX.get() {
        if tx.send(report).is_err() {
            logging::warn("No se pudo notificar el error a la interfaz (canal cerrado)");
        }
    } else {
        logging::warn("Interfaz aún no registrada; error solo en logs");
    }
}

/// Conecta el receptor global en un bucle de la UI y muestra cada error como
/// toast sobre `overlay`. Se llama una vez al construir la ventana.
///
/// Devuelve el manejador de la fuente de timeout para mantenerla viva.
pub fn attach(overlay: &adw::ToastOverlay) -> glib::SourceId {
    let (tx, rx) = mpsc::channel::<Report>();
    // Si ya había un canal (no debería), lo ignoramos; usamos el primero.
    let _ = REPORT_TX.set(tx);

    let overlay = overlay.clone();
    glib::idle_add_local(move || drain_reports(&rx, &overlay))
}

/// Recolecta los errores pendientes del canal y los muestra como toasts.
/// Devuelve `Continue` para seguir escuchando mientras el canal esté vivo.
fn drain_reports(rx: &Receiver<Report>, overlay: &adw::ToastOverlay) -> glib::ControlFlow {
    while let Ok(report) = rx.try_recv() {
        let msg = report.kind.user_message(&report.detail);
        let toast = adw::Toast::new(&msg);
        toast.set_timeout(4);
        overlay.add_toast(toast);
    }
    glib::ControlFlow::Continue
}
