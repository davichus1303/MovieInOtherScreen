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
            let _ = events.send(PlayerEvent::PlaybackError(format!(
                "No se pudo inicializar el reproductor: {err}"
            )));
            return;
        }
    };

    for command in commands {
        match command {
            PlayerCommand::Load(path) => player.load(&path),
            PlayerCommand::Play => player.play(),
            PlayerCommand::Pause => player.pause(),
            PlayerCommand::Stop => player.stop(),
            PlayerCommand::Seek(seconds) => player.seek(seconds),
            PlayerCommand::TogglePause => player.toggle_pause(),
            PlayerCommand::SetAudioDevice(id) => player.set_audio_device(&id),
            PlayerCommand::ListAudioDevices => player.publish_audio_devices(),
            PlayerCommand::Shutdown => break,
        }
    }
}

/// Sesión homogénea sobre la instancia única de mpv.
struct MpvSession {
    handler: mpv::MpvHandler,
    events: Sender<PlayerEvent>,
    paused: bool,
}

impl MpvSession {
    fn new(events: &Sender<PlayerEvent>) -> mpv::Result<Self> {
        let mut builder = mpv::MpvHandlerBuilder::new()?;
        // Priorizar aceleración por hardware cuando esté disponible.
        builder.try_hardware_decoding()?;
        builder.set_option("keep-open", "yes")?;
        // Embeber la salida en un GLArea de la app (Celluloid-style) en lugar
        // de abrir la ventana propia de mpv.
        builder.set_option("vo", "libmpv")?;
        let handler = builder.build()?;

        // Exponer el handle al renderer embebido de la UI.
        let _ = RENDER_HANDLE
            .set(Mutex::new(Some(MpvHandle(handler.raw() as super::ffi::mpv_handle))));

        let session = Self {
            handler,
            events: events.clone(),
            paused: false,
        };
        Ok(session)
    }

    /// Publica la lista de dispositivos de audio hacia la UI. La consulta usa
    /// FFI directo porque `audio-device-list` es un nodo mpv, no soportado por
    /// el crate `mpv`.
    fn publish_audio_devices(&self) {
        let handle = self.handler.raw().cast();
        match super::ffi::audio_devices(handle) {
            Ok(Some(devices)) => {
                let _ = self.events.send(PlayerEvent::AudioDevices(devices));
            }
            Ok(None) => {
                let _ = self.events.send(PlayerEvent::AudioDevices(Vec::new()));
            }
            Err(msg) => {
                let _ = self.events.send(PlayerEvent::AudioDevices(Vec::new()));
                let _ = self.events.send(PlayerEvent::PlaybackError(format!(
                    "No se pudo enumerar los dispositivos de audio: {msg}"
                )));
            }
        }
    }

