//! Som → nível 0/1 (demodulador FSK) e bits → som (sintetizador).
//!
//! O áudio é dividido em blocos de ~1/6 de bit. Em cada bloco, o algoritmo de
//! Goertzel mede só as duas frequências que interessam (custo O(N) por
//! frequência, sem FFT completa), com janela de Hann para reduzir vazamento.
//! O tempo de cada bloco vem da contagem de amostras, não do relógio do
//! sistema, então o ritmo dos bits é preciso.

use std::f32::consts::PI;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FskConfig {
    pub sample_rate: f32,
    pub f0: f32,
    pub f1: f32,
    pub bit_ms: f64,
}

impl FskConfig {
    pub const AUDIBLE: (f32, f32) = (1500.0, 2500.0);
    pub const NEAR_ULTRASONIC: (f32, f32) = (17_500.0, 18_500.0);

    pub fn new(sample_rate: f32, f0: f32, f1: f32, bit_ms: f64) -> Self {
        Self { sample_rate, f0, f1, bit_ms }
    }
}

/// Amplitude mínima (escala 0..1) para considerar que há tom.
const MIN_AMP: f32 = 0.003;
/// Quanto o tom vencedor precisa superar o outro.
const RATIO: f32 = 1.6;

#[derive(Clone, Copy, Debug)]
pub struct FskSample {
    pub t_ms: f64,
    pub level: u8,
    /// -1..1: quanto o tom "1" domina o tom "0"; 0 sem sinal. Para o gráfico.
    pub value: f32,
    pub amp0: f32,
    pub amp1: f32,
}

pub struct FskDemod {
    cfg: FskConfig,
    window: Vec<f32>,
    window_sum: f32,
    coeff0: f32,
    coeff1: f32,
    buf: Vec<f32>,
    consumed: u64,
}

fn goertzel_coeff(f: f32, sr: f32) -> f32 {
    2.0 * (2.0 * PI * f / sr).cos()
}

impl FskDemod {
    pub fn new(cfg: FskConfig) -> Self {
        let block = ((cfg.sample_rate as f64 * cfg.bit_ms / 1000.0 / 6.0) as usize).clamp(64, 4096);
        let window: Vec<f32> = (0..block)
            .map(|i| 0.5 - 0.5 * (2.0 * PI * i as f32 / (block - 1) as f32).cos())
            .collect();
        let window_sum = window.iter().sum();
        Self {
            coeff0: goertzel_coeff(cfg.f0, cfg.sample_rate),
            coeff1: goertzel_coeff(cfg.f1, cfg.sample_rate),
            cfg,
            window,
            window_sum,
            buf: Vec::with_capacity(block),
            consumed: 0,
        }
    }

    pub fn block_len(&self) -> usize {
        self.window.len()
    }

    /// Consome amostras PCM (mono, -1..1) e acrescenta um resultado por bloco completo.
    pub fn push(&mut self, pcm: &[f32], out: &mut Vec<FskSample>) {
        let n = self.window.len();
        for &x in pcm {
            self.buf.push(x);
            if self.buf.len() == n {
                self.consumed += n as u64;
                out.push(self.analyze());
                self.buf.clear();
            }
        }
    }

    fn analyze(&self) -> FskSample {
        let (mut a0, mut b0, mut a1, mut b1) = (0f32, 0f32, 0f32, 0f32);
        for (x, w) in self.buf.iter().zip(&self.window) {
            let v = x * w;
            let s0 = v + self.coeff0 * a0 - b0;
            b0 = a0;
            a0 = s0;
            let s1 = v + self.coeff1 * a1 - b1;
            b1 = a1;
            a1 = s1;
        }
        let power = |a: f32, b: f32, c: f32| (a * a + b * b - c * a * b).max(0.0);
        let amp0 = 2.0 * power(a0, b0, self.coeff0).sqrt() / self.window_sum;
        let amp1 = 2.0 * power(a1, b1, self.coeff1).sqrt() / self.window_sum;
        let strong = amp0.max(amp1) > MIN_AMP;
        let level = (strong && amp1 > amp0 * RATIO) as u8;
        let value = if strong { (amp1 - amp0) / (amp1 + amp0) } else { 0.0 };
        let center = self.consumed as f64 - self.window.len() as f64 / 2.0;
        FskSample { t_ms: center / self.cfg.sample_rate as f64 * 1000.0, level, value, amp0, amp1 }
    }
}

