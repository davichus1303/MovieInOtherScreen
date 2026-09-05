/*!
 * Player implementation on libmpv, running on its own thread.
 *
 * It owns **a single instance of libmpv**: one decoding process and one
 * audio stream. The UI never touches libmpv directly; it communicates
 * via `PlayerCommand`/`PlayerEvent`.
 *
 * ## Known and verified limitation (libmpv)
 *
 * According to `libmpv/render.h`, *"at most 1 mpv_render_context can exist
 * per mpv core (it represents the main video output)"*. Therefore libmpv
 * **does not** allow duplicating the same playback to multiple full-screen
 * monitors from a single core.
 *
 * This engine maintains the unique logical playback (one decode and one
 * audio, which is the core requirement) and displays it in a window. The
 * exact duplication to N synchronized monitors is recorded as a limitation
 * and future work (GL composition with frame re-reading, `mpvpaper`-style
 * approach), since it is not supported by the libmpv API.
 */

use std::sync::mpsc::{Receiver, Sender};
use std::sync::{Mutex, OnceLock};

use super::{PlayerCommand, PlayerEvent};
use crate::logging;

use crate::constants::engine;
use crate::constants::mpv::*;
use crate::constants::player::{
    logs::ENGINE_STARTED, logs::LOAD_PREFIX, messages::INIT_FAIL, LC_NUMERIC,
};
use crate::constants::player_area::volume as volume_limit;

/**
 * Shared libmpv handle with the render layer (UI GLArea).
 *
 * The single mpv core is created on the engine thread; the UI needs that
 * handle to create the embedded `mpv_render_context`. It is set once when
 * the session starts (it does not change while the app is alive).
 *
 * A raw pointer is not `Send`; the wrapper marks the handle as safe to
 * share across threads (it is used in a synchronized manner with the mutex).
 */
struct MpvHandle(super::ffi::mpv_handle);
unsafe impl Send for MpvHandle {}

static RENDER_HANDLE: OnceLock<Mutex<Option<MpvHandle>>> = OnceLock::new();

/** Returns the mpv handle if the engine has already created it. */
pub fn mpv_handle() -> Option<super::ffi::mpv_handle> {
    RENDER_HANDLE
        .get()
        .and_then(|m| m.lock().ok())
        .and_then(|g| g.as_ref().map(|h| h.0))
}

/**
 * Starts the player thread and returns its handle.
 *
 * The thread creates and owns the single instance of mpv, listens to
 * `commands` and publishes `events`.
 */
pub fn spawn(
    commands: Receiver<PlayerCommand>,
    events: Sender<PlayerEvent>,
) -> std::thread::JoinHandle<()> {
    std::thread::Builder::new()
        .name(engine::THREAD_NAME.into())
        .spawn(move || run(commands, events))
        .expect("el hilo del reproductor debe poder crearse")
}

