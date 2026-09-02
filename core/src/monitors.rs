//! Modelo lógico de monitores: detección y selección de destinos.
//!
//! La detección real (GDK/Wayland) ocurre en la capa de plataforma; esta
//! capa mantiene el estado y las reglas de selección, y es probable de forma
//! aislada.

/// Tipo de monitor.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MonitorKind {
    /// Monitor principal (donde vive la interfaz).
    Primary,
    /// Monitor adicional, posible destino de reproducción.
    Secondary,
}

/// Un monitor detectado.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Monitor {
    /// Identificador estable del monitor (p. ej. `eDP-1`).
    id: String,
    /// Nombre de presentación (p. ej. "Monitor 2").
    label: String,
    kind: MonitorKind,
    selected: bool,
}

impl Monitor {
    pub fn new(id: String, label: String, kind: MonitorKind) -> Self {
        Self {
            id,
            label,
            kind,
            selected: false,
        }
    }

    pub fn id(&self) -> &str {
        &self.id
    }

    pub fn label(&self) -> &str {
        &self.label
    }

    pub fn kind(&self) -> MonitorKind {
        self.kind
    }

    pub fn is_primary(&self) -> bool {
        self.kind == MonitorKind::Primary
    }

    pub fn is_selected(&self) -> bool {
        self.selected
    }
}

/// Una regla de negocio: solo los monitores adicionales pueden ser destinos.
impl Monitor {
    /// Determina si este monitor puede seleccionarse como destino.
    pub fn can_be_target(&self) -> bool {
        !self.is_primary()
    }
}

/// Conjunto de monitores detectados en el sistema.
#[derive(Debug, Clone, Default)]
pub struct MonitorSet {
    monitors: Vec<Monitor>,
}

impl MonitorSet {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn iter(&self) -> impl Iterator<Item = &Monitor> {
        self.monitors.iter()
    }

    pub fn len(&self) -> usize {
        self.monitors.len()
    }

    pub fn is_empty(&self) -> bool {
        self.monitors.is_empty()
    }

    /// Reemplaza el conjunto con el estado detectado por la plataforma.
    ///
    /// La selección previa se conserva solo para los monitores que siguen
    /// conectados; los desconectados se descartan.
    pub fn update_from_detected(&mut self, detected: Vec<Monitor>) {
        let was_selected: std::collections::HashSet<String> = self
            .monitors
            .iter()
            .filter(|m| m.is_selected())
            .map(|m| m.id().to_string())
            .collect();

        self.monitors = detected
            .into_iter()
            .map(|mut m| {
                if was_selected.contains(m.id()) {
                    m.selected = true;
                }
                m
            })
            .collect();
    }

    /// Monitores adicionales (posibles destinos), en orden de detección.
    pub fn secondaries(&self) -> impl Iterator<Item = &Monitor> {
        self.monitors.iter().filter(|m| m.is_secondary_for_target())
    }

    /// Monitores adicionales actualmente seleccionados.
    pub fn selected(&self) -> impl Iterator<Item = &Monitor> {
        self.monitors.iter().filter(|m| m.is_selected())
    }

    /// Número de monitores adicionales seleccionados.
    pub fn selected_count(&self) -> usize {
        self.selected().count()
    }

    /// Conmuta la selección de un monitor adicional.
    ///
    /// Devuelve `false` si el monitor no existe o es el principal.
    pub fn toggle(&mut self, id: &str) -> bool {
        let Some(m) = self.monitors.iter_mut().find(|m| m.id() == id) else {
            return false;
        };
        if !m.can_be_target() {
            return false;
        }
        m.selected = !m.selected;
        true
    }

    pub fn get(&self, id: &str) -> Option<&Monitor> {
        self.monitors.iter().find(|m| m.id() == id)
    }

    /// Compara si el conjunto tiene algún monitor adicional (hubo monitores
    /// además del principal).
    pub fn has_secondaries(&self) -> bool {
        self.monitors.iter().any(|m| m.is_secondary_for_target())
    }

    /// Compara si no hay ningún monitor adicional (solo el principal).
    pub fn only_primary(&self) -> bool {
        !self.has_secondaries()
    }
}

