/*! Playback control logic, independent of the concrete backend.
 *
 * This layer decides *what* to do (play, pause, stop, seek, next, previous,
 * home) and calculates which video(s) and positions they refer to. The actual
 * execution on libmpv is in the player layer that consumes these commands.
 */

/** Commands that the player can execute. */
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Command {
    Play,
    Pause,
    Stop,
    /** Returns the current video to the beginning without changing video. */
    Home,
    /** Seeks to a normalized position [0.0, 1.0] of the current video. */
    Seek(f64),
    /** Seeks to a specific segment (key/click). */
    SeekSegment {
        segment: u32,
    },
    /** Changes the current video to the next one in the sequence. */
    Next,
    /** Changes the current video to the previous one in the sequence. */
    Previous,
}

/** Playback state of the current video. */
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PlayState {
    #[default]
    Stopped,
    Playing,
    Paused,
}

/** Result of the last navigation operation at the player level. */
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlaybackNav {
    /** Still on the same video. */
    Same,
    /** Moved to a different video (transition required). */
    Changed,
    /** Attempted to move but could not (end of the list). */
    Impossible,
}

/**
 * Command interpreter over the video list.
 *
 * Keeps track of which video is loaded and delegates sequence navigation to
 * `VideoList`. It does not execute audio/video: it only produces decisions.
 */
#[derive(Debug, Clone, Default)]
pub struct PlaybackController {
    /** The video loaded for playback. */
    current: Option<usize>,
    play_state: PlayState,
}

impl PlaybackController {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn current_index(&self) -> Option<usize> {
        self.current
    }

    pub fn play_state(&self) -> PlayState {
        self.play_state
    }

    /** Records an external video change (selection from the list). */
    pub fn handoff(&mut self, index: usize, play_state: PlayState) {
        self.current = Some(index);
        self.play_state = play_state;
    }

    pub fn clear(&mut self) {
        self.current = None;
        self.play_state = PlayState::Stopped;
    }

    /** Applies a command against the list, returning the resulting decision. */
    pub fn apply(&mut self, cmd: Command, list: &VideoListRef) -> PlaybackNav {
        match cmd {
            Command::Play => {
                if !list.is_empty() {
                    let index = self.current.unwrap_or(0).min(list.len() - 1);
                    self.current = Some(index);
                    self.play_state = PlayState::Playing;
                } else {
                    self.play_state = PlayState::Stopped;
                }
                PlaybackNav::Same
            }
            Command::Pause => {
                self.play_state = PlayState::Paused;
                PlaybackNav::Same
            }
            Command::Stop => {
                self.play_state = PlayState::Stopped;
                PlaybackNav::Same
            }
            Command::Home | Command::Seek(_) | Command::SeekSegment { .. } => PlaybackNav::Same,
            Command::Next => self.move_relative(list, 1),
            Command::Previous => self.move_relative(list, -1),
        }
    }

    fn move_relative(&mut self, list: &VideoListRef, delta: isize) -> PlaybackNav {
        let Some(current) = self.current else {
            // Sin vídeo cargado: saltar al primero si la lista no está vacía.
            if list.is_empty() {
                return PlaybackNav::Impossible;
            }
            self.current = Some(0);
            return PlaybackNav::Changed;
        };
        let target = current as isize + delta;
        if target < 0 || target >= list.len() as isize {
            return PlaybackNav::Impossible;
        }
        self.current = Some(target as usize);
        PlaybackNav::Changed
    }
}

/**
 * Narrow reference to what the controller needs from the list.
 *
 * Avoids depending on the entire `VideoList` structure, keeping coupling low
 * and testability high.
 */
#[derive(Debug, Clone, Copy)]
pub struct VideoListRef {
    len: usize,
}