fn run(commands: Receiver<PlayerCommand>, events: Sender<PlayerEvent>) {
    let mut player = match MpvSession::new(&events) {
        Ok(p) => p,
        Err(err) => {
            let message = format!("{}{err}", INIT_FAIL);
            logging::error(&message);
            crate::reporting::report(crate::reporting::ErrorKind::Player, &message);
            let _ = events.send(PlayerEvent::PlaybackError(message));
            return;
        }
    };
    logging::info(ENGINE_STARTED);

    // Observa la posición y la duración para mover la barra de progreso.
    let _ = player
        .handler
        .observe_property::<f64>(PROP_TIME_POS, engine::OBSERVE_ID_TIME_POS);
    let _ = player
        .handler
        .observe_property::<f64>(PROP_DURATION, engine::OBSERVE_ID_DURATION);

    loop {
        // Drena los eventos de mpv (timeout 0 => no bloqueante). Los cambios
        // de las propiedades observadas se publican como eventos de la UI.
        let mut busy = false;
        while let Some(ev) = player.handler.wait_event(engine::EVENT_POLL_TIMEOUT_SECS) {
            busy = true;
            use mpv::Event;
            match ev {
                Event::PropertyChange { name, change, .. } => match name {
                    PROP_TIME_POS => {
                        if let mpv::Format::Double(p) = change {
                            let _ = events.send(PlayerEvent::Position(p));
                        }
                    }
                    PROP_DURATION => {
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
            Ok(PlayerCommand::Unload) => player.unload(),
            Ok(PlayerCommand::Seek(seconds)) => player.seek(seconds),
            Ok(PlayerCommand::Volume(value)) => player.set_volume(value),
            Ok(PlayerCommand::Mute(muted)) => player.set_muted(muted),
            Ok(PlayerCommand::Shutdown) => break,
            Err(std::sync::mpsc::TryRecvError::Disconnected) => break,
            Err(std::sync::mpsc::TryRecvError::Empty) => {}
        }

        // Pequeña pausa para no saturar la CPU cuando no hay actividad.
        if !busy {
            std::thread::sleep(std::time::Duration::from_millis(engine::IDLE_SLEEP_MS));
        }
    }
}

/** Session wrapper around the single mpv instance. */
struct MpvSession {
    handler: mpv::MpvHandler,
    events: Sender<PlayerEvent>,
    paused: bool,
}

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
        // Software volume (independent of the system mixer), capped at 100%
        // so it never amplifies beyond the system limit.
        builder.set_option(OPT_VOLUME_MAX, volume_limit::MAX)?;
        builder.set_option(OPT_KEEP_OPEN, VALUE_YES)?;
        // Embeber la salida en un GLArea de la app (Celluloid-style) en lugar
        // de abrir la ventana propia de mpv.
        builder.set_option(OPT_VO, VALUE_VO_LIBMPV)?;
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
        logging::info(format!("{}{path}", LOAD_PREFIX));
        if let Err(err) = self.handler.command(&[CMD_LOADFILE, path]) {
            let message = format!("Error al cargar el vídeo '{path}': {err}");
            logging::error(&message);
            self.report_error_str(message);
            return;
        }
        // Con `keep-open=yes`, cuando el vídeo anterior termina mpv deja el
        // core en pausa; `loadfile` no resetea `pause`, así que el nuevo vídeo
        // cargaría detenido en el área principal. Se des-pausa explícitamente
        // (igual que hacen los espejos en `FileLoaded`) para que arranque solo.
        self.play();
    }

    fn play(&mut self) {
        if let Err(err) = self.handler.set_property(PROP_PAUSE, false) {
            self.report_error(err);
            return;
        }
        self.set_paused(false);
    }

    fn pause(&mut self) {
        if let Err(err) = self.handler.set_property(PROP_PAUSE, true) {
            self.report_error(err);
            return;
        }
        self.set_paused(true);
    }

    fn stop(&mut self) {
        let _ = self.handler.set_property(PROP_PAUSE, true);
        let _ = self
            .handler
            .command(&[CMD_SEEK, SEEK_TO_START, SEEK_MODE_ABSOLUTE]);
        self.set_paused(true);
    }

    /**
     * Fully unloads the current video, leaving the player without a file:
     * the GLArea becomes empty and nothing can be played again until
     * another video is loaded.
     */
    fn unload(&mut self) {
        let _ = self.handler.set_property(PROP_PAUSE, true);
        // Cargar una ruta vacía desvincula el archivo actual del core de mpv.
        let _ = self.handler.command(&[CMD_LOADFILE, EMPTY_LOAD_PATH]);
        self.set_paused(true);
    }

    fn seek(&mut self, seconds: f64) {
        if let Err(err) = self.seek_to(seconds) {
            self.report_error(err);
        }
    }

    fn seek_to(&mut self, seconds: f64) -> mpv::Result<()> {
        let arg = format!("{seconds}");
        self.handler.command(&[CMD_SEEK, &arg, SEEK_MODE_ABSOLUTE])
    }

    /**
     * Sets the playback volume, clamped to [0, 100]. It is mpv's software
     * volume, so it does not touch the system mixer and never exceeds the
     * `volume-max` cap set at configuration time.
     */
    fn set_volume(&mut self, volume: f64) {
        let v = volume.clamp(volume_limit::MIN, volume_limit::MAX);
        if let Err(err) = self.handler.set_property(PROP_VOLUME, v) {
            self.report_error(err);
        }
    }

    /** Mutes or unmutes the audio. */
    fn set_muted(&mut self, muted: bool) {
        if let Err(err) = self.handler.set_property(PROP_MUTE, muted) {
            self.report_error(err);
        }
    }

    fn set_paused(&mut self, paused: bool) {
        self.paused = paused;
        let _ = self.events.send(PlayerEvent::Paused(paused));
    }

    /**
     * Applies a gradual input transition (~3 s) before the change.
     *
     * libmpv does not offer a native crossfade between two videos in the
     * sequence; as a smooth and non-abrupt approximation, a video and audio
     * fade-in is applied when loading the new item.
     */
    fn apply_transition_into(&mut self) {
        let _ = self.handler.command(&[
            CMD_AF,
            &format!("fade in:st=0:d={}", engine::TRANSITION_SECONDS),
        ]);
        let _ = self.handler.command(&[
            CMD_VF,
            &format!("fade in:st=0:d={}", engine::TRANSITION_SECONDS),
        ]);
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
