//! Lógica de control de reproducción, independiente del backend concreto.
//!
//! Esta capa decide *qué* hacer (play, pause, stop, seek, siguiente, anterior,
//! inicio) y calcula a qué vídeo/s posiciones se refieren. La ejecución real
//! sobre libmpv está en la capa de reproductor que consume estos comandos.

/// Comandos que el reproductor puede ejecutar.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Command {
    Play,
    Pause,
    Stop,
    /// Lleva el vídeo actual al inicio, sin cambiar de vídeo.
    Home,
    /// Busca a una posición normalizada [0.0, 1.0] del vídeo actual.
    Seek(f64),
    /// Busca a un segmento concreto (tecla/clic).
    SeekSegment { segment: u32 },
    /// Cambia el vídeo actual al siguiente de la secuencia.
    Next,
    /// Cambia el vídeo actual al anterior de la secuencia.
    Previous,
}

/// Estado de reproducción del vídeo actual.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlayState {
    Stopped,
    Playing,
    Paused,
}

impl Default for PlayState {
    fn default() -> Self {
        Self::Stopped
    }
}

/// Resultado de la última operación de navegación a nivel de reproductor.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlaybackNav {
    /// Se sigue en el mismo vídeo.
    Same,
    /// Se pasó a un vídeo distinto (requiere transición).
    Changed,
    /// Se intentó moverse pero no se pudo (extremo de la lista).
    Impossible,
}

/// Intérprete de comandos sobre la lista de vídeos.
///
/// Mantiene qué vídeo está cargado y delega la navegación de la secuencia a
/// `VideoList`. No ejecuta audio/video: solo produce decisiones.
#[derive(Debug, Clone, Default)]
pub struct PlaybackController {
    /// El vídeo cargado para reproducir.
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

    /// Registra un cambio de vídeo externo (selección desde la lista).
    pub fn handoff(&mut self, index: usize, play_state: PlayState) {
        self.current = Some(index);
        self.play_state = play_state;
    }

    pub fn clear(&mut self) {
        self.current = None;
        self.play_state = PlayState::Stopped;
    }

    /// Aplica un comando contra la lista dando la decisión resultante.
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

/// Referencia estrecha a lo que el controlador necesita de la lista.
///
/// Evita depender de toda la estructura `VideoList`, manteniendo el bajo
/// acoplamiento y la testabilidad.
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
        assert_eq!(c.apply(Command::Previous, &list(4)), PlaybackNav::Impossible);
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
}
