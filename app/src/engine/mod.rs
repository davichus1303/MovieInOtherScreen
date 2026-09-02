//! Implementación concreta del motor de reproducción con libmpv.
//!
//! Este módulo encapsula toda la interacción con libmpv, aislando
//! el resto de la aplicación de los detalles de la API C.

use std::sync::mpsc::{Receiver, Sender, TryRecvError};
use std::thread;

use mos_core::playback::{PlaybackCmd, PlaybackEvent, PlaybackState};

/// Handle interno de mpv (opaco para el resto de la app).
type MpvHandle = *mut std::os::raw::c_void;

/// Sesión de mpv que vive en su propio hilo.
struct MpvSession {
    handler: mpv::MpvHandler,
    events: Sender<PlaybackEvent>,
    state: PlaybackState,
    audio_device: Option<String>,
}

impl MpvSession {
    fn new(events: &Sender<PlaybackEvent>) -> mpv::Result<Self> {
        // libmpv exige LC_NUMERIC en "C"
        unsafe {
            setlocale(LC_NUMERIC, b"C\0".as_ptr());
        }

        let mut builder = mpv::MpvHandlerBuilder::new()?;
        // Aceleración por hardware (estilo VLC): se activa si hay cualquier GPU
        // (dedicada o integrada), sin depender del códec del vídeo.
        crate::hwaccel::apply_to(&mut builder)?;
        builder.set_option("keep-open", "yes")?;
        builder.set_option("vo", "libmpv")?;
        builder.set_option("audio", "yes")?;
        let handler = builder.build()?;

        let session = Self {
            handler,
            events: events.clone(),
            state: PlaybackState::default(),
            audio_device: None,
        };
        Ok(session)
    }

    fn run(&mut self, cmd_rx: Receiver<PlaybackCmd>) {
        // Observar propiedades
        let _ = self.handler.observe_property::<f64>("time-pos", 1);
        let _ = self.handler.observe_property::<f64>("duration", 2);
        let _ = self.handler.observe_property::<bool>("pause", 3);

        loop {
            // Procesar eventos de mpv
            while let Some(ev) = self.handler.wait_event(0.0) {
                self.handle_mpv_event(ev);
            }

            // Procesar comandos de la UI
            match cmd_rx.try_recv() {
                Ok(cmd) => self.handle_command(cmd),
                Err(TryRecvError::Empty) => {}
                Err(TryRecvError::Disconnected) => break,
            }

            // Pequeña pausa para no saturar CPU
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
    }

    fn handle_mpv_event(&mut self, ev: mpv::Event) {
        use mpv::Event;
        match ev {
            mpv::Event::PropertyChange { name, change, .. } => match name.as_ref() {
                "time-pos" => {
                    if let mpv::Format::Double(pos) = change {
                        self.state.update_position(pos);
                        let _ = self.events.send(PlaybackEvent::Position(pos));
                    }
                }
                "duration" => {
                    if let mpv::Format::Double(dur) = change {
                        self.state.update_duration(dur);
                        let _ = self.events.send(PlaybackEvent::Duration(dur));
                    }
                }
                "pause" => {
                    if let mpv::Format::Flag(paused) = change {
                        self.state.set_paused(paused);
                        let _ = self.events.send(PlaybackEvent::Paused(paused));
                    }
                }
                _ => {}
            },
            mpv::Event::EndFile(res) => {
                if res.is_ok() {
                    let _ = self.events.send(PlaybackEvent::Ended);
                }
            }
            _ => {}
        }
    }

    fn handle_command(&mut self, cmd: PlaybackCmd) {
        match cmd {
            PlaybackCmd::Load(path) => self.load(&path),
            PlaybackCmd::Play => self.play(),
            PlaybackCmd::Pause => self.pause(),
            PlaybackCmd::TogglePause => self.toggle_pause(),
            PlaybackCmd::Seek(pos) => self.seek(pos),
            PlaybackCmd::Stop => self.stop(),
            PlaybackCmd::SetAudioDevice(id) => self.set_audio_device(&id),
            PlaybackCmd::Shutdown => {
                let _ = self.handler.command(&["quit"]);
            }
        }
    }

    fn load(&mut self, path: &str) {
        self.apply_transition();
        if let Err(err) = self.handler.command(&["loadfile", path]) {
            let msg = format!("Error al cargar '{path}': {err}");
            let _ = self.events.send(PlaybackEvent::Error(msg));
        } else {
            self.state.set_path(Some(path.to_string()));
        }
    }

    fn play(&mut self) {
        if let Err(err) = self.handler.set_property("pause", false) {
            let _ = self.events.send(PlaybackEvent::Error(err.to_string()));
        } else {
            self.state.set_paused(false);
        }
    }

    fn pause(&mut self) {
        if let Err(err) = self.handler.set_property("pause", true) {
            let _ = self.events.send(PlaybackEvent::Error(err.to_string()));
        } else {
            self.state.set_paused(true);
        }
    }

    fn toggle_pause(&mut self) {
        let target = !self.state.paused;
        if let Err(err) = self.handler.set_property("pause", target) {
            let _ = self.events.send(PlaybackEvent::Error(err.to_string()));
        } else {
            self.state.set_paused(target);
        }
    }

    fn stop(&mut self) {
        let _ = self.handler.set_property("pause", true);
        let _ = self.handler.command(&["seek", "0", "absolute"]);
    }

    fn seek(&mut self, seconds: f64) {
        if let Err(err) = self.seek_to(seconds) {
            let _ = self.events.send(PlaybackEvent::Error(err.to_string()));
        }
    }

    fn seek_to(&mut self, seconds: f64) -> mpv::Result<()> {
        let arg = format!("{seconds}");
        self.handler.command(&["seek", &arg, "absolute"])
    }

    fn set_audio_device(&mut self, id: &str) {
        let full_id =
            if id.starts_with("pipewire/") || id.starts_with("pulse/") || id.starts_with("alsa/") {
                id.to_string()
            } else {
                format!("pipewire/{id}")
            };
        if let Err(err) = self.handler.set_property("audio-device", full_id.as_str()) {
            let _ = self.events.send(PlaybackEvent::Error(err.to_string()));
        } else {
            self.audio_device = Some(id.to_string());
        }
    }

    fn apply_transition(&mut self) {
        const TRANSITION_SECONDS: f64 = 3.0;
        let _ = self
            .handler
            .command(&["af", &format!("fade in:st=0:d={TRANSITION_SECONDS}")]);
        let _ = self
            .handler
            .command(&["vf", &format!("fade in:st=0:d={TRANSITION_SECONDS}")]);
    }
}

/// Punto de entrada para el hilo del motor mpv.
fn run_mpv_engine(cmd_rx: Receiver<PlaybackCmd>, events: Sender<PlaybackEvent>) {
    let mut session = match MpvSession::new(&events) {
        Ok(s) => s,
        Err(err) => {
            let _ = events.send(PlaybackEvent::Error(format!("mpv init failed: {err}")));
            return;
        }
    };
    session.run(cmd_rx);
}

/// Spawnea el hilo del motor mpv.
pub fn spawn_mpv_engine(
    cmd_rx: Receiver<PlaybackCmd>,
    events: Sender<PlaybackEvent>,
) -> thread::JoinHandle<()> {
    thread::Builder::new()
        .name("mpv-engine".into())
        .spawn(move || run_mpv_engine(cmd_rx, events))
        .expect("mpv engine thread must start")
}

// LC_NUMERIC (POSIX).
const LC_NUMERIC: i32 = 1;
unsafe extern "C" {
    fn setlocale(category: i32, locale: *const u8) -> *mut u8;
}
