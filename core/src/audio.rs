//! Modelo lógico de dispositivos de audio y su selección.
//!
//! La detección real (PipeWire/PulseAudio/libmpv) pertenece a la capa de
//! plataforma. Aquí se mantiene el estado de selección, la persistencia de la
//! preferencia y las reglas de comportamiento ante dispositivos que aparecen
//! o desaparecen.

/// Un dispositivo de salida de audio.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AudioDevice {
    /// Identificador estable (p. ej. nombre del nodo PipeWire).
    id: String,
    /// Nombre de presentación (p. ej. "Speakers").
    label: String,
    /// Si el sistema lo marca como salida por defecto.
    is_default: bool,
}

impl AudioDevice {
    pub fn new(id: String, label: String, is_default: bool) -> Self {
        Self {
            id,
            label,
            is_default,
        }
    }

    pub fn id(&self) -> &str {
        &self.id
    }

    pub fn label(&self) -> &str {
        &self.label
    }

    pub fn is_default(&self) -> bool {
        self.is_default
    }
}

/// Conjunto de dispositivos de audio disponibles.
///
/// Sus responsabilidades:
/// - Mantener la lista de dispositivos detectados.
/// - Resolver qué dispositivo está activo según la preferencia guardada y lo
///   disponible, sin que "conectar/desconectar" cambie la preferencia del
///   usuario.
#[derive(Debug, Clone, Default)]
pub struct AudioDevices {
    devices: Vec<AudioDevice>,
    /// Preferencia explícita del usuario (id guardado), si existe.
    preferred: Option<String>,
    /// Dispositivo activo resuelto.
    active: Option<String>,
}

impl AudioDevices {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn iter(&self) -> impl Iterator<Item = &AudioDevice> {
        self.devices.iter()
    }

    pub fn get(&self, id: &str) -> Option<&AudioDevice> {
        self.devices.iter().find(|d| d.id() == id)
    }

    pub fn len(&self) -> usize {
        self.devices.len()
    }

    pub fn is_empty(&self) -> bool {
        self.devices.is_empty()
    }

    /// Actualiza la lista de dispositivos detectados.
    ///
    /// La preferencia guardada nunca se modifica aquí. El dispositivo activo
    /// se recalcula con una regla segura:
    /// 1. Si la preferencia sigue disponible, es el activo.
    /// 2. Si no, se usa el dispositivo por defecto del sistema si existe.
    /// 3. Si no hay ninguno, queda inactivo (sin perder la preferencia).
    pub fn update_detected(&mut self, devices: Vec<AudioDevice>) {
        self.devices = devices;
        self.recompute_active();
    }

    /// Establece explícitamente la preferencia y el activo.
    ///
    /// Devuelve `false` si el dispositivo no existe en la lista.
    pub fn select(&mut self, id: &str) -> bool {
        if self.get(id).is_none() {
            return false;
        }
        self.preferred = Some(id.to_string());
        self.active = Some(id.to_string());
        true
    }

    /// Identificador del dispositivo actualmente activo, si existe.
    pub fn active_id(&self) -> Option<&str> {
        self.active.as_deref()
    }

    /// Preferencia guardada del usuario, si existe.
    pub fn preferred_id(&self) -> Option<&str> {
        self.preferred.as_deref()
    }

    /// El dispositivo activo, si existe.
    pub fn active(&self) -> Option<&AudioDevice> {
        self.active.as_deref().and_then(|id| self.get(id))
    }

    /// Seleccionar de nuevo el dispositivo ya activo no produce cambios (la
    /// preferencia y el activo quedan igual). Devuelve `false` si no existe.
    pub fn select_re_entrante(&mut self, id: &str) -> bool {
        if self.get(id).is_none() {
            return false;
        }
        if !self.is_active(id) {
            self.select(id);
        }
        true
    }

    pub fn is_active(&self, id: &str) -> bool {
        self.active.as_deref() == Some(id)
    }

    /// Serializa la preferencia para persistirla.
    pub fn preference_for_storage(&self) -> Option<String> {
        self.preferred.clone()
    }

