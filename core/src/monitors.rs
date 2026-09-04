/*! Logical monitor model: detection and destination selection.
 *
 * Actual detection (GDK/Wayland) happens in the platform layer; this
 * layer maintains state and selection rules, and is testable in isolation.
 */

/** Monitor type. */
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MonitorKind {
    /** Primary monitor (where the interface lives). */
    Primary,
    /** Additional monitor, possible playback destination. */
    Secondary,
}

/** A detected monitor. */
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Monitor {
    /** Stable monitor identifier (e.g. `eDP-1`). */
    id: String,
    /** Display name (e.g. "Monitor 2"). */
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

/** A business rule: only additional monitors can be destinations. */
impl Monitor {
    /** Determines whether this monitor can be selected as a destination. */
    pub fn can_be_target(&self) -> bool {
        !self.is_primary()
    }
}

/** Set of monitors detected on the system. */
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

    /**
     * Replaces the set with the platform-detected state.
     *
     * Previous selection is preserved only for monitors that are still
     * connected; disconnected ones are discarded.
     */
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

    /** Additional monitors (possible destinations), in detection order. */
    pub fn secondaries(&self) -> impl Iterator<Item = &Monitor> {
        self.monitors.iter().filter(|m| m.can_be_target())
    }

    /** Currently selected additional monitors. */
    pub fn selected(&self) -> impl Iterator<Item = &Monitor> {
        self.monitors.iter().filter(|m| m.is_selected())
    }

    /** Number of selected additional monitors. */
    pub fn selected_count(&self) -> usize {
        self.selected().count()
    }

    /**
     * Toggles the selection of an additional monitor.
     *
     * Returns `false` if the monitor does not exist or is the primary one.
     */
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

    /** Checks whether the set has any additional monitors (beyond the primary). */
    pub fn has_secondaries(&self) -> bool {
        self.monitors.iter().any(|m| m.can_be_target())
    }

    /** Checks whether there are no additional monitors (only the primary). */
    pub fn only_primary(&self) -> bool {
        !self.has_secondaries()
    }

    /**
     * Marks the monitor `id` as the principal and the rest as secondary.
     *
     * Used when the user picks the screen that acts as the interface monitor.
     * The new principal stops being a destination (its selection is cleared),
     * while every other monitor becomes a possible destination again. Returns
     * `false` if `id` does not belong to the set.
     */
    pub fn set_primary(&mut self, id: &str) -> bool {
        if !self.monitors.iter().any(|m| m.id() == id) {
            return false;
        }
        for m in self.monitors.iter_mut() {
            if m.id() == id {
                m.kind = MonitorKind::Primary;
                m.selected = false;
            } else {
                m.kind = MonitorKind::Secondary;
            }
        }
        true
    }
}

