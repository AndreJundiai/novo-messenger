//! Luz → nível 0/1.
//!
//! O brilho medido pela câmera muda com a distância, a luz ambiente e a
//! exposição automática. Por isso o limiar é adaptativo: o meio entre o mínimo
//! e o máximo dos últimos 3 s. Mínimo e máximo usam filas monotônicas, então
//! cada amostra custa O(1) amortizado.

use std::collections::VecDeque;

const WINDOW_MS: f64 = 3000.0;
/// Contraste mínimo (em níveis de 0 a 255) para considerar que há sinal.
pub const MIN_CONTRAST: f32 = 20.0;

#[derive(Clone, Copy, Debug)]
pub struct LightSample {
    pub level: u8,
    /// Brilho normalizado entre o mínimo e o máximo recentes (0..1), para o gráfico.
    pub norm: f32,
    pub contrast: f32,
    pub luma: f32,
}

#[derive(Default)]
pub struct LightDetector {
    mins: VecDeque<(f64, f32)>,
    maxs: VecDeque<(f64, f32)>,
}

impl LightDetector {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn push(&mut self, t: f64, luma: f32) -> LightSample {
        while self.mins.back().is_some_and(|&(_, v)| v >= luma) {
            self.mins.pop_back();
        }
        self.mins.push_back((t, luma));
        while self.maxs.back().is_some_and(|&(_, v)| v <= luma) {
            self.maxs.pop_back();
        }
        self.maxs.push_back((t, luma));
        while self.mins.front().is_some_and(|&(ts, _)| t - ts > WINDOW_MS) {
            self.mins.pop_front();
        }
        while self.maxs.front().is_some_and(|&(ts, _)| t - ts > WINDOW_MS) {
            self.maxs.pop_front();
        }
        let mn = self.mins.front().map_or(luma, |p| p.1);
        let mx = self.maxs.front().map_or(luma, |p| p.1);
        let contrast = mx - mn;
        let level = (contrast > MIN_CONTRAST && luma > (mn + mx) / 2.0) as u8;
        let norm = if contrast > 5.0 { (luma - mn) / contrast } else { 0.5 };
        LightSample { level, norm, contrast, luma }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn follows_slow_exposure_drift() {
        // Liga/desliga a cada 150 ms enquanto a exposição sobe de 40 para 140.
        let mut d = LightDetector::new();
        let mut errors = 0;
        for i in 0..300 {
            let t = i as f64 * 33.3;
            let on = ((t / 150.0) as u64).is_multiple_of(2);
            let base = 40.0 + (t / 10_000.0 * 100.0) as f32;
            let s = d.push(t, if on { base + 60.0 } else { base });
            if i > 30 && s.level != on as u8 {
                errors += 1;
            }
        }
        assert_eq!(errors, 0);
    }

    #[test]
    fn steady_light_is_not_signal() {
        let mut d = LightDetector::new();
        for i in 0..200 {
            let s = d.push(i as f64 * 33.3, 120.0 + (i % 3) as f32);
            assert_eq!(s.level, 0);
        }
    }
}