/// Gera o áudio PCM (mono, -1..1) de uma sequência de bits.
///
/// Fase contínua entre bits (sem estalos), `lead_ms` de tom "0" no começo para
/// o receptor estabilizar e 5 ms de rampa nas pontas.
pub fn synth(bits: &[u8], cfg: &FskConfig, lead_ms: f64, amplitude: f32) -> Vec<f32> {
    let sr = cfg.sample_rate as f64;
    let lead = (lead_ms / 1000.0 * sr) as usize;
    let per_bit = cfg.bit_ms / 1000.0 * sr;
    let total = lead + (bits.len() as f64 * per_bit).ceil() as usize;
    let ramp = ((0.005 * sr) as usize).max(1);
    let mut out = Vec::with_capacity(total);
    let mut phase = 0f64;
    for i in 0..total {
        let bit = if i < lead { 0 } else { bits.get(((i - lead) as f64 / per_bit) as usize).copied().unwrap_or(0) };
        let f = if bit != 0 { cfg.f1 } else { cfg.f0 } as f64;
        phase = (phase + 2.0 * std::f64::consts::PI * f / sr) % (2.0 * std::f64::consts::PI);
        let edge = i.min(total - 1 - i);
        let env = if edge < ramp { 0.5 - 0.5 * (std::f64::consts::PI * edge as f64 / ramp as f64).cos() } else { 1.0 };
        out.push((phase.sin() * env) as f32 * amplitude);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::uart::{bytes_to_bits, Uart};

    fn decode(pcm: &[f32], cfg: FskConfig, chunk: usize) -> Vec<u8> {
        let mut d = FskDemod::new(cfg);
        let mut u = Uart::new(cfg.bit_ms);
        let mut samples = Vec::new();
        for c in pcm.chunks(chunk) {
            d.push(c, &mut samples);
        }
        samples.iter().filter_map(|s| u.push(s.t_ms, s.level)).collect()
    }

    fn noise(n: usize, amp: f32) -> Vec<f32> {
        let mut rng = crate::uart::tests::Lcg(99);
        (0..n).map(|_| (rng.next() as f32 - 0.5) * 2.0 * amp).collect()
    }

    #[test]
    fn audible_roundtrip_with_noise_and_low_volume() {
        let cfg = FskConfig::new(48_000.0, 1500.0, 2500.0, 80.0);
        let data = "Olá ✂️".as_bytes();
        let mut pcm = synth(&bytes_to_bits(data), &cfg, 300.0, 0.05);
        let n = noise(pcm.len(), 0.03);
        for (x, n) in pcm.iter_mut().zip(n) {
            *x += n;
        }
        assert!(decode(&pcm, cfg, 128).ends_with(data));
    }

    #[test]
    fn near_ultrasonic_roundtrip_44k1() {
        let (f0, f1) = FskConfig::NEAR_ULTRASONIC;
        let cfg = FskConfig::new(44_100.0, f0, f1, 60.0);
        let data = b"ultra";
        let pcm = synth(&bytes_to_bits(data), &cfg, 300.0, 0.3);
        assert!(decode(&pcm, cfg, 1000).ends_with(data));
    }

    #[test]
    fn silence_produces_nothing() {
        let cfg = FskConfig::new(48_000.0, 1500.0, 2500.0, 80.0);
        assert!(decode(&noise(480_000, 0.001), cfg, 512).is_empty());
    }
}
