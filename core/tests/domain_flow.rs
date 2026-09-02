//! Tests de integración de la capa de dominio: varios módulos colaborando.
//!
//! Ejerce un flujo realista de la aplicación a nivel de lógica (sin GTK):
//! navegación de vídeos + control de reproducción + selección de monitores +
//! selección de audio + persistencia de configuración.

use mos_core::audio::{AudioDevice, AudioDevices};
use mos_core::config::{self, keys};
use mos_core::monitors::{Monitor, MonitorKind, MonitorSet};
use mos_core::player::{Command, PlayState, PlaybackController, PlaybackNav, VideoListRef};
use mos_core::segments::Segment;
use mos_core::video_list::{Video, VideoList};
use std::path::PathBuf;

fn video(name: &str) -> Video {
    Video::new(PathBuf::from(name))
}

fn monitor(id: &str, label: &str, kind: MonitorKind) -> Monitor {
    Monitor::new(id.to_string(), label.to_string(), kind)
}

#[test]
fn flujo_completo_navegacion_y_busqueda() {
    let mut list = VideoList::new();
    list.add(vec![video("a.mp4"), video("b.mp4"), video("c.mp4")]);
    list.select(1);
    assert_eq!(list.selected().map(|v| v.name()), Some("b.mp4"));

    // La barra de progreso (segmentos) siempre produce una posición válida.
    let segment = Segment::try_from(7).unwrap();
    assert!((segment.position() - 0.6).abs() < f64::EPSILON);

    // El reproductor interpreta el comando "Siguiente" y avanza.
    let mut control = PlaybackController::new();
    control.handoff(1, PlayState::Playing);
    assert_eq!(
        control.apply(Command::Next, &VideoListRef::new(3)),
        PlaybackNav::Changed
    );
    assert_eq!(control.current_index(), Some(2));

    // En el último vídeo, "Siguiente" no hace nada.
    assert_eq!(
        control.apply(Command::Next, &VideoListRef::new(3)),
        PlaybackNav::Impossible
    );
    assert_eq!(control.current_index(), Some(2));
}

#[test]
fn flujo_completo_monitores_y_audio_persisten() {
    // Detección de monitores: principal + dos secundarios.
    let mut monitors = MonitorSet::new();
    monitors.update_from_detected(vec![
        monitor("eDP-1", "Monitor 1", MonitorKind::Primary),
        monitor("HDMI-A-1", "Monitor 2", MonitorKind::Secondary),
        monitor("DP-1", "Monitor 3", MonitorKind::Secondary),
    ]);
    assert_eq!(monitors.selected_count(), 0);

    // El principal no es destino; los secundarios sí.
    assert!(!monitors.toggle("eDP-1"));
    assert!(monitors.toggle("HDMI-A-1"));
    assert!(monitors.toggle("DP-1"));
    assert_eq!(monitors.selected_count(), 2);

    // Selección de audio + persistencia.
    let mut audio = AudioDevices::new();
    audio.update_detected(vec![
        AudioDevice::new("speakers".into(), "Speakers".into(), true),
        AudioDevice::new("hdmi".into(), "HDMI".into(), false),
    ]);
    audio.select("hdmi");
    assert!(audio.is_active("hdmi"));

    // Guardar la preferencia y restaurarla en una instancia nueva.
    let mut config = mos_core::config::Config::new();
    config.set(keys::AUDIO_DEVICE, audio.preference_for_storage().unwrap());

    let dir = tempfile::TempDir::new().unwrap();
    let path = dir.path().join("app.conf");
    config::save_to(&path, &config).unwrap();

    let loaded = config::load_from(&path).unwrap();
    let mut restored = AudioDevices::new();
    restored.restore_preference(loaded.get(keys::AUDIO_DEVICE).map(str::to_string));

    // Al redetectar, la preferencia vuelve a aplicarse.
    restored.update_detected(vec![
        AudioDevice::new("speakers".into(), "Speakers".into(), true),
        AudioDevice::new("hdmi".into(), "HDMI".into(), false),
    ]);
    assert!(restored.is_active("hdmi"));
}

#[test]
fn flujo_un_solo_monitor_funciona_sin_secundarios() {
    let mut monitors = MonitorSet::new();
    monitors.update_from_detected(vec![monitor("eDP-1", "Monitor 1", MonitorKind::Primary)]);
    assert!(monitors.only_primary());
    assert!(!monitors.has_secondaries());
}
