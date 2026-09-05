/*
 * Mirror playback to additional monitors.
 *
 * libmpv only allows **one** `mpv_render_context` per core (render.h), so the
 * output of a single core cannot be duplicated to multiple surfaces. The
 * strategy (same as Syncplay/mpvsync/mpvpaper) is: **one mpv core per
 * selected monitor**, each with its own `mpv_render_context` embedded in a
 * `GtkGLArea` of a fullscreen window, all playing the SAME file and
 * synchronized (pause / seek) with the main player.
 *
 * Synchronization is time-based (not frame-perfect): all start together,
 * play at the same rate, and follow the position jumps of the master.
 */

use std::collections::HashMap;
use std::sync::mpsc::{Receiver, Sender};

use gtk::prelude::*;

use libadwaita as adw;

use crate::logging;
use crate::player::embed::EmbeddedVideo;
use crate::player::ffi;
use crate::reporting::{self, ErrorKind};

use crate::constants::mirror;
use crate::constants::monitors;
use crate::constants::mpv::*;

/** Commands the UI sends to a mirror's synchronized core. */
#[derive(Debug, Clone, PartialEq)]
pub enum MirrorCmd {
    /** Loads the file (and seeks to `pos` if `Some`). */
    Load(String, Option<f64>),
    Play,
    Pause,
    /** Seeks to a position (seconds). */
    Seek(f64),
    /** Terminates the core. */
    Shutdown,
}

/**
 * Mirror mpv core: lives on its own thread, without audio (`audio=no`),
 * with embedded output (`vo=libmpv`).
 */
struct MirrorCore {
    tx: Sender<MirrorCmd>,
    /** Raw handle for creating the `mpv_render_context` on the UI thread. */
    handle: ffi::mpv_handle,
}

impl MirrorCore {
    fn spawn() -> Option<Self> {
        let (cmd_tx, cmd_rx) = std::sync::mpsc::channel::<MirrorCmd>();
        // Canal por el que el hilo devuelve su handle una vez creado (usize).
        let (handle_tx, handle_rx) = std::sync::mpsc::channel::<usize>();
        if std::thread::Builder::new()
            .name(mirror::THREAD_NAME.into())
            .spawn(move || run_mirror(cmd_rx, handle_tx))
            .is_err()
        {
            reporting::report(ErrorKind::Mirror, mirror::messages::THREAD_CREATE_FAIL);
            return None;
        }
        let handle = match handle_rx.recv() {
            Ok(h) => h as ffi::mpv_handle,
            Err(_) => {
                reporting::report(ErrorKind::Mirror, mirror::messages::TERMINATED_EARLY);
                return None;
            }
        };
        Some(Self { tx: cmd_tx, handle })
    }

    fn send(&self, cmd: MirrorCmd) {
        if let Err(err) = self.tx.send(cmd) {
            reporting::report(
                ErrorKind::Mirror,
                format!("{}{err}", mirror::messages::SEND_FAIL),
            );
        }
    }
}

