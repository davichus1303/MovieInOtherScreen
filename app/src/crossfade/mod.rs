/*!
 * Crossfade transition between videos.
 *
 * The engine switches files with `loadfile` (a hard cut). When enabled, each
 * change to a new video spawns a short-lived "outgoing" core: it keeps the
 * previous video (image and audio) on top of the player while both fade away
 * over [`crate::constants::crossfade::DURATION_SECS`], revealing the new video
 * that the engine is already playing underneath.
 *
 * Reuses the strategy of the mirrors (`crate::mirror`): one extra mpv core per
 * spawned layer, an `mpv_render_context` embedded in a `GtkGLArea`, and an
 * explicit channel back to the core thread. The incoming video is NOT loaded
 * again: it stays on the main engine core, whose existing fade-in complements
 * this crossfade (outgoing audio ramps down while incoming ramps up, DJ style).
 *
 * The main player, the mirrors, the timeline and the volume controls are not
 * touched; this module only adds the visual layer and the checkbox state.
 */

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::mpsc::{Receiver, Sender};

use gtk::glib;
use gtk::prelude::*;

use crate::constants::crossfade as cfg;
use crate::constants::mpv as mpv_const;
use crate::logging;
use crate::player::embed::EmbeddedVideo;
use crate::player::ffi;
use crate::reporting::{self, ErrorKind};

/** Commands the UI sends to an outgoing ("fade out") core. */
#[derive(Debug, Clone, Copy, PartialEq)]
enum FadeCmd {
    /** Sets the playback volume of the outgoing video (0-100). */
    Volume(f64),
    /** Terminates the core. */
    Shutdown,
}

/**
 * Short-lived mpv core that keeps rendering the previous video during the
 * transition. Lives on its own thread, with sound (`audio` enabled), with
 * embedded output (`vo=libmpv`).
 */
struct FadeCore {
    tx: Sender<FadeCmd>,
    /** Raw handle for creating the `mpv_render_context` on the UI thread. */
    handle: ffi::mpv_handle,
}

impl FadeCore {
    /**
     * Creates the core thread, loads `path` at `position` and returns it.
     *
     * `volume`/`muted` mirror the state of the main engine so the outgoing
     * video starts at the same loudness the user was hearing.
     */
    fn spawn(path: &str, position: f64, volume: f64, muted: bool) -> Option<Self> {
        let (cmd_tx, cmd_rx) = std::sync::mpsc::channel::<FadeCmd>();
        let (handle_tx, handle_rx) = std::sync::mpsc::channel::<usize>();
        let path = path.to_string();
        if std::thread::Builder::new()
            .name(cfg::THREAD_NAME.into())
            .spawn(move || run_fade_core(cmd_rx, handle_tx, &path, position, volume, muted))
            .is_err()
        {
            reporting::report(ErrorKind::Player, cfg::messages::THREAD_CREATE_FAIL);
            return None;
        }
        let handle = match handle_rx.recv() {
            Ok(h) => h as ffi::mpv_handle,
            Err(_) => {
                reporting::report(ErrorKind::Player, cfg::messages::CORE_TERMINATED);
                return None;
            }
        };
        Some(Self { tx: cmd_tx, handle })
    }

    fn volume(&self, volume: f64) {
        let _ = self.tx.send(FadeCmd::Volume(volume));
    }
}

