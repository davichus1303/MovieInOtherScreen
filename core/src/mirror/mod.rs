//! Motor de espejos: lógica pura para gestionar réplicas de reproducción.
//!
//! Esta capa NO depende de GTK. Define la lógica de negocio para:
//! - Sincronizar monitores seleccionados con reproducción
//! - Detectar cambios de video y propagarlos
//! - Gestionar estado de espejos (abiertos, path cargado, posición)

use std::collections::HashMap;
use std::sync::mpsc::Sender;

/// Comandos que el motor de espejos entiende.
#[derive(Debug, Clone)]
pub enum MirrorCmd {
    /// Carga un archivo (y salta a `pos` si es `Some`).
    Load(String, Option<f64>),
    Play,
    Pause,
    Seek(f64),
    Shutdown,
}

/// Estado de una ventana de espejo individual.
#[derive(Debug)]
struct MirrorWindow {
    is_loaded: bool,
}

/// Estado interno del motor de espejos (lógica pura, sin GTK).
#[derive(Debug, Default)]
pub struct MirrorEngine {
    /// Path actualmente reproducido en el reproductor principal.
    current_path: Option<String>,
    /// Path que ya se cargó en los espejos abiertos.
    loaded_path: Option<String>,
    /// Monitores seleccionados actualmente (id -> Monitor).
    selected_monitors: HashMap<String, crate::monitors::Monitor>,
    /// Ventanas de espejo abiertas (id -> estado).
    windows: HashMap<String, MirrorWindow>,
    /// Sender hacia los hilos de espejos (inyección de dependencia).
    cmd_tx: Option<Sender<MirrorCmd>>,
}

impl MirrorEngine {
    pub fn new() -> Self {
        Self::default()
    }

    /// Inyecta el canal de comandos hacia los hilos de espejos.
    pub fn set_command_sender(&mut self, tx: Sender<MirrorCmd>) {
        self.cmd_tx = Some(tx);
    }

    /// Actualiza la selección de monitores.
    pub fn set_selected_monitors(&mut self, monitors: Vec<crate::monitors::Monitor>) {
        self.selected_monitors = monitors
            .into_iter()
            .map(|m| (m.id().to_string(), m))
            .collect();
    }

    /// Notifica que el reproductor principal va a reproducir `path`.
    /// Actualiza `current_path` y recarga espejos si el video cambió.
    pub fn set_playing(&mut self, path: String) {
        self.current_path = Some(path);
    }

    /// Reconcilia espejos con la selección actual.
    /// `pos_base`: posición (segundos) para alinear espejos nuevos.
    pub fn reconfigure(&mut self, pos_base: Option<f64>) {
        let current_path = self.current_path.clone();
        let Some(path) = current_path else { return };

        let video_changed = self.path_changed();

        // Cerrar espejos que ya no están seleccionados
        let stale: Vec<String> = self
            .windows
            .keys()
            .filter(|id| !self.selected_monitors.contains_key(id.as_str()))
            .cloned()
            .collect();
        for id in stale {
            self.remove_window(&id);
        }

        // Cargar video en espejos nuevos, y en TODOS si el video cambió
        for id in self.selected_monitors.keys() {
            if let Some(w) = self.windows.get_mut(id) {
                if video_changed || !w.is_loaded {
                    if let Some(tx) = &self.cmd_tx {
                        let _ = tx.send(MirrorCmd::Load(path.clone(), pos_base));
                    }
                    w.is_loaded = true;
                }
            } else {
                // Espejo nuevo: se abriría aquí con pos_base
                // (la apertura real de ventana es responsabilidad de la UI)
                self.windows
                    .insert(id.clone(), MirrorWindow { is_loaded: true });
                if let Some(tx) = &self.cmd_tx {
                    let _ = tx.send(MirrorCmd::Load(path.clone(), pos_base));
                }
            }
        }

        self.mark_path_loaded();

        // Play en todos
        if let Some(tx) = &self.cmd_tx {
            for _ in self.windows.values() {
                let _ = tx.send(MirrorCmd::Play);
            }
        }
    }

    fn path_changed(&self) -> bool {
        self.current_path.as_ref() != self.loaded_path.as_ref()
    }

    fn mark_path_loaded(&mut self) {
        self.loaded_path = self.current_path.clone();
    }

    fn remove_window(&mut self, id: &str) {
        if let Some(tx) = &self.cmd_tx {
            let _ = tx.send(MirrorCmd::Shutdown);
        }
        self.windows.remove(id);
    }
}

/// Trait para la UI: abstrae la creación/gestión de ventanas de espejo.
pub trait MirrorWindowManager {
    fn open_mirror(&mut self, id: &str, monitor: &crate::monitors::Monitor) -> Result<(), String>;
    fn close_mirror(&mut self, id: &str);
    fn is_open(&self, id: &str) -> bool;
}