    /// Restaura una preferencia guardada. No valida la existencia del
    /// dispositivo: eso lo hará `update_detected` al recalcular.
    pub fn restore_preference(&mut self, id: Option<String>) {
        self.preferred = id;
        self.recompute_active();
    }

    fn recompute_active(&mut self) {
        if let Some(id) = &self.preferred {
            if self.get(id).is_some() {
                self.active = Some(id.clone());
                return;
            }
        }
        self.active = self
            .devices
            .iter()
            .find(|d| d.is_default())
            .map(|d| d.id().to_string());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spk() -> AudioDevice {
        AudioDevice::new("default-output".into(), "Speakers".into(), true)
    }
    fn hdmi() -> AudioDevice {
        AudioDevice::new("hdmi-output".into(), "HDMI".into(), false)
    }
    fn bt() -> AudioDevice {
        AudioDevice::new("bt-output".into(), "Bluetooth".into(), false)
    }

    #[test]
    fn deteccion_inicial_usa_por_defecto() {
        let mut devices = AudioDevices::new();
        devices.update_detected(vec![spk(), hdmi()]);
        assert_eq!(devices.active_id(), Some("default-output"));
    }

    #[test]
    fn seleccion_activa_nuevo_dispositivo() {
        let mut devices = AudioDevices::new();
        devices.update_detected(vec![spk(), hdmi()]);
        assert!(devices.select("hdmi-output"));
        assert!(devices.is_active("hdmi-output"));
        assert!(!devices.is_active("default-output"));
    }

    #[test]
    fn conserva_preferencia_al_detectar_nuevos_dispositivos() {
        let mut devices = AudioDevices::new();
        devices.update_detected(vec![spk()]);
        devices.select("default-output");
        devices.update_detected(vec![spk(), hdmi(), bt()]);
        assert!(devices.is_active("default-output"));
        assert_eq!(devices.preferred_id(), Some("default-output"));
    }

    #[test]
    fn desaparece_temporalmente_dispositivo_seleccionado() {
        let mut devices = AudioDevices::new();
        devices.update_detected(vec![spk(), hdmi()]);
        devices.select("hdmi-output");
        // El HDMI desaparece de la detección.
        devices.update_detected(vec![spk()]);
        // El activo vuelve al por defecto, pero la preferencia no se pierde.
        assert_eq!(devices.active_id(), Some("default-output"));
        assert_eq!(devices.preferred_id(), Some("hdmi-output"));
        // Reaparece: la preferencia se restaura.
        devices.update_detected(vec![spk(), hdmi()]);
        assert_eq!(devices.active_id(), Some("hdmi-output"));
    }

    #[test]
    fn seleccionar_ya_activo_no_cambia_nada() {
        let mut devices = AudioDevices::new();
        devices.update_detected(vec![spk(), hdmi()]);
        devices.select("default-output");
        let before = devices.preferred_id().map(str::to_string);
        let active_before = devices.active_id().map(str::to_string);
        assert!(devices.select_re_entrante("default-output"));
        assert_eq!(devices.preferred_id().map(str::to_string), before);
        assert_eq!(devices.active_id().map(str::to_string), active_before);
    }

    #[test]
    fn seleccionar_dispositivo_inexistente_falla() {
        let mut devices = AudioDevices::new();
        devices.update_detected(vec![spk()]);
        assert!(!devices.select("no-existe"));
    }

    #[test]
    fn persistencia_round_trip() {
        let mut devices = AudioDevices::new();
        devices.update_detected(vec![spk(), hdmi()]);
        devices.select("hdmi-output");
        let stored = devices.preference_for_storage();

        let mut restored = AudioDevices::new();
        restored.restore_preference(stored);
        assert_eq!(restored.preferred_id(), Some("hdmi-output"));
        // Antes de detectar dispositivos no hay activo…
        assert_eq!(restored.active_id(), None);
        // …pero al detectar, la preferencia se aplica.
        restored.update_detected(vec![spk(), hdmi()]);
        assert!(restored.is_active("hdmi-output"));
    }
}