/** Mirror core thread: owns the single mpv instance for the mirror. */
fn run_mirror(rx: Receiver<MirrorCmd>, handle_tx: Sender<usize>) {
    // libmpv exige LC_NUMERIC en "C" (evita MPV_ERROR_NOMEM).
    crate::player::ffi::ensure_lc_numeric_c();

    let mut handler = match mpv::MpvHandlerBuilder::new().and_then(|mut b| {
        b.set_option(OPT_VO, VALUE_VO_LIBMPV)?;
        b.set_option(OPT_AUDIO, VALUE_NO)?;
        b.set_option(OPT_KEEP_OPEN, VALUE_YES)?;
        // Config y scripts del usuario desactivados: comportamiento
        // determinista y sin scripts/ytdl/IPC inyectados desde el sistema.
        b.set_option(OPT_CONFIG, VALUE_NO)?;
        b.set_option(OPT_LOAD_SCRIPTS, VALUE_NO)?;
        // Aceleración por hardware (estilo VLC): si hay GPU dedicada o
        // integrada se decodifica por hardware, sin depender del códec.
        crate::hwaccel::apply_to(&mut b)?;
        b.build()
    }) {
        Ok(h) => h,
        Err(err) => {
            logging::error(format!("{}{err}", mirror::messages::CORE_CREATE_FAIL));
            reporting::report(
                ErrorKind::Mirror,
                format!("{}{err}", mirror::messages::CORE_INIT_FAIL),
            );
            return;
        }
    };
    let _ = handle_tx.send(handler.raw() as usize);
    logging::info(mirror::logs::CORE_CREATED);

    // Posición pendiente de aplicar cuando el archivo termine de cargarse.
    // libmpv ignora un `seek` emitido antes de que el archivo esté cargado, así
    // que el salto se difiere al evento `FileLoaded` (evita arrancar desde 0
    // cuando se abre un espejo a mitad de reproducción).
    let mut pending_seek: Option<f64> = None;

    loop {
        // Bucle de eventos de mpv (timeout 0 => no bloqueante). Drena la cola
        // de eventos y detecta cuándo el archivo está listo para saltar.
        let mut busy = false;
        while let Some(ev) = handler.wait_event(mirror::EVENT_POLL_TIMEOUT_SECS) {
            busy = true;
            if let mpv::Event::FileLoaded = ev {
                if let Some(p) = pending_seek.take() {
                    let arg = format!("{p}");
                    let _ = handler.command(&[CMD_SEEK, &arg, SEEK_MODE_ABSOLUTE]);
                    logging::info(format!(
                        "{}{p}{}",
                        mirror::logs::LOAD_SEEK_PREFIX,
                        mirror::LOG_SEEK_SUFFIX
                    ));
                }
                let _ = handler.set_property(PROP_PAUSE, false);
            }
        }

        // Procesa los comandos de la UI (no bloqueante).
        match rx.try_recv() {
            Ok(MirrorCmd::Load(path, pos)) => {
                logging::info(format!("{}{path}", mirror::logs::LOADING_PREFIX));
                // Pausa antes de cargar para no arrancar desde 0; el `Play` y
                // el `seek` real se aplican en `FileLoaded`.
                let _ = handler.set_property(PROP_PAUSE, true);
                let _ = handler.command(&[CMD_LOADFILE, &path]);
                pending_seek = pos;
            }
            Ok(MirrorCmd::Play) => {
                let _ = handler.set_property(PROP_PAUSE, false);
            }
            Ok(MirrorCmd::Pause) => {
                let _ = handler.set_property(PROP_PAUSE, true);
            }
            Ok(MirrorCmd::Seek(p)) => {
                let arg = format!("{p}");
                let _ = handler.command(&[CMD_SEEK, &arg, SEEK_MODE_ABSOLUTE]);
            }
            Ok(MirrorCmd::Shutdown) => break,
            Err(std::sync::mpsc::TryRecvError::Disconnected) => break,
            Err(std::sync::mpsc::TryRecvError::Empty) => {}
        }

        // Pequeña pausa para no saturar la CPU cuando no hay actividad.
        if !busy {
            std::thread::sleep(std::time::Duration::from_millis(mirror::IDLE_SLEEP_MS));
        }
    }
    logging::info(mirror::logs::CORE_ENDED);
}

/** A mirror window: fullscreen over a monitor, with its GLArea. */
struct MirrorWindow {
    window: gtk::ApplicationWindow,
    core: MirrorCore,
}

impl MirrorWindow {
    /**
     * Opens a fullscreen window on the given `monitor` for the specified mirror.
     *
     * `monitor` is already resolved to its real `gdk::Monitor` by the controller.
     * Returns `None` if the mirror's mpv core could not be created.
     */
    fn open(id: &str, monitor: &gtk::gdk::Monitor, application: &adw::Application) -> Option<Self> {
        let core = MirrorCore::spawn()?;
        if core.handle.is_null() {
            reporting::report(
                ErrorKind::Mirror,
                format!(
                    "{}{id}{}",
                    mirror::messages::NO_HANDLE_PREFIX,
                    mirror::messages::NO_HANDLE_SUFFIX
                ),
            );
            return None;
        }
        let video = EmbeddedVideo::with_handle(core.handle);
        // The mirror shuts down when its window is destroyed, only AFTER
        // `unrealize` frees the `mpv_render_context`. libmpv requires
        // `mpv_render_context_free` to precede the `mpv_handle` destruction;
        // destroying the handle with a live render context triggers the
        // `queue_dtor` `assert` in `dispatch.c` when closing the app.
        {
            let shutdown_tx = core.tx.clone();
            video.widget().connect_unrealize(move |_| {
                let _ = shutdown_tx.send(MirrorCmd::Shutdown);
            });
        }

        let window = gtk::ApplicationWindow::builder()
            .application(application)
            .decorated(false)
            .build();
        window.set_child(Some(video.widget()));
        window.fullscreen_on_monitor(monitor);
        window.present();

        Some(Self { window, core })
    }

    fn close(&self) {
        self.window.close();
    }
}

/**
 * Manages all mirrors: reconciles monitor selection with open windows and
 * (re)sends the playback command to those that request it.
 */
pub struct MirrorController {
    application: adw::Application,
    /** Monitor id -> open mirror. */
    windows: HashMap<String, MirrorWindow>,
    /** Last file played in the main player. */
    current_path: Option<String>,
    /** Path already loaded in the mirrors (to detect video changes). */
    loaded_path: Option<String>,
}

impl MirrorController {
    pub fn new(application: &adw::Application) -> Self {
        Self {
            application: application.clone(),
            windows: HashMap::new(),
            current_path: None,
            loaded_path: None,
        }
    }

