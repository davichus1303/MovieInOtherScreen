//! Modelo de vídeos y su lista.
//!
//! Esta capa es Rust puro, sin dependencias de GTK, para poder ser probada
//! de forma aislada con `cargo test`.

use std::path::PathBuf;

/// Un vídeo de la lista. Solo almacena metadatos mínimos: la lista vive en la
/// UI, la reproducción en el reproductor. Aquí no hay lógica de reproducción.
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

/// Colección ordenada de vídeos agregada por el usuario.
///
/// Expone exclusivamente la navegación (secuencia) y la selección actual.
/// No sabe nada de reproducción ni de la UI.
#[derive(Debug, Clone)]
pub struct VideoList {
    videos: Vec<Video>,
    selected: Option<usize>,
}

/// Resultado de intentar mover la selección.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Navigate {
    /// La selección se movió.
    Moved,
    /// No se pudo mover porque ya estamos en el extremo de la lista.
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

    /// Índice seleccionado actual, si existe.
    pub fn selected_index(&self) -> Option<usize> {
        self.selected
    }

    /// El vídeo seleccionado, si existe.
    pub fn selected(&self) -> Option<&Video> {
        self.selected.and_then(|i| self.videos.get(i))
    }

    pub fn iter(&self) -> impl Iterator<Item = &Video> {
        self.videos.iter()
    }

    pub fn get(&self, index: usize) -> Option<&Video> {
        self.videos.get(index)
    }

    /// Agrega los vídeos al final, conservando cualquier selección previa.
    pub fn add(&mut self, videos: Vec<Video>) {
        self.videos.extend(videos);
    }

    /// Selecciona el vídeo en `index`. Devuelve `false` si el índice no existe.
    pub fn select(&mut self, index: usize) -> bool {
        if index < self.videos.len() {
            self.selected = Some(index);
            true
        } else {
            false
        }
    }

    /// Vacía la selección actual sin borrar los vídeos de la lista.
    pub fn clear_selection(&mut self) {
        self.selected = None;
    }

    /// Avanza a la siguiente posición de la secuencia.
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

    /// Retrocede a la posición anterior de la secuencia.
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
