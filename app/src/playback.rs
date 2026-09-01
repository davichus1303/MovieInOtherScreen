//! Reproducción reutilizable de un vídeo de la lista.
//!
//! Centraliza la única vía para pedir al reproductor que reproduzca un vídeo
//! de la lista: [`play_index`][`play_from_list`] resuelve el vídeo por su
//! índice y manda `PlayerCommand::Load(path)` al hilo del motor. La salida
//! visual la pinta de forma independiente el motor mpv embebido
//! ([`crate::player::embed`]) en el área de reproducción de la app.
//!
//! Esta capa **no** contiene lógica de reproducción: solo traduce una
//! selección de la UI en el comando de carga correspondiente, y registra en
//! el log el resultado (inicio o error) para poder diagnosticar fallos.
//!
//! Está pensada para reutilizarse desde cualquier punto de la interfaz (doble
//! clic en la lista, botón "Reproducir", arrastrar y soltar, etc.); cada
//! llamada converge en el mismo `PlayerCommand::Load`.

use std::cell::RefCell;
use std::rc::Rc;

use gtk::prelude::*;

use mos_core::video_list::VideoList;

use crate::player::PlayerCommand;

/// Registra las marcas de éxito/error de la reproducción en el archivo de log.
mod playback_log {
    use crate::logging;

    /// Mensaje de éxito tras encolar la carga del vídeo.
    pub(super) fn started(path: &str) -> bool {
        logging::info(format!("Reproducción encolada: {path}"))
    }

    /// Mensaje cuando la lista no contiene el índice solicitado.
    pub(super) fn missing_index(index: usize) -> bool {
        logging::warn(format!("Índice fuera de rango al reproducir: {index}"))
    }

    /// Mensaje cuando el canal hacia el motor no acepta el comando (motor
    /// caído o cerrado).
    pub(super) fn send_failed(path: &str) -> bool {
        logging::error(format!(
            "No se pudo enviar la orden de carga al motor mpv: {path}"
        ))
    }

    /// Mensaje cuando no hay selección activa (solo aplica a variantes que la
    /// requieren, como el botón "Reproducir").
    pub(super) fn no_selection() -> bool {
        logging::warn("Reproducir solicitado sin vídeo seleccionado")
    }
}

/// Carga en el reproductor el vídeo de la lista con índice `index`.
///
/// Es la función central y reutilizable: no toca nada de la UI salvo leer del
/// estado compartido. Devuelve `true` si se envió el comando de carga al
/// motor. Registra en el log el inicio o el error según el resultado.
pub fn play_from_list(
    videos: &Rc<RefCell<VideoList>>,
    index: usize,
    player: &std::sync::mpsc::Sender<PlayerCommand>,
) -> bool {
    let path = {
        let borrowed = videos.borrow();
        match borrowed.get(index) {
            Some(v) => v.path().to_string_lossy().into_owned(),
            None => {
                playback_log::missing_index(index);
                return false;
            }
        }
    };

    let sent = player.send(PlayerCommand::Load(path.clone())).is_ok();
    if sent {
        playback_log::started(&path);
    } else {
        playback_log::send_failed(&path);
    }
    sent
}

/// Reproduce el vídeo actualmente seleccionado en la lista.
///
/// Variante reutilizable para flujos basados en selección (p. ej. el botón
/// "Reproducir"). Devuelve `true` si había selección y se envió el comando.
pub fn play_selected(
    videos: &Rc<RefCell<VideoList>>,
    player: &std::sync::mpsc::Sender<PlayerCommand>,
) -> bool {
    match videos.borrow().selected_index() {
        Some(i) => play_from_list(videos, i, player),
        None => {
            playback_log::no_selection();
            false
        }
    }
}

/// Conecta la reproducción al doble clic (o Enter) sobre una fila de la lista.
///
/// Es el comportamiento actualmente activo: reproducir pulsando un vídeo de
/// la lista lateral. Reutiliza [`play_from_list`].
pub fn connect_double_click(
    list: &gtk::ListBox,
    videos: &Rc<RefCell<VideoList>>,
    player: &std::sync::mpsc::Sender<PlayerCommand>,
) {
    let videos = videos.clone();
    let player = player.clone();
    list.connect_row_activated(move |_list, row| {
        play_from_list(&videos, row.index() as usize, &player);
    });
}

/// Conecta un botón para lanzar el vídeo seleccionado en la lista.
///
/// Toma la selección activa (la fila subrayada) cada vez que se pulsa el
/// botón. Reutiliza [`play_selected`].
pub fn connect_play_button(
    button: &gtk::Button,
    videos: &Rc<RefCell<VideoList>>,
    player: &std::sync::mpsc::Sender<PlayerCommand>,
) {
    let videos = videos.clone();
    let player = player.clone();
    button.connect_clicked(move |_| {
        play_selected(&videos, &player);
    });
}