// Helpers internos para separar el razonamiento de `is_selected`.

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

    // --- Tests de regresión del bug de sincronización ---
    //
    // El bug: al desactivar/activar una pantalla la sincronización se perdía
    // porque distintas partes calculaban los "destinos" con criterios
    // diferentes. La regla única es `can_be_target()` (no ser primario).
    // Estos tests garantizan que TODAS las rutas usen el mismo criterio.

    #[test]
    fn secondaries_nunca_incluye_al_primario() {
        let set = MonitorSet {
            monitors: vec![
                m("eDP-1", "Monitor 1", MonitorKind::Primary),
                m("HDMI-A-1", "Monitor 2", MonitorKind::Secondary),
                m("DP-1", "Monitor 3", MonitorKind::Secondary),
            ],
        };
        // secondaries() nunca debe emitir el monitor primario.
        let ids: Vec<&str> = set.secondaries().map(Monitor::id).collect();
        assert_eq!(ids, ["HDMI-A-1", "DP-1"]);
        assert!(!ids.contains(&"eDP-1"));
    }

    #[test]
    fn selected_solo_contiene_secundarios() {
        let mut set = MonitorSet {
            monitors: vec![
                m("eDP-1", "Monitor 1", MonitorKind::Primary),
                m("HDMI-A-1", "Monitor 2", MonitorKind::Secondary),
                m("DP-1", "Monitor 3", MonitorKind::Secondary),
            ],
        };
        set.toggle("HDMI-A-1");
        let selected_ids: Vec<&str> = set.selected().map(Monitor::id).collect();
        assert_eq!(selected_ids, ["HDMI-A-1"]);
        // El principal nunca aparece entre los seleccionados.
        assert!(!selected_ids.contains(&"eDP-1"));
    }

    #[test]
    fn ratio_seleccion_es_secundarios_seleccionados_sobre_secundarios() {
        // Es lo que la sync consume: si todo usa secondaries(), el ratio
        // de mirrors abiertos coincide con la selección.
        let mut set = MonitorSet {
            monitors: vec![
                m("eDP-1", "Monitor 1", MonitorKind::Primary),
                m("HDMI-A-1", "Monitor 2", MonitorKind::Secondary),
                m("DP-1", "Monitor 3", MonitorKind::Secondary),
                m("DP-2", "Monitor 4", MonitorKind::Secondary),
            ],
        };
        set.toggle("DP-1");
        set.toggle("DP-2");
        assert_eq!(set.secondaries().count(), 3);
        assert_eq!(set.selected_count(), 2);
        // 2 de 3 secundarios (el primario no cuenta como destino).
        assert_eq!(set.secondaries().count() - set.selected_count(), 1);
    }

    #[test]
    fn toggle_de_id_inexistente_devuelve_false() {
        let mut set = MonitorSet {
            monitors: vec![m("eDP-1", "Monitor 1", MonitorKind::Primary)],
        };
        assert!(!set.toggle("no-existe"));
        assert_eq!(set.selected_count(), 0);
    }

    #[test]
    fn primario_nunca_aparece_en_secondaries_ni_has_secondaries() {
        // Solo un primario: no hay secundarios aunque exista selección previa.
        let mut set = MonitorSet {
            monitors: Vec::new(),
        };
        set.update_from_detected(vec![m("eDP-1", "Monitor 1", MonitorKind::Primary)]);
        assert!(!set.has_secondaries());
        assert!(set.only_primary());
        assert_eq!(set.secondaries().count(), 0);
    }

    #[test]
    fn get_devuelve_el_monitor_por_id() {
        let set = MonitorSet {
            monitors: vec![
                m("eDP-1", "Monitor 1", MonitorKind::Primary),
                m("DP-1", "Monitor 3", MonitorKind::Secondary),
            ],
        };
        assert_eq!(set.get("DP-1").map(Monitor::label), Some("Monitor 3"));
        assert_eq!(set.get("HDMI"), None);
    }

    #[test]
    fn can_be_target_es_false_solo_para_primario() {
        let primary = m("eDP-1", "Monitor 1", MonitorKind::Primary);
        let secondary = m("DP-1", "Monitor 3", MonitorKind::Secondary);
        assert!(!primary.can_be_target());
        assert!(secondary.can_be_target());
    }

    // --- Tests of changing the principal monitor ---

    #[test]
    fn set_primary_marca_el_nuevo_y_desbloquea_el_resto() {
        let mut set = MonitorSet {
            monitors: vec![
                m("eDP-1", "Monitor 1", MonitorKind::Primary),
                m("HDMI-A-1", "Monitor 2", MonitorKind::Secondary),
            ],
        };
        assert!(set.set_primary("HDMI-A-1"));
        // The new one is primary and the old one becomes a target again.
        assert!(set.get("HDMI-A-1").unwrap().is_primary());
        assert!(set.get("eDP-1").unwrap().can_be_target());
        assert_eq!(set.secondaries().count(), 1);
        assert_eq!(set.get("eDP-1").map(Monitor::id), Some("eDP-1"));
    }

    #[test]
    fn set_primary_limpia_la_seleccion_del_nuevo_principal() {
        let mut set = MonitorSet {
            monitors: vec![
                m("eDP-1", "Monitor 1", MonitorKind::Primary),
                m("HDMI-A-1", "Monitor 2", MonitorKind::Secondary),
            ],
        };
        set.toggle("HDMI-A-1");
        assert_eq!(set.selected_count(), 1);
        assert!(set.set_primary("HDMI-A-1"));
        // The new principal stops being a target: it is no longer selected.
        assert_eq!(set.selected_count(), 0);
        assert!(set.get("HDMI-A-1").unwrap().is_primary());
    }

    #[test]
    fn set_primary_de_id_inexistente_devuelve_false() {
        let mut set = MonitorSet {
            monitors: vec![m("eDP-1", "Monitor 1", MonitorKind::Primary)],
        };
        assert!(!set.set_primary("no-existe"));
        assert!(set.get("eDP-1").unwrap().is_primary());
    }

    #[test]
    fn set_primary_cambiar_de_principal_preserva_un_solo_primario() {
        let mut set = MonitorSet {
            monitors: vec![
                m("eDP-1", "Monitor 1", MonitorKind::Primary),
                m("HDMI-A-1", "Monitor 2", MonitorKind::Secondary),
                m("DP-1", "Monitor 3", MonitorKind::Secondary),
            ],
        };
        set.set_primary("DP-1");
        let primarios = set.iter().filter(|m| m.is_primary()).count();
        assert_eq!(primarios, 1);
        assert!(set.get("DP-1").unwrap().is_primary());
        // Both former targets remain available.
        assert_eq!(set.secondaries().count(), 2);
    }
}