/** Outgoing core thread: plays the previous file while the UI fades it out. */
fn run_fade_core(
    rx: Receiver<FadeCmd>,
    handle_tx: Sender<usize>,
    path: &str,
    position: f64,
    volume: f64,
    muted: bool,
) {
    crate::player::ffi::ensure_lc_numeric_c();

    let mut handler = match mpv::MpvHandlerBuilder::new().and_then(|mut b| {
        b.set_option(mpv_const::OPT_VO, mpv_const::VALUE_VO_LIBMPV)?;
        b.set_option(mpv_const::OPT_KEEP_OPEN, mpv_const::VALUE_YES)?;
        // Config y scripts del usuario desactivados: comportamiento
        // determinista y sin scripts/ytdl/IPC inyectados desde el sistema.
        b.set_option(mpv_const::OPT_CONFIG, mpv_const::VALUE_NO)?;
        b.set_option(mpv_const::OPT_LOAD_SCRIPTS, mpv_const::VALUE_NO)?;
        // Aceleración por hardware (estilo VLC): si hay GPU dedicada o
        // integrada se decodifica por hardware, sin depender del códec.
        crate::hwaccel::apply_to(&mut b)?;
        b.build()
    }) {
        Ok(h) => h,
        Err(err) => {
            logging::error(format!("{}{err}", cfg::messages::CORE_CREATE_FAIL));
            reporting::report(
                ErrorKind::Player,
                format!("{}{err}", cfg::messages::CORE_INIT_FAIL),
            );
            return;
        }
    };
    let _ = handle_tx.send(handler.raw() as usize);
    logging::info(cfg::logs::CORE_CREATED);

    // Cargar pausado: el `seek` y el volumen se aplican en `FileLoaded`
    // (libmpv ignora el `seek` antes de que el archivo esté cargado).
    let _ = handler.set_property(mpv_const::PROP_PAUSE, true);
    let _ = handler.command(&[mpv_const::CMD_LOADFILE, path]);

    // Último volumen recibido (los ticks de la UI pueden llegar antes de que
    // `FileLoaded` se emita; se aplica el más reciente al arrancar).
    let mut pending_volume = volume;

    loop {
        let mut busy = false;
        while let Some(ev) = handler.wait_event(cfg::EVENT_POLL_TIMEOUT_SECS) {
            busy = true;
            if let mpv::Event::FileLoaded = ev {
                let arg = format!("{position}");
                let _ =
                    handler.command(&[mpv_const::CMD_SEEK, &arg, mpv_const::SEEK_MODE_ABSOLUTE]);
                let _ = handler.set_property(mpv_const::PROP_VOLUME, pending_volume);
                let _ = handler.set_property(mpv_const::PROP_MUTE, muted);
                let _ = handler.set_property(mpv_const::PROP_PAUSE, false);
            }
        }

        match rx.try_recv() {
            Ok(FadeCmd::Volume(v)) => {
                pending_volume = v;
                let _ = handler.set_property(mpv_const::PROP_VOLUME, v);
            }
            Ok(FadeCmd::Shutdown) => break,
            Err(std::sync::mpsc::TryRecvError::Disconnected) => break,
            Err(std::sync::mpsc::TryRecvError::Empty) => {}
        }

        // Pequeña pausa para no saturar la CPU cuando no hay actividad.
        if !busy {
            std::thread::sleep(std::time::Duration::from_millis(cfg::IDLE_SLEEP_MS));
        }
    }
    logging::info(cfg::logs::CORE_ENDED);
}

/** A transition currently being animated on the main thread. */
struct ActiveFade {
    /** Identifies this fade so stale timers are discarded. */
    id: u64,
    core: FadeCore,
    /** Outgoing video layer (removed from the stage when the fade ends). */
    widget: gtk::GLArea,
    /** Loudness captured from the engine when the fade started. */
    start_volume: f64,
    /** Milliseconds elapsed since the fade started. */
    elapsed_ms: u32,
}

/**
 * Manages the crossfade state: whether it is enabled, the current video, and
 * the in-flight fade layer. Owns the `gtk::Overlay` that stages the layers.
 */
pub struct CrossfadeController {
    enabled: bool,
    stage: gtk::Overlay,
    current_path: Option<String>,
    active: Option<ActiveFade>,
    /** Id of the next fade (monotonic, guards stale timers). */
    next_fade_id: u64,
    /** Source of the animation timer of the current fade. */
    timer: Option<glib::SourceId>,
}

impl CrossfadeController {
    /** Builds the controller over the video `stage`. */
    pub fn new(stage: gtk::Overlay) -> Self {
        Self {
            enabled: false,
            stage,
            current_path: None,
            active: None,
            next_fade_id: 0,
            timer: None,
        }
    }