impl VideoListRef {
    pub fn new(len: usize) -> Self {
        Self { len }
    }

    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    pub fn len(&self) -> usize {
        self.len
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn list(n: usize) -> VideoListRef {
        VideoListRef::new(n)
    }

    #[test]
    fn play_selecciona_el_primero_sin_estado_previo() {
        let mut c = PlaybackController::new();
        c.apply(Command::Play, &list(3));
        assert_eq!(c.current_index(), Some(0));
        assert_eq!(c.play_state(), PlayState::Playing);
    }

    #[test]
    fn pause_y_play_conmutan_estado() {
        let mut c = PlaybackController::new();
        c.apply(Command::Play, &list(1));
        c.apply(Command::Pause, &list(1));
        assert_eq!(c.play_state(), PlayState::Paused);
        c.apply(Command::Play, &list(1));
        assert_eq!(c.play_state(), PlayState::Playing);
    }

    #[test]
    fn stop_pone_estado_detenido() {
        let mut c = PlaybackController::new();
        c.apply(Command::Play, &list(1));
        c.apply(Command::Stop, &list(1));
        assert_eq!(c.play_state(), PlayState::Stopped);
    }

    #[test]
    fn siguiente_avanza_en_la_secuencia() {
        let mut c = PlaybackController::new();
        c.apply(Command::Play, &list(4));
        assert_eq!(c.current_index(), Some(0));
        assert_eq!(c.apply(Command::Next, &list(4)), PlaybackNav::Changed);
        assert_eq!(c.current_index(), Some(1));
        assert_eq!(c.apply(Command::Next, &list(4)), PlaybackNav::Changed);
        assert_eq!(c.current_index(), Some(2));
    }

    #[test]
    fn anterior_retrocede_en_la_secuencia() {
        let mut c = PlaybackController::new();
        c.handoff(3, PlayState::Playing);
        assert_eq!(c.apply(Command::Previous, &list(4)), PlaybackNav::Changed);
        assert_eq!(c.current_index(), Some(2));
        assert_eq!(c.apply(Command::Previous, &list(4)), PlaybackNav::Changed);
        assert_eq!(c.current_index(), Some(1));
    }

    #[test]
    fn ultimo_video_siguiente_no_hace_nada() {
        let mut c = PlaybackController::new();
        c.handoff(3, PlayState::Playing);
        assert_eq!(c.apply(Command::Next, &list(4)), PlaybackNav::Impossible);
        assert_eq!(c.current_index(), Some(3));
    }

    #[test]
    fn primer_video_anterior_no_hace_nada() {
        let mut c = PlaybackController::new();
        c.handoff(0, PlayState::Playing);
        assert_eq!(
            c.apply(Command::Previous, &list(4)),
            PlaybackNav::Impossible
        );
        assert_eq!(c.current_index(), Some(0));
    }

    #[test]
    fn home_no_cambia_el_video_ni_el_estado() {
        let mut c = PlaybackController::new();
        c.handoff(2, PlayState::Paused);
        assert_eq!(c.apply(Command::Home, &list(4)), PlaybackNav::Same);
        assert_eq!(c.current_index(), Some(2));
        assert_eq!(c.play_state(), PlayState::Paused);
    }

    #[test]
    fn play_con_lista_vacia_pone_detenido() {
        let mut c = PlaybackController::new();
        c.apply(Command::Play, &list(0));
        assert_eq!(c.play_state(), PlayState::Stopped);
        assert_eq!(c.current_index(), None);
    }

    #[test]
    fn seek_y_seek_segment_no_navegan() {
        let mut c = PlaybackController::new();
        c.handoff(1, PlayState::Playing);
        assert_eq!(c.apply(Command::Seek(0.5), &list(3)), PlaybackNav::Same);
        assert_eq!(
            c.apply(Command::SeekSegment { segment: 5 }, &list(3)),
            PlaybackNav::Same
        );
        assert_eq!(c.current_index(), Some(1));
        assert_eq!(c.play_state(), PlayState::Playing);
    }

    #[test]
    fn handoff_registra_estado_externo() {
        let mut c = PlaybackController::new();
        c.handoff(2, PlayState::Playing);
        assert_eq!(c.current_index(), Some(2));
        assert_eq!(c.play_state(), PlayState::Playing);
    }

    #[test]
    fn clear_resetea_el_estado() {
        let mut c = PlaybackController::new();
        c.handoff(2, PlayState::Playing);
        c.clear();
        assert_eq!(c.current_index(), None);
        assert_eq!(c.play_state(), PlayState::Stopped);
    }

    #[test]
    fn siguiente_sin_video_cargado_salta_al_primero() {
        let mut c = PlaybackController::new();
        assert_eq!(c.apply(Command::Next, &list(3)), PlaybackNav::Changed);
        assert_eq!(c.current_index(), Some(0));
    }

    #[test]
    fn siguiente_con_lista_vacia_imposible() {
        let mut c = PlaybackController::new();
        assert_eq!(c.apply(Command::Next, &list(0)), PlaybackNav::Impossible);
        assert_eq!(c.current_index(), None);
    }
}
