/*
 * Logging and notification of application errors.
 *
 * Provides a single door through which all errors/failures of the program
 * reach the interface (toast) and the logs. It centralizes the mapping between
 * an internal error and a readable message for the user, so functions that
 * detect a failure do not have to decide how to display or log it: they only
 * call [`report`].
 *
 * Messages travel to the main GTK thread through a global channel; the UI
 * registers a receiver (see [`attach`]) that shows them as popup toasts
 * (libadwaita) and, in parallel, everything is written to the logs.
 */

use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::OnceLock;

use gtk::glib;
use gtk::prelude::*;

use libadwaita as adw;

use crate::constants::reporting::{
    tags, user_messages, warnings, DETAIL_SEPARATOR, LOG_TAG_FORMAT, TOAST_TIMEOUT_SECS,
};
use crate::logging;

/** Known error categories of the application. */
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorKind {
    /** Failure creating/closing a monitor mirror. */
    Mirror,
    /** Error from the mpv playback engine. */
    Player,
    /** Failure loading/opening a video file. */
    Video,
    /** Failure detecting or selecting monitors. */
    Monitors,
    /** Audio error. */
    Audio,
    /** Internal/unexpected unclassified error. */
    Internal,
}

impl ErrorKind {
    /** Readable message shown to the user in the toast. */
    fn user_message(self, detail: &str) -> String {
        let base = match self {
            ErrorKind::Mirror => user_messages::MIRROR.to_string(),
            ErrorKind::Player => user_messages::PLAYER.to_string(),
            ErrorKind::Video => user_messages::VIDEO.to_string(),
            ErrorKind::Monitors => user_messages::MONITORS.to_string(),
            ErrorKind::Audio => user_messages::AUDIO.to_string(),
            ErrorKind::Internal => user_messages::INTERNAL.to_string(),
        };
        if detail.trim().is_empty() {
            base
        } else {
            format!("{base}{DETAIL_SEPARATOR}{detail}")
        }
    }

    /** Category tag for the log. */
    fn tag(self) -> &'static str {
        match self {
            ErrorKind::Mirror => tags::MIRROR,
            ErrorKind::Player => tags::PLAYER,
            ErrorKind::Video => tags::VIDEO,
            ErrorKind::Monitors => tags::MONITORS,
            ErrorKind::Audio => tags::AUDIO,
            ErrorKind::Internal => tags::INTERNAL,
        }
    }
}

/** An error notification ready to be shown in the interface. */
#[derive(Debug, Clone)]
struct Report {
    kind: ErrorKind,
    detail: String,
}

/** Global channel through which errors reach the UI main thread. */
static REPORT_TX: OnceLock<Sender<Report>> = OnceLock::new();

/**
 * Logs an error, writes it to the logs, and notifies the interface
 * (popup toast) so the user can see it.
 *
 * This is the function called by all error sources in the application.
 */
pub fn report(kind: ErrorKind, detail: impl AsRef<str>) {
    let report = Report {
        kind,
        detail: detail.as_ref().to_string(),
    };

    let user_msg = report.kind.user_message(&report.detail);
    let log_msg = LOG_TAG_FORMAT
        .replacen("{}", report.kind.tag(), 1)
        .replacen("{}", &report.detail, 1);

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
            logging::warn(warnings::CHANNEL_CLOSED);
        }
    } else {
        logging::warn(warnings::NOT_ATTACHED);
    }
}

/**
 * Connects the global receiver in a UI loop and shows each error as a
 * toast over `overlay`. Called once when building the window.
 *
 * Returns the handler of the timeout source to keep it alive.
 */
pub fn attach(overlay: &adw::ToastOverlay) -> glib::SourceId {
    let (tx, rx) = mpsc::channel::<Report>();
    // Si ya había un canal (no debería), lo ignoramos; usamos el primero.
    let _ = REPORT_TX.set(tx);

    let overlay = overlay.clone();
    glib::idle_add_local(move || drain_reports(&rx, &overlay))
}

/**
 * Collects the pending errors from the channel and shows them as toasts.
 * Returns `Continue` to keep listening while the channel is alive.
 */
fn drain_reports(rx: &Receiver<Report>, overlay: &adw::ToastOverlay) -> glib::ControlFlow {
    while let Ok(report) = rx.try_recv() {
        let msg = report.kind.user_message(&report.detail);
        let toast = adw::Toast::new(&msg);
        toast.set_timeout(TOAST_TIMEOUT_SECS);
        overlay.add_toast(toast);
    }
    glib::ControlFlow::Continue
}
