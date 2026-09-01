//! Espejo de la reproducción hacia monitores adicionales.
//!
//! libmpv solo permite **un** `mpv_render_context` por core (render.h), por lo
//! que no se puede duplicar la salida de un único núcleo a varias superficies.
//! La estrategia (misma que usan Syncplay/mpvsync/mpvpaper) es: **un core de
//! mpv por monitor seleccionado**, cada uno con su propio `mpv_render_context`
//! embebido en un `GtkGLArea` de una ventana fullscreen, todos reproduciendo el
//! MISMO archivo y sincronizados (pausa / seek) con el reproductor principal.
//!
//! La sincronización es por tiempo (no frame-perfect): todos arrancan a la vez,
//! reproducen al mismo ritmo y siguen los saltos de posición del maestro.

use std::collections::HashMap;
use std::sync::mpsc::{Receiver, Sender};

use gtk::prelude::*;

use libadwaita as adw;

use crate::logging;
use crate::player::ffi;
use crate::player::embed::EmbeddedVideo;

/// Comandos que la UI envía al core sincronizado de un espejo.
#[derive(Debug, Clone, PartialEq)]
pub enum MirrorCmd {
    /// Carga el archivo (y busca a `pos` si es `Some`).
    Load(String, Option<f64>),
    Play,
    Pause,
    /// Salta a una posición (segundos).
    Seek(f64),
    /// Termina el core.
    Shutdown,
}

/// Núcleo de mpv de un espejo: vive en su propio hilo, sin audio
/// (`audio=no`), con salida embebida (`vo=libmpv`).
struct MirrorCore {
    tx: Sender<MirrorCmd>,
    /// Handle crudo para crear el `mpv_render_context` en el hilo de la UI.
    handle: ffi::mpv_handle,
}

impl MirrorCore {
    fn spawn() -> Self {
        let (cmd_tx, cmd_rx) = std::sync::mpsc::channel::<MirrorCmd>();
        // Canal por el que el hilo devuelve su handle una vez creado (usize).
        let (handle_tx, handle_rx) = std::sync::mpsc::channel::<usize>();
        std::thread::Builder::new()
            .name("mpv-mirror".into())
            .spawn(move || run_mirror(cmd_rx, handle_tx))
            .expect("el hilo del espejo debe poder crearse");
        let handle = handle_rx
            .recv()
            .expect("el hilo del espejo debe enviar su handle") as ffi::mpv_handle;
        Self {
            tx: cmd_tx,
            handle,
        }
    }

    fn send(&self, cmd: MirrorCmd) {
        if let Err(err) = self.tx.send(cmd) {
            logging::error(format!("No se pudo enviar comando al espejo: {err}"));
        }
    }
}

