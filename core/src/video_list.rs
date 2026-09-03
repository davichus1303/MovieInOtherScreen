/*! Video model and its list.
 *
 * This layer is pure Rust, with no GTK dependencies, so it can be tested
 * in isolation with `cargo test`.
 */

use std::path::PathBuf;

/**
 * A video in the list. Only stores minimal metadata: the list lives in the
 * UI, playback lives in the player. There is no playback logic here.
 */
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Video {
    path: PathBuf,
    name: String,
}

impl Video {
    pub fn new(path: PathBuf) -> Self {
        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| path.display().to_string());
        Self { path, name }
    }

    pub fn path(&self) -> &PathBuf {
        &self.path
    }

    pub fn name(&self) -> &str {
        &self.name
    }
}

/**
 * Ordered collection of videos aggregated by the user.
 *
 * Exposes only navigation (sequence) and current selection.
 * It knows nothing about playback or the UI.
 */
#[derive(Debug, Clone)]
pub struct VideoList {
    videos: Vec<Video>,
    selected: Option<usize>,
}

/** Result of attempting to move the selection. */
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Navigate {
    /** The selection moved. */
    Moved,
    /** Could not move because we are already at the end of the list. */
    Stuck,
}

impl Default for VideoList {
    fn default() -> Self {
        Self::new()
    }
}

impl VideoList {
    pub fn new() -> Self {
        Self {
            videos: Vec::new(),
            selected: None,
        }
    }

    pub fn len(&self) -> usize {
        self.videos.len()
    }

    pub fn is_empty(&self) -> bool {
        self.videos.is_empty()
    }

    /** Current selected index, if any. */
    pub fn selected_index(&self) -> Option<usize> {
        self.selected
    }

    /** The selected video, if any. */
    pub fn selected(&self) -> Option<&Video> {
        self.selected.and_then(|i| self.videos.get(i))
    }

    pub fn iter(&self) -> impl Iterator<Item = &Video> {
        self.videos.iter()
    }

    pub fn get(&self, index: usize) -> Option<&Video> {
        self.videos.get(index)
    }

    /** Appends videos to the end, preserving any previous selection. */
    pub fn add(&mut self, videos: Vec<Video>) {
        self.videos.extend(videos);
    }

    /** Selects the video at `index`. Returns `false` if the index does not exist. */
    pub fn select(&mut self, index: usize) -> bool {
        if index < self.videos.len() {
            self.selected = Some(index);
            true
        } else {
            false
        }
    }

    /** Clears the current selection without removing videos from the list. */
    pub fn clear_selection(&mut self) {
        self.selected = None;
    }

    /** Removes all videos from the list and the selection. */
    pub fn clear(&mut self) {
        self.videos.clear();
        self.selected = None;
    }

    /** Advances to the next position in the sequence. */
    #[allow(clippy::should_implement_trait)]
    pub fn next(&mut self) -> Navigate {
        match self.selected {
            Some(i) if i + 1 < self.videos.len() => {
                self.selected = Some(i + 1);
                Navigate::Moved
            }
            Some(_) => Navigate::Stuck,
            None if self.videos.is_empty() => Navigate::Stuck,
            None => {
                // Sin selección: avanzar al primer vídeo.
                self.selected = Some(0);
                Navigate::Moved
            }
        }
    }

