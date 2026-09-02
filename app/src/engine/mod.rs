/*
 * Concrete implementation of the playback engine with libmpv.
 *
 * This module encapsulates all interaction with libmpv, isolating the rest of
 * the application from the details of the C API.
 */

use std::sync::mpsc::{Receiver, Sender, TryRecvError};
use std::thread;

use mos_core::playback::{PlaybackCmd, PlaybackEvent, PlaybackState};

use crate::constants::engine as eng;
use crate::constants::mpv::*;
use crate::constants::player;

/** Internal mpv handle (opaque to the rest of the app). */
type MpvHandle = *mut std::os::raw::c_void;

/** mpv session that lives on its own thread. */
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
            setlocale(player::LC_NUMERIC, b"C\0".as_ptr());
        }

        let mut builder = mpv::MpvHandlerBuilder::new()?;
        // Aceleración por hardware (estilo VLC): se activa si hay cualquier GPU
        // (dedicada o integrada), sin depender del códec del vídeo.
        crate::hwaccel::apply_to(&mut builder)?;
        builder.set_option(OPT_KEEP_OPEN, VALUE_YES)?;
        builder.set_option(OPT_VO, VALUE_VO_LIBMPV)?;
        builder.set_option(OPT_AUDIO, VALUE_YES)?;
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
        let _ = self.handler.observe_property::<f64>(PROP_TIME_POS, eng::OBSERVE_ID_TIME_POS);
        let _ = self.handler.observe_property::<f64>(PROP_DURATION, eng::OBSERVE_ID_DURATION);
        let _ = self.handler.observe_property::<bool>(PROP_PAUSE, eng::OBSERVE_ID_PAUSE);

        loop {
            // Procesar eventos de mpv
            while let Some(ev) = self.handler.wait_event(eng::EVENT_POLL_TIMEOUT_SECS) {
                self.handle_mpv_event(ev);
            }

            // Procesar comandos de la UI
            match cmd_rx.try_recv() {
                Ok(cmd) => self.handle_command(cmd),
                Err(TryRecvError::Empty) => {}
                Err(TryRecvError::Disconnected) => break,
            }

            // Pequeña pausa para no saturar CPU
            std::thread::sleep(std::time::Duration::from_millis(eng::IDLE_SLEEP_MS));
        }
    }

    fn handle_mpv_event(&mut self, ev: mpv::Event) {
        use mpv::Event;
        match ev {
            mpv::Event::PropertyChange { name, change, .. } => match name.as_ref() {
                PROP_TIME_POS => {
                    if let mpv::Format::Double(pos) = change {
                        self.state.update_position(pos);
                        let _ = self.events.send(PlaybackEvent::Position(pos));
                    }
                }
                PROP_DURATION => {
                    if let mpv::Format::Double(dur) = change {
                        self.state.update_duration(dur);
                        let _ = self.events.send(PlaybackEvent::Duration(dur));
                    }
                }
                PROP_PAUSE => {
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
                let _ = self.handler.command(&[CMD_QUIT]);
            }
        }
    }

    fn load(&mut self, path: &str) {
        self.apply_transition();
        if let Err(err) = self.handler.command(&[CMD_LOADFILE, path]) {
            let msg = format!("Error al cargar '{path}': {err}");
            let _ = self.events.send(PlaybackEvent::Error(msg));
        } else {
            self.state.set_path(Some(path.to_string()));
        }
    }

    fn play(&mut self) {
        if let Err(err) = self.handler.set_property(PROP_PAUSE, false) {
            let _ = self.events.send(PlaybackEvent::Error(err.to_string()));
        } else {
            self.state.set_paused(false);
        }
    }

    fn pause(&mut self) {
        if let Err(err) = self.handler.set_property(PROP_PAUSE, true) {
            let _ = self.events.send(PlaybackEvent::Error(err.to_string()));
        } else {
            self.state.set_paused(true);
        }
    }

    fn toggle_pause(&mut self) {
        let target = !self.state.paused;
        if let Err(err) = self.handler.set_property(PROP_PAUSE, target) {
            let _ = self.events.send(PlaybackEvent::Error(err.to_string()));
        } else {
            self.state.set_paused(target);
        }
    }

    fn stop(&mut self) {
        let _ = self.handler.set_property(PROP_PAUSE, true);
        let _ = self.handler.command(&[CMD_SEEK, SEEK_TO_START, SEEK_MODE_ABSOLUTE]);
    }

    fn seek(&mut self, seconds: f64) {
        if let Err(err) = self.seek_to(seconds) {
            let _ = self.events.send(PlaybackEvent::Error(err.to_string()));
        }
    }

    fn seek_to(&mut self, seconds: f64) -> mpv::Result<()> {
        let arg = format!("{seconds}");
        self.handler.command(&[CMD_SEEK, &arg, SEEK_MODE_ABSOLUTE])
    }

    fn set_audio_device(&mut self, id: &str) {
        let full_id =
            if id.starts_with(AUDIO_PREFIX_PIPEWIRE) || id.starts_with(AUDIO_PREFIX_PULSE) || id.starts_with(AUDIO_PREFIX_ALSA) {
                id.to_string()
            } else {
                format!("{}{id}", AUDIO_PREFIX_PIPEWIRE)
            };
        if let Err(err) = self.handler.set_property(PROP_AUDIO_DEVICE, full_id.as_str()) {
            let _ = self.events.send(PlaybackEvent::Error(err.to_string()));
        } else {
            self.audio_device = Some(id.to_string());
        }
    }

    fn apply_transition(&mut self) {
        let _ = self
            .handler
            .command(&[CMD_AF, &format!("fade in:st=0:d={}", eng::TRANSITION_SECONDS)]);
        let _ = self
            .handler
            .command(&[CMD_VF, &format!("fade in:st=0:d={}", eng::TRANSITION_SECONDS)]);
    }
}

/** Entry point for the mpv engine thread. */
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

/** Spawns the mpv engine thread. */
pub fn spawn_mpv_engine(
    cmd_rx: Receiver<PlaybackCmd>,
    events: Sender<PlaybackEvent>,
) -> thread::JoinHandle<()> {
    thread::Builder::new()
        .name(eng::THREAD_NAME.into())
        .spawn(move || run_mpv_engine(cmd_rx, events))
        .expect("mpv engine thread must start")
}

// LC_NUMERIC (POSIX).
unsafe extern "C" {
    fn setlocale(category: i32, locale: *const u8) -> *mut u8;
}