/// Hilo del core del espejo: posee la única instancia de mpv del espejo.
fn run_mirror(rx: Receiver<MirrorCmd>, handle_tx: Sender<usize>) {
    // libmpv exige LC_NUMERIC en "C" (evita MPV_ERROR_NOMEM).
    unsafe {
        setlocale(LC_NUMERIC, b"C\0".as_ptr());
    }

    let mut handler = match mpv::MpvHandlerBuilder::new()
        .and_then(|mut b| {
            b.set_option("vo", "libmpv")?;
            b.set_option("audio", "no")?;
            b.set_option("keep-open", "yes")?;
            b.build()
        }) {
        Ok(h) => h,
        Err(err) => {
            logging::error(format!("No se pudo crear el espejo de mpv: {err}"));
            return;
        }
    };
    let _ = handle_tx.send(handler.raw() as usize);
    logging::info("[mirror] core de mpv creado");

    // Posición pendiente de aplicar cuando el archivo termine de cargarse.
    // libmpv ignora un `seek` emitido antes de que el archivo esté cargado, así
    // que el salto se difiere al evento `FileLoaded` (evita arrancar desde 0
    // cuando se abre un espejo a mitad de reproducción).
    let mut pending_seek: Option<f64> = None;

    loop {
        // Bucle de eventos de mpv (timeout 0 => no bloqueante). Drena la cola
        // de eventos y detecta cuándo el archivo está listo para saltar.
        let mut busy = false;
        while let Some(ev) = handler.wait_event(0.0) {
            busy = true;
            if let mpv::Event::FileLoaded = ev {
                if let Some(p) = pending_seek.take() {
                    let arg = format!("{p}");
                    let _ = handler.command(&["seek", &arg, "absolute"]);
                    logging::info(format!("[mirror] cargado, saltando a {p}s"));
                }
                let _ = handler.set_property("pause", false);
            }
        }

        // Procesa los comandos de la UI (no bloqueante).
        match rx.try_recv() {
            Ok(MirrorCmd::Load(path, pos)) => {
                logging::info(format!("[mirror] cargando {path}"));
                // Pausa antes de cargar para no arrancar desde 0; el `Play` y
                // el `seek` real se aplican en `FileLoaded`.
                let _ = handler.set_property("pause", true);
                let _ = handler.command(&["loadfile", &path]);
                pending_seek = pos;
            }
            Ok(MirrorCmd::Play) => {
                let _ = handler.set_property("pause", false);
            }
            Ok(MirrorCmd::Pause) => {
                let _ = handler.set_property("pause", true);
            }
            Ok(MirrorCmd::Seek(p)) => {
                let arg = format!("{p}");
                let _ = handler.command(&["seek", &arg, "absolute"]);
            }
            Ok(MirrorCmd::Shutdown) => break,
            Err(std::sync::mpsc::TryRecvError::Disconnected) => break,
            Err(std::sync::mpsc::TryRecvError::Empty) => {}
        }

        // Pequeña pausa para no saturar la CPU cuando no hay actividad.
        if !busy {
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
    }
    logging::info("[mirror] core de mpv finalizado");
}

// LC_NUMERIC (POSIX).
const LC_NUMERIC: i32 = 1;
#[link(name = "c")]
unsafe extern "C" {
    fn setlocale(category: i32, locale: *const u8) -> *mut u8;
}

/// Una ventana de espejo: fullscreen sobre un monitor, con su GLArea.
struct MirrorWindow {
    window: gtk::ApplicationWindow,
    core: MirrorCore,
}

impl MirrorWindow {
    /// Abre una ventana fullscreen en el monitor `monitor` para el espejo dado.
    ///
    /// `monitor` ya está resuelto a su `gdk::Monitor` real por el controlador.
    fn open(id: &str, monitor: &gtk::gdk::Monitor, application: &adw::Application) -> Self {
        let core = MirrorCore::spawn();
        if core.handle.is_null() {
            logging::error(format!("[mirror] espejo {id}: sin handle de mpv"));
        }
        let video = EmbeddedVideo::with_handle(core.handle);

        let window = gtk::ApplicationWindow::builder()
            .application(application)
            .decorated(false)
            .build();
        window.set_child(Some(video.widget()));
        window.fullscreen_on_monitor(monitor);
        window.present();

        Self { window, core }
    }

    fn close(&self) {
        self.window.close();
    }
}

/// Gestiona todos los espejos: reconcilia la selección de monitores con las
/// ventanas abiertas y (re)envía la orden de reproducción a los que lo piden.
pub struct MirrorController {
    application: adw::Application,
    /// id de monitor -> espejo abierto.
    windows: HashMap<String, MirrorWindow>,
    /// Último archivo reproducido en el reproductor principal.
    current_path: Option<String>,
    /// Path que ya se cargó en los espejos abiertos (para detectar cambios de video).
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

    /// Cierra todos los espejos.
    pub fn clear(&mut self) {
        for (_, w) in self.windows.drain() {
            w.core.send(MirrorCmd::Shutdown);
            w.close();
        }
    }

    /// Cierra el espejo de un monitor concreto (p. ej. al deseleccionarlo).
    pub fn remove(&mut self, id: &str) {
        if let Some(w) = self.windows.remove(id) {
            w.core.send(MirrorCmd::Shutdown);
            w.close();
        }
    }

    /// Abre/cierra espejos para que coincidan con los monitores `selected`
    /// (ids lógicos `gdk-{i}`), respetando la selección y el estado actual.
    ///
    /// `pos_base`: posición (segundos) a la que alinear un espejo que se abre
    /// a mitad de reproducción.
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
                let w = MirrorWindow::open(id, &monitor, &self.application);
                self.windows.insert(id.clone(), w);
                self.windows[id].core.send(MirrorCmd::Load(path.clone(), pos_base));
            }
        }

        // Marcar path como cargado en espejos.
        self.mark_path_loaded();

        // Iniciar reproducción en todos.
        for w in self.windows.values() {
            w.core.send(MirrorCmd::Play);
        }
    }

    /// Notifica al reproductor principal que se reproducirá `path`.
    pub fn set_playing(&mut self, path: String) {
        self.current_path = Some(path);
    }

    /// Indica si el path actual difiere del último cargado en los espejos.
    fn path_changed(&self) -> bool {
        self.current_path.as_ref() != self.loaded_path.as_ref()
    }

    /// Marca el path como ya cargado en los espejos.
    fn mark_path_loaded(&mut self) {
        self.loaded_path = self.current_path.clone();
    }

    /// `true` mientras no haya reproducción activa (sin archivo cargado).
    pub fn is_idle(&self) -> bool {
        self.current_path.is_none()
    }

    /// Sincroniza el control (play/pausa/salto) con el maestro.
    pub fn control(&mut self, cmd: MirrorCmd) {
        for (_, w) in self.windows.iter_mut() {
            w.core.send(cmd.clone());
        }
    }

    /// Resuelve el `gdk::Monitor` real a partir del id lógico `gdk-{i}`.
    fn resolve_monitor(&self, id: &str) -> Option<gtk::gdk::Monitor> {
        let idx = id.strip_prefix("gdk-")?.parse::<usize>().ok()?;
        let display = gtk::gdk::Display::default()?;
        let monitors: Vec<gtk::gdk::Monitor> = display
            .monitors()
            .iter::<gtk::gdk::Monitor>()
            .filter_map(Result::ok)
            .collect();
        monitors.into_iter().nth(idx)
    }
}

/// Lee la posición actual del reproductor principal (`time-pos`, segundos).
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