    /** Goes back to the previous position in the sequence. */
    pub fn previous(&mut self) -> Navigate {
        match self.selected {
            Some(i) if i > 0 => {
                self.selected = Some(i - 1);
                Navigate::Moved
            }
            Some(_) => Navigate::Stuck,
            None if self.videos.is_empty() => Navigate::Stuck,
            None => {
                self.selected = Some(0);
                Navigate::Moved
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn video(name: &str) -> Video {
        Video::new(PathBuf::from(name))
    }

    fn list(names: &[&str]) -> VideoList {
        let mut l = VideoList::new();
        l.add(names.iter().map(|n| video(n)).collect());
        l
    }

    #[test]
    fn lista_vacia_sin_seleccion() {
        let l = VideoList::new();
        assert!(l.is_empty());
        assert_eq!(l.len(), 0);
        assert_eq!(l.selected_index(), None);
        assert_eq!(l.selected(), None);
    }

    #[test]
    fn nombre_derivado_del_file_name() {
        assert_eq!(video("pelicula.mp4").name(), "pelicula.mp4");
        assert_eq!(video("/ruta/a/otra.mkv").name(), "otra.mkv");
    }

    #[test]
    fn video_sin_file_name_usa_la_ruta_completa() {
        let v = Video::new(PathBuf::from("/"));
        assert_eq!(v.name(), "/");
    }

    #[test]
    fn add_anade_al_final() {
        let mut l = VideoList::new();
        l.add(vec![video("a.mp4"), video("b.mp4")]);
        assert_eq!(l.len(), 2);
        assert_eq!(
            l.iter().map(|v| v.name()).collect::<Vec<_>>(),
            ["a.mp4", "b.mp4"]
        );
    }

    #[test]
    fn select_valido_e_invalido() {
        let mut l = list(&["a.mp4", "b.mp4", "c.mp4"]);
        assert!(l.select(1));
        assert_eq!(l.selected_index(), Some(1));
        assert_eq!(l.selected().map(|v| v.name()), Some("b.mp4"));
        // Índice que no existe.
        assert!(!l.select(99));
        // El intento fallido no rompe la selección previa.
        assert_eq!(l.selected_index(), Some(1));
    }

    #[test]
    fn clear_selection_no_borra_videos() {
        let mut l = list(&["a.mp4", "b.mp4"]);
        l.select(1);
        l.clear_selection();
        assert_eq!(l.selected_index(), None);
        assert_eq!(l.len(), 2);
    }

    #[test]
    fn clear_borra_todo() {
        let mut l = list(&["a.mp4", "b.mp4"]);
        l.select(0);
        l.clear();
        assert!(l.is_empty());
        assert_eq!(l.selected_index(), None);
    }

    #[test]
    fn next_avanza_y_se_queda_al_final() {
        let mut l = list(&["a.mp4", "b.mp4", "c.mp4"]);
        l.select(0);
        assert_eq!(l.next(), Navigate::Moved);
        assert_eq!(l.selected_index(), Some(1));
        assert_eq!(l.next(), Navigate::Moved);
        assert_eq!(l.selected_index(), Some(2));
        assert_eq!(l.next(), Navigate::Stuck);
        assert_eq!(l.selected_index(), Some(2));
    }

    #[test]
    fn previous_retrocede_y_se_queda_al_inicio() {
        let mut l = list(&["a.mp4", "b.mp4", "c.mp4"]);
        l.select(2);
        assert_eq!(l.previous(), Navigate::Moved);
        assert_eq!(l.selected_index(), Some(1));
        assert_eq!(l.previous(), Navigate::Moved);
        assert_eq!(l.selected_index(), Some(0));
        assert_eq!(l.previous(), Navigate::Stuck);
        assert_eq!(l.selected_index(), Some(0));
    }

    #[test]
    fn sin_seleccion_saltan_al_primero() {
        let mut l = list(&["a.mp4", "b.mp4"]);
        assert_eq!(l.next(), Navigate::Moved);
        assert_eq!(l.selected_index(), Some(0));
        let mut l2 = list(&["a.mp4", "b.mp4"]);
        assert_eq!(l2.previous(), Navigate::Moved);
        assert_eq!(l2.selected_index(), Some(0));
    }

    #[test]
    fn navegacion_en_lista_vacia_esta_atascada() {
        let mut l = VideoList::new();
        assert_eq!(l.next(), Navigate::Stuck);
        assert_eq!(l.previous(), Navigate::Stuck);
        assert_eq!(l.selected_index(), None);
    }

    #[test]
    fn get_devuelve_el_video_en_indice() {
        let l = list(&["a.mp4", "b.mp4"]);
        assert_eq!(l.get(0).map(|v| v.name()), Some("a.mp4"));
        assert_eq!(l.get(5), None);
    }

    #[test]
    fn add_conserva_la_seleccion_existente() {
        let mut l = list(&["a.mp4"]);
        l.select(0);
        l.add(vec![video("b.mp4")]);
        assert_eq!(l.selected_index(), Some(0));
        assert_eq!(l.len(), 2);
    }
}