    /**
     * Closes all mirrors.
     *
     * Each mirror thread shuts down through the `unrealize` of its GLArea (which
     * already freed the `mpv_render_context`); here we only close the windows to
     * guarantee that order before destroying the `mpv_handle`.
     */
    pub fn clear(&mut self) {
        for (_, w) in self.windows.drain() {
            w.close();
        }
    }

    /** Closes the mirror for a specific monitor (e.g. when deselected). */
    pub fn remove(&mut self, id: &str) {
        if let Some(w) = self.windows.remove(id) {
            w.close();
        }
    }

    /**
     * Opens/closes mirrors so they match the `selected` monitors
     * (logical ids `gdk-{i}`), respecting the selection and current state.
     *
     * `pos_base`: position (seconds) at which to align a mirror that opens
     * mid-playback.
     */
    pub fn reconfigure(&mut self, selected: &[String], pos_base: Option<f64>) {
        let current_path = self.current_path.clone();

        // Cerrar los que ya no están seleccionados.
        let stale: Vec<String> = self
            .windows
            .keys()
            .filter(|id| !selected.contains(id))
            .cloned()
            .collect();
        for id in stale {
            self.remove(&id);
        }

        // Si no hay reproducción activa, no abrimos ventanas nuevas.
        let Some(path) = current_path else {
            return;
        };

        // Detectar si el video cambió (path diferente al ya cargado en espejos).
        let video_changed = self.path_changed();

        // Cargar el video en espejos nuevos, y en TODOS si el video cambió.
        for id in selected {
            if let Some(w) = self.windows.get(id) {
                if video_changed {
                    // Video cambió: recargar en TODOS los espejos existentes.
                    w.core.send(MirrorCmd::Load(path.clone(), pos_base));
                } else {
                    // Sin cambio: ya está sincronizado via control commands.
                    continue;
                }
            } else if let Some(monitor) = self.resolve_monitor(id) {
                // Espejo nuevo: abrir y cargar en la posición actual del maestro.
                let Some(w) = MirrorWindow::open(id, &monitor, &self.application) else {
                    reporting::report(
                        ErrorKind::Mirror,
                        format!("No se pudo abrir el espejo del monitor {id}"),
                    );
                    continue;
                };
                self.windows.insert(id.clone(), w);
                self.windows[id]
                    .core
                    .send(MirrorCmd::Load(path.clone(), pos_base));
            }
        }

        // Marcar path como cargado en espejos.
        self.mark_path_loaded();

        // Iniciar reproducción en todos.
        for w in self.windows.values() {
            w.core.send(MirrorCmd::Play);
        }
    }

    /** Notifies the main player that `path` will be played. */
    pub fn set_playing(&mut self, path: String) {
        self.current_path = Some(path);
    }

    /** Indicates whether the current path differs from the last one loaded in mirrors. */
    fn path_changed(&self) -> bool {
        self.current_path.as_ref() != self.loaded_path.as_ref()
    }

    /** Marks the path as already loaded in the mirrors. */
    fn mark_path_loaded(&mut self) {
        self.loaded_path = self.current_path.clone();
    }

    /** `true` while there is no active playback (no file loaded). */
    pub fn is_idle(&self) -> bool {
        self.current_path.is_none()
    }

    /** Synchronizes control (play/pause/seek) with the master. */
    pub fn control(&mut self, cmd: MirrorCmd) {
        for (_, w) in self.windows.iter_mut() {
            w.core.send(cmd.clone());
        }
    }

    /**
     * Fully resets the state: closes all mirrors and forgets the current
     * video, so `is_idle()` returns `true` again and no mirrors are reopened
     * when reconciling with an already empty list.
     */
    pub fn reset(&mut self) {
        self.clear();
        self.current_path = None;
        self.loaded_path = None;
    }

    /** Resolves the real `gdk::Monitor` from the logical id `gdk-{i}`. */
    fn resolve_monitor(&self, id: &str) -> Option<gtk::gdk::Monitor> {
        let idx = id
            .strip_prefix(monitors::ID_PREFIX)?
            .parse::<usize>()
            .ok()?;
        let display = gtk::gdk::Display::default()?;
        let monitors: Vec<gtk::gdk::Monitor> = display
            .monitors()
            .iter::<gtk::gdk::Monitor>()
            .filter_map(Result::ok)
            .collect();
        monitors.into_iter().nth(idx)
    }
}

/** Reads the current position of the main player (`time-pos`, seconds). */
pub fn main_time_pos() -> Option<f64> {
    let handle = crate::player::mpv_engine::mpv_handle()?;
    let name = b"time-pos\0";
    let mut value: f64 = 0.0;
    let rc = unsafe {
        ffi::mpv_get_property(
            handle,
            name.as_ptr() as *const std::os::raw::c_char,
            ffi::MPV_FORMAT_DOUBLE,
            (&mut value as *mut f64).cast(),
        )
    };
    if rc < 0 {
        None
    } else {
        Some(value)
    }
}