// Helpers internos para separar el razonamiento de `is_selected`.
impl Monitor {
    fn is_secondary_for_target(&self) -> bool {
        self.can_be_target()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn m(id: &str, label: &str, kind: MonitorKind) -> Monitor {
        Monitor::new(id.to_string(), label.to_string(), kind)
    }

    #[test]
    fn un_solo_monitor_no_tiene_secundarios() {
        let set = MonitorSet {
            monitors: vec![m("eDP-1", "Monitor 1", MonitorKind::Primary)],
        };
        assert!(set.only_primary());
        assert!(!set.has_secondaries());
    }

    #[test]
    fn dos_monitores_un_secundario() {
        let set = MonitorSet {
            monitors: vec![
                m("eDP-1", "Monitor 1", MonitorKind::Primary),
                m("HDMI-A-1", "Monitor 2", MonitorKind::Secondary),
            ],
        };
        assert!(set.has_secondaries());
        assert_eq!(set.secondaries().count(), 1);
    }

    #[test]
    fn tres_monitores_dos_secundarios() {
        let set = MonitorSet {
            monitors: vec![
                m("eDP-1", "Monitor 1", MonitorKind::Primary),
                m("HDMI-A-1", "Monitor 2", MonitorKind::Secondary),
                m("DP-1", "Monitor 3", MonitorKind::Secondary),
            ],
        };
        assert_eq!(set.secondaries().count(), 2);
    }

    #[test]
    fn el_principal_no_puede_seleccionarse() {
        let mut set = MonitorSet {
            monitors: vec![m("eDP-1", "Monitor 1", MonitorKind::Primary)],
        };
        assert!(!set.toggle("eDP-1"));
        assert_eq!(set.selected_count(), 0);
    }

    #[test]
    fn seleccion_y_deseleccion_de_secundario() {
        let mut set = MonitorSet {
            monitors: vec![
                m("eDP-1", "Monitor 1", MonitorKind::Primary),
                m("HDMI-A-1", "Monitor 2", MonitorKind::Secondary),
            ],
        };
        assert!(set.toggle("HDMI-A-1"));
        assert_eq!(set.selected_count(), 1);
        assert!(set.toggle("HDMI-A-1"));
        assert_eq!(set.selected_count(), 0);
    }

    #[test]
    fn varios_secundarios_seleccionados() {
        let mut set = MonitorSet {
            monitors: vec![
                m("eDP-1", "Monitor 1", MonitorKind::Primary),
                m("HDMI-A-1", "Monitor 2", MonitorKind::Secondary),
                m("DP-1", "Monitor 3", MonitorKind::Secondary),
            ],
        };
        set.toggle("HDMI-A-1");
        set.toggle("DP-1");
        assert_eq!(set.selected_count(), 2);
    }

    #[test]
    fn monitor_desconectado_se_pierde_seleccion() {
        let mut set = MonitorSet {
            monitors: Vec::new(),
        };
        set.update_from_detected(vec![
            m("eDP-1", "Monitor 1", MonitorKind::Primary),
            m("HDMI-A-1", "Monitor 2", MonitorKind::Secondary),
        ]);
        set.toggle("HDMI-A-1");
        assert_eq!(set.selected_count(), 1);

        set.update_from_detected(vec![m("eDP-1", "Monitor 1", MonitorKind::Primary)]);
        assert_eq!(set.selected_count(), 0);
    }

    #[test]
    fn reconexion_conserva_seleccion() {
        let mut set = MonitorSet {
            monitors: Vec::new(),
        };
        set.update_from_detected(vec![
            m("eDP-1", "Monitor 1", MonitorKind::Primary),
            m("HDMI-A-1", "Monitor 2", MonitorKind::Secondary),
        ]);
        set.toggle("HDMI-A-1");
        set.update_from_detected(vec![
            m("eDP-1", "Monitor 1", MonitorKind::Primary),
            m("HDMI-A-1", "Monitor 2", MonitorKind::Secondary),
        ]);
        assert_eq!(set.selected_count(), 1);
    }
}
