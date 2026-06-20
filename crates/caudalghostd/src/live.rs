use caudalghost_core::types::Sample;
use std::collections::VecDeque;

/// Últimos N segundos de muestras, solo en RAM (capa "live" del spec).
pub struct LiveRing {
    cap: usize,
    buf: VecDeque<Sample>,
}

impl LiveRing {
    pub fn new(cap: usize) -> Self {
        Self {
            cap,
            buf: VecDeque::with_capacity(cap),
        }
    }

    pub fn push(&mut self, s: Sample) {
        if self.buf.len() == self.cap {
            self.buf.pop_front();
        }
        self.buf.push_back(s);
    }

    /// Los `n` más recientes, en orden cronológico.
    pub fn last(&self, n: usize) -> Vec<Sample> {
        let skip = self.buf.len().saturating_sub(n);
        self.buf.iter().skip(skip).cloned().collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use caudalghost_core::types::Sample;

    fn s(ts: i64) -> Sample {
        Sample {
            ts,
            deltas: vec![],
            apps: vec![],
            flows: vec![],
        }
    }

    #[test]
    fn capacidad_fija_descarta_lo_viejo() {
        let mut r = LiveRing::new(3);
        for t in 0..5 {
            r.push(s(t));
        }
        let last = r.last(10);
        assert_eq!(last.iter().map(|x| x.ts).collect::<Vec<_>>(), vec![2, 3, 4]);
    }

    #[test]
    fn last_n_devuelve_los_mas_recientes_en_orden() {
        let mut r = LiveRing::new(10);
        for t in 0..6 {
            r.push(s(t));
        }
        let last = r.last(2);
        assert_eq!(last.iter().map(|x| x.ts).collect::<Vec<_>>(), vec![4, 5]);
    }
}