    fn load(&mut self, path: &str) {
        self.apply_transition_into();
        let _ = self.handler.command(&["loadfile", path]);
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

    fn set_audio_device(&mut self, id: &str) {
        if let Err(err) = self.handler.set_property("audio-device", id) {
            self.report_error(err);
        }
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
        let _ = self.handler.command(&[
            "af",
            &format!("fade in:st=0:d={TRANSITION_SECONDS}"),
        ]);
        let _ = self.handler.command(&[
            "vf",
            &format!("fade in:st=0:d={TRANSITION_SECONDS}"),
        ]);
    }

    fn report_error(&self, err: mpv::Error) {
        let _ = self
            .events
            .send(PlayerEvent::PlaybackError(format!("{err}")));
    }
}

/// Reduce la lista bruta de `audio-device-list` a los **sinks funcionales**
/// reales, presentados y nombrados como en las herramientas rápidas de GNOME
/// (ajuste de sonido / `wpctl`).
///
/// `audio-device-list` de mpv enumera todos los backends de audio del sistema
/// (alsa, pipewire, pulse, jack, sdl, plughw, dmix, sysdefault...). La mayoría
/// no son salidas utilizables por el usuario: lo son los sinks reales, que
/// mpv expone bajo el backend `pipewire/` (o `pulse/` como respaldo) con un
/// id de la forma `alsa_output.<tarjeta>.<ruta>_sink`.
///
/// Reglas:
/// - Conserva únicamente los sinks reales: ids que contienen `alsa_output` y
///   terminan en `_sink`. Es la misma lista que muestra GNOME.
/// - Deduplica: cada sink aparece dos veces (backend `pipewire/` y `pulse/`);
///   se prefiere el `pipewire/` (el valor más moderno en PipeWire), y si no lo
///   hubiera se usa el `pulse/` como respaldo.
/// - Nombra con una etiqueta corta, sin el prefijo de tarjeta redundante que
///   GNOME oculta (p. ej. "HDMI / DisplayPort 1 Output" en vez de "Tiger
///   Lake-LP Smart Sound Technology Audio Controller HDMI / ... ").
///
/// Devuelve `(id, etiqueta)`; `id` es el valor completo que mpv acepta en
/// `audio-device`.
pub fn functional_sinks(devices: &[(String, String)]) -> Vec<(String, String)> {
    let backend = if devices
        .iter()
        .any(|(id, _)| id.starts_with("pipewire/"))
    {
        "pipewire/"
    } else {
        "pulse/"
    };

    devices
        .iter()
        .filter_map(|(id, desc)| {
            let Some(slug) = id.strip_prefix(backend) else {
                return None;
            };
            if !(slug.contains("alsa_output") && slug.ends_with("_sink")) {
                return None;
            }
            Some((id.clone(), short_label(desc)))
        })
        .collect()
}

/// Quita el prefijo de tarjeta/controlador, dejando el puerto de salida.
fn short_label(desc: &str) -> String {
    let desc = desc.trim();
    for marker in [
        " Audio Controller ",
        " Digital Audio ",
        " Audio ",
    ] {
        if let Some(pos) = desc.find(marker) {
            let after = desc[pos + marker.len()..].trim();
            if !after.is_empty() {
                return after.to_string();
            }
        }
    }
    desc.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dev(id: &str, desc: &str) -> (String, String) {
        (id.to_string(), desc.to_string())
    }

    #[test]
    fn filtra_solo_sinks_reales_y_deduplica_pipewire() {
        let raw = vec![
            dev("auto", "Autoselect device"),
            dev(
                "pipewire/alsa_output.pci-0000_00_1f.3-platform-skl_hda_dsp_generic.HiFi__hw_sofhdadsp_5__sink",
                "Tiger Lake-LP Smart Sound Technology Audio Controller HDMI / DisplayPort 3 Output",
            ),
            dev(
                "pipewire/alsa_output.pci-0000_00_1f.3-platform-skl_hda_dsp_generic.HiFi__hw_sofhdadsp_3__sink",
                "Tiger Lake-LP Smart Sound Technology Audio Controller HDMI / DisplayPort 1 Output",
            ),
            dev(
                "pipewire/alsa_output.pci-0000_00_1f.3-platform-skl_hda_dsp_generic.HiFi__hw_sofhdadsp__sink",
                "Tiger Lake-LP Smart Sound Technology Audio Controller Speaker + Headphones",
            ),
            // Duplicado pulse del mismo sink (3) -> debe descartarse
            dev(
                "pulse/alsa_output.pci-0000_00_1f.3-platform-skl_hda_dsp_generic.HiFi__hw_sofhdadsp_3__sink",
                "Tiger Lake-LP Smart Sound Technology Audio Controller HDMI / DisplayPort 1 Output",
            ),
            // Backends no-funcionales -> deben descartarse
            dev("alsa/sysdefault:CARD=sofhdadsp", "sof-hda-dsp /Default Audio Device"),
            dev("alsa/dmix:CARD=sofhdadsp,DEV=0", "Direct sample mixing device"),
            dev("jack", "Default (jack)"),
        ];

        let result = functional_sinks(&raw);

        assert_eq!(result.len(), 3, "debe quedar un areglo por sink: {result:?}");
        assert!(
            result.iter().all(|(id, _)| id.starts_with("pipewire/")),
            "se prefiere pipewire: {result:?}"
        );
        assert!(
            result.iter().all(|(_, label)| !label.contains("Audio Controller")),
            "etiqueta corta: {result:?}"
        );
        assert_eq!(result[0].1, "HDMI / DisplayPort 3 Output");
        assert_eq!(result[1].1, "HDMI / DisplayPort 1 Output");
        assert_eq!(result[2].1, "Speaker + Headphones");
    }

    #[test]
    fn sin_pipewire_usa_pulse_como_respaldo() {
        let raw = vec![
            dev(
                "pulse/alsa_output.pci._sink",
                "Some Controller Speaker",
            ),
            dev("alsa/plughw:CARD=x", "Hardware"),
        ];
        let result = functional_sinks(&raw);
        assert_eq!(result.len(), 1);
        assert!(result[0].0.starts_with("pulse/"));
    }
}

#[cfg(test)]
mod real_data_tests {
    use super::*;