    /** Enable/disable the crossfade effect (`set_active` of the checkbox). */
    pub fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
    }

    /**
     * Notifies the controller that the player is about to switch to `new_path`.
     *
     * Must be invoked BEFORE the engine is told to load, so the engine still
     * reports the old video's position (`time-pos`). If a crossfade can apply
     * (enabled, previous video present and different, playback active) the
     * outgoing layer is spawned and animated.
     */
    pub fn notify_play(shared: &Rc<RefCell<Self>>, new_path: &str) {
        let mut this = shared.borrow_mut();
        let previous = this.current_path.replace(new_path.to_string());
        if !this.enabled {
            return;
        }
        let Some(old_path) = previous else { return };
        if old_path == new_path {
            return;
        }
        let Some(position) = crate::mirror::main_time_pos() else {
            return;
        };
        let volume = read_double_property(mpv_const::PROP_VOLUME)
            .unwrap_or(crate::constants::player_area::volume::DEFAULT);
        let muted = read_bool_property(mpv_const::PROP_MUTE).unwrap_or(false);
        drop(this);
        Self::start_fade(shared, &old_path, position, volume, muted);
    }

    /** Starts the fading layer and schedules its animation. */
    fn start_fade(
        shared: &Rc<RefCell<Self>>,
        old_path: &str,
        position: f64,
        volume: f64,
        muted: bool,
    ) {
        let mut this = shared.borrow_mut();
        // Temporizador y transición en curso: la nueva operación los supera
        // (cualquier tick obsoleto se descarta comparando el id del fade).
        if let Some(source) = this.timer.take() {
            source.remove();
        }
        this.cancel_active();
        let fade_id = this.next_fade_id;
        this.next_fade_id += 1;

        let Some(core) = FadeCore::spawn(old_path, position, volume, muted) else {
            return;
        };
        let video = EmbeddedVideo::with_handle(core.handle);
        let widget = video.widget().clone();
        widget.set_hexpand(true);
        widget.set_vexpand(true);
        // La capa superior no debe interceptar la interacción con el resto.
        widget.set_can_target(false);
        // Empieza totalmente opaca (cubre el recién cargado) y se desvanece.
        widget.set_opacity(1.0);
        {
            let shutdown_tx = core.tx.clone();
            widget.connect_unrealize(move |_| {
                let _ = shutdown_tx.send(FadeCmd::Shutdown);
            });
        }
        this.stage.add_overlay(&widget);
        this.active = Some(ActiveFade {
            id: fade_id,
            core,
            widget,
            start_volume: volume,
            elapsed_ms: 0,
        });
        logging::info(cfg::logs::FADE_STARTED);

        let timer_shared = shared.clone();
        this.timer = Some(glib::timeout_add_local(
            std::time::Duration::from_millis(cfg::STEP_MS as u64),
            move || timer_shared.borrow_mut().advance_fade(fade_id),
        ));
    }

    /** Advances the current fade one tick; ends it when the time elapses. */
    fn advance_fade(&mut self, fade_id: u64) -> glib::ControlFlow {
        let Some(fade) = self.active.as_mut() else {
            self.timer = None;
            return glib::ControlFlow::Break;
        };
        if fade.id != fade_id {
            // Tick de un fade ya sustituido: se descarta junto a su temporizador.
            self.timer = None;
            return glib::ControlFlow::Break;
        }
        fade.elapsed_ms += cfg::STEP_MS;
        let duration_ms = cfg::DURATION_SECS * 1000.0;
        let progress = (fade.elapsed_ms as f64 / duration_ms).clamp(0.0, 1.0);

        // Vídeo: opacidad 1.0 -> 0.0. Audio: volumen de salida -> 0.
        fade.widget.set_opacity(1.0 - progress);
        fade.core.volume(fade.start_volume * (1.0 - progress));

        if progress >= 1.0 {
            let fade = self.active.take().expect("fade activo confirmado");
            self.finish_fade(fade);
            self.timer = None;
            return glib::ControlFlow::Break;
        }
        glib::ControlFlow::Continue
    }

    /** Stops any running fade without waiting for its animation end. */
    fn cancel_active(&mut self) {
        if let Some(fade) = self.active.take() {
            self.finish_fade(fade);
        }
    }

    /**
     * Tears the layer down: removes it from the stage (fires `unrealize`,
     * which already sends `Shutdown` to the core) and resends the command as
     * a safety net before the core is dropped.
     */
    fn finish_fade(&mut self, fade: ActiveFade) {
        self.stage.remove_overlay(&fade.widget);
        let _ = fade.core.tx.send(FadeCmd::Shutdown);
        logging::info(cfg::logs::FADE_ENDED);
    }
}

/** Reads a double mpv property from the main engine's core. */
fn read_double_property(name: &str) -> Option<f64> {
    use std::ffi::CString;
    use std::os::raw::c_char;

    let handle = crate::player::mpv_engine::mpv_handle()?;
    let cname = CString::new(name).ok()?;
    let mut value: f64 = 0.0;
    let rc = unsafe {
        ffi::mpv_get_property(
            handle,
            cname.as_ptr() as *const c_char,
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

/** Reads a boolean mpv property from the main engine's core. */
fn read_bool_property(name: &str) -> Option<bool> {
    use std::ffi::CString;
    use std::os::raw::c_char;

    let handle = crate::player::mpv_engine::mpv_handle()?;
    let cname = CString::new(name).ok()?;
    let mut value: i32 = 0;
    let rc = unsafe {
        ffi::mpv_get_property(
            handle,
            cname.as_ptr() as *const c_char,
            ffi::MPV_FORMAT_FLAG,
            (&mut value as *mut i32).cast(),
        )
    };
    if rc < 0 {
        None
    } else {
        Some(value != 0)
    }
}
