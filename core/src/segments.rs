//! Segmentos de la barra de progreso y su mapeo desde teclado y clic.

/// Número de segmentos en los que se divide conceptualmente la barra.
pub const SEGMENT_COUNT: u32 = 10;

/// Representa un segmento válido de la barra (del 1 al 10).
///
/// Se usa internamente para un mapeo inequívoco: el segmento 10 se alcanza
/// con la tecla física `0`, nunca con una tecla inexistente "10".
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Segment(u32);

impl Segment {
    /// Posición normalizada [0.0, 1.0] en la que empieza este segmento.
    ///
    /// El segmento 1 comienza en 0.0, el 10 en 0.9.
    pub fn position(&self) -> f64 {
        f64::from(self.0.saturating_sub(1)) / f64::from(SEGMENT_COUNT)
    }
}

impl TryFrom<u32> for Segment {
    type Error = SegmentError;

    /// Construye un segmento a partir de un número de tecla física (1-9, y 0).
    ///
    /// La tecla `0` se interpreta como el segmento final (equivalente a 10),
    /// eliminando la ambigüedad con la décima posición.
    fn try_from(physical_key: u32) -> Result<Self, Self::Error> {
        match physical_key {
            0 => Ok(Segment(SEGMENT_COUNT)),
            1..=9 => Ok(Segment(physical_key)),
            _ => Err(SegmentError::OutOfRange(physical_key)),
        }
    }
}

/// Construye un segmento a partir de una proporción normalizada [0.0, 1.0].
///
/// Se usa para calcular el segmento correspondiente a un clic en la barra.
impl From<f64> for Segment {
    fn from(fraction: f64) -> Self {
        let f = fraction.clamp(0.0, 0.999_999);
        let nth = (f * f64::from(SEGMENT_COUNT)).floor() as u32 + 1;
        Segment(nth.min(SEGMENT_COUNT))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SegmentError {
    OutOfRange(u32),
}

impl std::fmt::Display for SegmentError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SegmentError::OutOfRange(n) => {
                write!(f, "tecla física {n} fuera del rango de segmentos (0-9)")
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tecla_uno_es_segmento_uno_en_posicion_cero() {
        let s = Segment::try_from(1).unwrap();
        assert_eq!(s.position(), 0.0);
    }

    #[test]
    fn tecla_nueve_es_segmento_nueve_en_posicion_0_8() {
        let s = Segment::try_from(9).unwrap();
        assert_eq!(s.position(), 0.8);
    }

    #[test]
    fn tecla_cero_es_segmento_10_en_posicion_0_9() {
        let s = Segment::try_from(0).unwrap();
        assert_eq!(s.position(), 0.9);
    }

    #[test]
    fn clic_al_principio_es_segmento_uno() {
        assert_eq!(Segment::from(0.0).position(), 0.0);
        assert_eq!(Segment::from(0.01).position(), 0.0);
    }

    #[test]
    fn clic_al_final_es_segmento_10() {
        assert_eq!(Segment::from(0.95).position(), 0.9);
        assert_eq!(Segment::from(1.0).position(), 0.9);
    }

    #[test]
    fn clic_medio_es_segmento_6() {
        assert_eq!(Segment::from(0.54).position(), 0.5);
    }

    #[test]
    fn tecla_fuera_de_rango_error() {
        assert_eq!(Segment::try_from(10), Err(SegmentError::OutOfRange(10)));
    }
}
