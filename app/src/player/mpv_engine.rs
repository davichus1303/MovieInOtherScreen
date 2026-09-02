//! Implementación del reproductor sobre libmpv, ejecutada en un hilo propio.
//!
//! Posee **una única instancia de libmpv**: una única decodificación y un
//! único flujo de audio. La UI nunca toca libmpv directamente; se comunica
//! por `PlayerCommand`/`PlayerEvent`.
//!
//! ## Limitación conocida y verificada (libmpv)
//!
//! Según `libmpv/render.h`, *"at most 1 mpv_render_context can exist per mpv
//! core (it represents the main video output)"*. Por tanto libmpv **no**
//! permite duplicar una misma reproducción a varios monitores en pantalla
//! completa desde un único núcleo.
//!
//! Este motor mantiene la reproducción lógica única (un solo decode y un solo
//! audio, que es el requisito central) y la muestra en una ventana. La
//! duplicación exacta a N monitores sincronizados queda registrada como
//! limitación y como trabajo futuro (composición GL con relectura de frames,
//! enfoque tipo `mpvpaper`), al no estar soportada por la API de libmpv.

use std::sync::mpsc::{Receiver, Sender};
use std::sync::{Mutex, OnceLock};

use super::{PlayerCommand, PlayerEvent};
use crate::logging;

/// Duración (segundos) de la transición gradual al cambiar de vídeo.
pub const TRANSITION_SECONDS: f64 = 3.0;

/// Handle libmpv compartido con la capa de render (GLArea de la UI).
///
/// El único core de mpv se crea en el hilo del motor; la UI necesita ese
/// handle para crear el `mpv_render_context` embebido. Se fija una sola vez
/// al arrancar la sesión (no cambia mientras la app vive).
///
/// Un raw pointer no es `Send`; el wrapper marca el handle como seguro de
/// compartir entre hilos (el ocupa se usa de forma sincronizada con el mutex).
struct MpvHandle(super::ffi::mpv_handle);
unsafe impl Send for MpvHandle {}

static RENDER_HANDLE: OnceLock<Mutex<Option<MpvHandle>>> = OnceLock::new();

/// Devuelve el handle de mpv si el motor ya lo creó.
pub fn mpv_handle() -> Option<super::ffi::mpv_handle> {
    RENDER_HANDLE
        .get()
        .and_then(|m| m.lock().ok())
        .and_then(|g| g.as_ref().map(|h| h.0))
}

/// Arranca el hilo del reproductor y devuelve su handle.
///
/// El hilo crea y posee la única instancia de mpv, escucha `commands` y
/// publica `events`.
pub fn spawn(
    commands: Receiver<PlayerCommand>,
    events: Sender<PlayerEvent>,
) -> std::thread::JoinHandle<()> {
    std::thread::Builder::new()
        .name("mpv-engine".into())
        .spawn(move || run(commands, events))
        .expect("el hilo del reproductor debe poder crearse")
}

