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
