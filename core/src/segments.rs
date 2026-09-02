/*! Progress bar segments and their mapping from keyboard and click. */

use crate::constants::segments::MAX_FRACTION_CLAMP;

/** Number of segments the bar is conceptually divided into. */
pub const SEGMENT_COUNT: u32 = 10;

/**
 * Represents a valid bar segment (from 1 to 10).
 *
 * Used internally for an unambiguous mapping: segment 10 is reached
 * with the physical key `0`, never with a non-existent key "10".
 */
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Segment(u32);

impl Segment {
    /**
     * Normalized position [0.0, 1.0] at which this segment starts.
     *
     * Segment 1 starts at 0.0, segment 10 at 0.9.
     */
    pub fn position(&self) -> f64 {
        f64::from(self.0.saturating_sub(1)) / f64::from(SEGMENT_COUNT)
    }
}

impl TryFrom<u32> for Segment {
    type Error = SegmentError;

    /**
     * Builds a segment from a physical key number (1-9, and 0).
     *
     * The `0` key is interpreted as the final segment (equivalent to 10),
     * eliminating ambiguity with the tenth position.
     */
    fn try_from(physical_key: u32) -> Result<Self, Self::Error> {
        match physical_key {
            0 => Ok(Segment(SEGMENT_COUNT)),
            1..=9 => Ok(Segment(physical_key)),
            _ => Err(SegmentError::OutOfRange(physical_key)),
        }
    }
}

/**
 * Builds a segment from a normalized fraction [0.0, 1.0].
 *
 * Used to calculate the segment corresponding to a click on the bar.
 */
impl From<f64> for Segment {
    fn from(fraction: f64) -> Self {
        let f = fraction.clamp(0.0, MAX_FRACTION_CLAMP);
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