fn run(commands: Receiver<PlayerCommand>, events: Sender<PlayerEvent>) {
    let mut player = match MpvSession::new(&events) {
        Ok(p) => p,
        Err(err) => {
            let message = format!("No se pudo inicializar el reproductor: {err}");
            logging::error(&message);
            crate::reporting::report(crate::reporting::ErrorKind::Player, &message);
            let _ = events.send(PlayerEvent::PlaybackError(message));
            return;
        }
    };
    logging::info("Motor mpv inicializado correctamente.");

    // Observa la posición y la duración para mover la barra de progreso.
    let _ = player.handler.observe_property::<f64>("time-pos", 1);
    let _ = player.handler.observe_property::<f64>("duration", 2);

    loop {
        // Drena los eventos de mpv (timeout 0 => no bloqueante). Los cambios
        // de las propiedades observadas se publican como eventos de la UI.
        let mut busy = false;
        while let Some(ev) = player.handler.wait_event(0.0) {
            busy = true;
            use mpv::Event;
            match ev {
                Event::PropertyChange { name, change, .. } => match name {
                    "time-pos" => {
                        if let mpv::Format::Double(p) = change {
                            let _ = events.send(PlayerEvent::Position(p));
                        }
                    }
                    "duration" => {
                        if let mpv::Format::Double(d) = change {
                            let _ = events.send(PlayerEvent::Duration(d));
                        }
                    }
                    _ => {}
                },
                Event::Pause => {
                    player.paused = true;
                    let _ = events.send(PlayerEvent::Paused(true));
                }
                Event::Unpause => {
                    player.paused = false;
                    let _ = events.send(PlayerEvent::Paused(false));
                }
                Event::EndFile(Ok(_)) => {
                    let _ = events.send(PlayerEvent::Ended);
                }
                _ => {}
            }
        }

        // Procesa los comandos de la UI (no bloqueante).
        match commands.try_recv() {
            Ok(PlayerCommand::Load(path)) => player.load(&path),
            Ok(PlayerCommand::Play) => player.play(),
            Ok(PlayerCommand::Pause) => player.pause(),
            Ok(PlayerCommand::Stop) => player.stop(),
            Ok(PlayerCommand::Seek(seconds)) => player.seek(seconds),
            Ok(PlayerCommand::TogglePause) => player.toggle_pause(),
            Ok(PlayerCommand::Shutdown) => break,
            Err(std::sync::mpsc::TryRecvError::Disconnected) => break,
            Err(std::sync::mpsc::TryRecvError::Empty) => {}
        }

        // Pequeña pausa para no saturar la CPU cuando no hay actividad.
        if !busy {
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
    }
}

/// Sesión homogénea sobre la instancia única de mpv.
struct MpvSession {
    handler: mpv::MpvHandler,
    events: Sender<PlayerEvent>,
    paused: bool,
}

/// Categoría de locale `LC_NUMERIC` (definición POSIX).
const LC_NUMERIC: i32 = 1;

unsafe extern "C" {
    fn setlocale(category: i32, locale: *const u8) -> *mut u8;
}

impl MpvSession {
    fn new(events: &Sender<PlayerEvent>) -> mpv::Result<Self> {
        // libmpv exige `LC_NUMERIC` en `"C"`. El ajuste hecho en `main` puede
        // perderse cuando GTK/GLib resetea el locale a las variables de entorno
        // al inicializarse, así que se vuelve a fijar justo aquí, antes de crear
        // el núcleo de mpv (fallaría con MPV_ERROR_NOMEM si no fuera "C").
        unsafe {
            setlocale(LC_NUMERIC, b"C\0".as_ptr());
        }

        let mut builder = mpv::MpvHandlerBuilder::new()?;
        // Aceleración por hardware (estilo VLC): se activa si hay cualquier GPU
        // (dedicada o integrada), sin depender del códec del vídeo.
        crate::hwaccel::apply_to(&mut builder)?;
        builder.set_option("keep-open", "yes")?;
        // Embeber la salida en un GLArea de la app (Celluloid-style) en lugar
        // de abrir la ventana propia de mpv.
        builder.set_option("vo", "libmpv")?;
        let handler = builder.build()?;

        // Exponer el handle al renderer embebido de la UI.
        let _ = RENDER_HANDLE.set(Mutex::new(Some(MpvHandle(
            handler.raw() as super::ffi::mpv_handle
        ))));

        let session = Self {
            handler,
            events: events.clone(),
            paused: false,
        };
        Ok(session)
    }

    fn load(&mut self, path: &str) {
        self.apply_transition_into();
        logging::info(format!("Cargando vídeo en el motor mpv: {path}"));
        if let Err(err) = self.handler.command(&["loadfile", path]) {
            let message = format!("Error al cargar el vídeo '{path}': {err}");
            logging::error(&message);
            self.report_error_str(message);
        }
    }

    fn play(&mut self) {
        if let Err(err) = self.handler.set_property("pause", false) {
            self.report_error(err);
            return;
        }
        self.set_paused(false);
    }

    fn pause(&mut self) {
        if let Err(err) = self.handler.set_property("pause", true) {
            self.report_error(err);
            return;
        }
        self.set_paused(true);
    }

    fn toggle_pause(&mut self) {
        let target = !self.paused;
        let _ = self.handler.set_property("pause", target);
        self.set_paused(target);
    }

    fn stop(&mut self) {
        let _ = self.handler.set_property("pause", true);
        let _ = self.handler.command(&["seek", "0", "absolute"]);
        self.set_paused(true);
    }

    fn seek(&mut self, seconds: f64) {
        if let Err(err) = self.seek_to(seconds) {
            self.report_error(err);
        }
    }

    fn seek_to(&mut self, seconds: f64) -> mpv::Result<()> {
        let arg = format!("{seconds}");
        self.handler.command(&["seek", &arg, "absolute"])
    }

    fn set_paused(&mut self, paused: bool) {
        self.paused = paused;
        let _ = self.events.send(PlayerEvent::Paused(paused));
    }

    /// Aplica una transición gradual de entrada (~3 s) antes del cambio.
    ///
    /// libmpv no ofrece un fundido cruzado nativo entre dos vídeos de la
    /// secuencia; como aproximación correcta y no brusca se aplica un fundido
    /// de entrada de vídeo y audio al cargar el nuevo elemento.
    fn apply_transition_into(&mut self) {
        let _ = self
            .handler
            .command(&["af", &format!("fade in:st=0:d={TRANSITION_SECONDS}")]);
        let _ = self
            .handler
            .command(&["vf", &format!("fade in:st=0:d={TRANSITION_SECONDS}")]);
    }

    fn report_error(&self, err: mpv::Error) {
        let message = format!("{err}");
        self.report_error_str(message);
    }

    fn report_error_str(&self, message: String) {
        logging::error(&message);
        let _ = self.events.send(PlayerEvent::PlaybackError(message));
    }
}