    #[test]
    fn datos_reales_del_sistema_filtran_a_4_sinks() {
        // Lista real de `audio-device-list` de este equipo (probe mpvtest):
        // solo los sinks del hardware deben quedar, con etiqueta corta.
        let raw: Vec<(String, String)> = vec![
            ("auto".into(), "Autoselect device".into()),
            ("pipewire".into(), "Default (pipewire)".into()),
            ("pipewire/alsa_output.pci-0000_00_1f.3-platform-skl_hda_dsp_generic.HiFi__hw_sofhdadsp_5__sink".into(), "Tiger Lake-LP Smart Sound Technology Audio Controller HDMI / DisplayPort 3 Output".into()),
            ("pipewire/alsa_output.pci-0000_00_1f.3-platform-skl_hda_dsp_generic.HiFi__hw_sofhdadsp_4__sink".into(), "Tiger Lake-LP Smart Sound Technology Audio Controller HDMI / DisplayPort 2 Output".into()),
            ("pipewire/alsa_output.pci-0000_00_1f.3-platform-skl_hda_dsp_generic.HiFi__hw_sofhdadsp_3__sink".into(), "Tiger Lake-LP Smart Sound Technology Audio Controller HDMI / DisplayPort 1 Output".into()),
            ("pipewire/alsa_output.pci-0000_00_1f.3-platform-skl_hda_dsp_generic.HiFi__hw_sofhdadsp__sink".into(), "Tiger Lake-LP Smart Sound Technology Audio Controller Speaker + Headphones".into()),
            ("pulse/alsa_output.pci-0000_00_1f.3-platform-skl_hda_dsp_generic.HiFi__hw_sofhdadsp_5__sink".into(), "Tiger Lake-LP Smart Sound Technology Audio Controller HDMI / DisplayPort 3 Output".into()),
            ("pulse/alsa_output.pci-0000_00_1f.3-platform-skl_hda_dsp_generic.HiFi__hw_sofhdadsp_4__sink".into(), "Tiger Lake-LP Smart Sound Technology Audio Controller HDMI / DisplayPort 2 Output".into()),
            ("pulse/alsa_output.pci-0000_00_1f.3-platform-skl_hda_dsp_generic.HiFi__hw_sofhdadsp_3__sink".into(), "Tiger Lake-LP Smart Sound Technology Audio Controller HDMI / DisplayPort 1 Output".into()),
            ("pulse/alsa_output.pci-0000_00_1f.3-platform-skl_hda_dsp_generic.HiFi__hw_sofhdadsp__sink".into(), "Tiger Lake-LP Smart Sound Technology Audio Controller Speaker + Headphones".into()),
            ("alsa".into(), "Default (alsa)".into()),
            ("alsa/lavrate".into(), "Rate Converter Plugin Using Libav/FFmpeg Library".into()),
            ("alsa/plughw:CARD=sofhdadsp,DEV=3".into(), "sof-hda-dsp, Sceptre F24/Hardware device with all software conversions".into()),
            ("alsa/dmix:CARD=sofhdadsp,DEV=0".into(), "Direct sample mixing device".into()),
            ("alsa/sysdefault:CARD=sofhdadsp".into(), "Default Audio Device".into()),
            ("jack".into(), "Default (jack)".into()),
            ("sdl".into(), "Default (sdl)".into()),
        ];

        let result = functional_sinks(&raw);

        let labels: Vec<&str> = result.iter().map(|(_, l)| l.as_str()).collect();
        assert_eq!(result.len(), 4, "solo 4 sinks reales: {labels:?}");
        assert_eq!(result[0].1, "HDMI / DisplayPort 3 Output");
        assert_eq!(result[1].1, "HDMI / DisplayPort 2 Output");
        assert_eq!(result[2].1, "HDMI / DisplayPort 1 Output");
        assert_eq!(result[3].1, "Speaker + Headphones");
        assert!(result.iter().all(|(id, _)| id.starts_with("pipewire/")));
    }
}
