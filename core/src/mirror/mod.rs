/*! Mirror engine: pure logic for managing playback replicas.
 *
 * This layer does NOT depend on GTK. It defines the business logic for:
 * - Synchronizing selected monitors with playback
 * - Detecting video changes and propagating them
 * - Managing mirror state (open, loaded path, position)
 */

use std::collections::HashMap;
use std::sync::mpsc::Sender;

/** Commands understood by the mirror engine. */
#[derive(Debug, Clone)]
pub enum MirrorCmd {
    /** Loads a file (and seeks to `pos` if `Some`). */
    Load(String, Option<f64>),
    Play,
    Pause,
    Seek(f64),
    Shutdown,
}

/** State of a single mirror window. */
#[derive(Debug)]
struct MirrorWindow {
    is_loaded: bool,
}

/** Internal state of the mirror engine (pure logic, no GTK). */
#[derive(Debug, Default)]
pub struct MirrorEngine {
    /** Path currently playing in the main player. */
    current_path: Option<String>,
    /** Path already loaded in open mirrors. */
    loaded_path: Option<String>,
    /** Currently selected monitors (id -> Monitor). */
    selected_monitors: HashMap<String, crate::monitors::Monitor>,
    /** Open mirror windows (id -> state). */
    windows: HashMap<String, MirrorWindow>,
    /** Sender to the mirror threads (dependency injection). */
    cmd_tx: Option<Sender<MirrorCmd>>,
}

impl MirrorEngine {
    pub fn new() -> Self {
        Self::default()
    }

    /** Injects the command channel to the mirror threads. */
    pub fn set_command_sender(&mut self, tx: Sender<MirrorCmd>) {
        self.cmd_tx = Some(tx);
    }

    /** Updates the monitor selection. */
    pub fn set_selected_monitors(&mut self, monitors: Vec<crate::monitors::Monitor>) {
        self.selected_monitors = monitors
            .into_iter()
            .map(|m| (m.id().to_string(), m))
            .collect();
    }

    /**
     * Notifies that the main player is about to play `path`.
     * Updates `current_path` and reloads mirrors if the video changed.
     */
    pub fn set_playing(&mut self, path: String) {
        self.current_path = Some(path);
    }

    /**
     * Reconciles mirrors with the current selection.
     * `pos_base`: position (seconds) to align new mirrors.
     */
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

/** Trait for the UI: abstracts the creation/management of mirror windows. */
pub trait MirrorWindowManager {
    fn open_mirror(&mut self, id: &str, monitor: &crate::monitors::Monitor) -> Result<(), String>;
    fn close_mirror(&mut self, id: &str);
    fn is_open(&self, id: &str) -> bool;
}
