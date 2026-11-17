//! Camada de bits, igual a uma porta serial (UART).
//!
//! Linha parada = 0. Cada byte = 1 bit de início (1) + 8 bits de dados (o menos
//! significativo primeiro) + 2 bits de parada (0). O receptor se sincroniza na
//! borda de subida de cada bit de início, então tolera relógios imprecisos e
//! amostragem irregular (câmera a 30 fps, quadros perdidos).

/// Preâmbulo 1010… (calibra o limiar do receptor) + silêncio.
const PREAMBLE: [u8; 20] = [1, 0, 1, 0, 1, 0, 1, 0, 1, 0, 1, 0, 1, 0, 1, 0, 0, 0, 0, 0];

pub fn bytes_to_bits(bytes: &[u8]) -> Vec<u8> {
    let mut bits = Vec::with_capacity(PREAMBLE.len() + bytes.len() * 11 + 4);
    bits.extend_from_slice(&PREAMBLE);
    for &byte in bytes {
        bits.push(1);
        bits.extend((0..8).map(|i| (byte >> i) & 1));
        bits.extend_from_slice(&[0, 0]);
    }
    bits.extend_from_slice(&[0, 0, 0, 0]);
    bits
}

/// Trocas seguidas exigidas pela fase 1. O preâmbulo de toda transmissão tem
/// só 15, então ele sozinho não passa a fase.
pub const TOGGLE_RUN: u32 = 24;

/// Fase 1: 1010… por cerca de `ms` milissegundos (no mínimo 40 bits).
pub fn toggle_bits(ms: f64, bit_ms: f64) -> Vec<u8> {
    let n = ((ms / bit_ms).round() as usize).max(40);
    let mut bits: Vec<u8> = (0..n).map(|i| (i % 2 == 0) as u8).collect();
    bits.push(0);
    bits
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum State {
    Idle,
    Data,
    Stop,
}

pub struct Uart {
    bit_ms: f64,
    state: State,
    prev: u8,
    prev_t: Option<f64>,
    edge: f64,
    bit: u32,
    byte: u8,
}

impl Uart {
    pub fn new(bit_ms: f64) -> Self {
        Self { bit_ms, state: State::Idle, prev: 0, prev_t: None, edge: 0.0, bit: 0, byte: 0 }
    }

    /// Recebe uma amostra (tempo em ms, nível 0/1). Devolve um byte quando completo.
    pub fn push(&mut self, t: f64, level: u8) -> Option<u8> {
        let l = (level != 0) as u8;
        let t_bit = self.bit_ms;
        let mut out = None;
        match self.state {
            State::Idle => {
                if l == 1 && self.prev == 0 {
                    self.edge = self.prev_t.map_or(t, |p| (p + t) / 2.0);
                    self.state = State::Data;
                    self.bit = 0;
                    self.byte = 0;
                }
            }
            State::Data => {
                while self.bit < 8 && t >= self.edge + (self.bit as f64 + 1.5) * t_bit {
                    self.byte |= l << self.bit;
                    self.bit += 1;
                }
                if self.bit == 8 {
                    self.state = State::Stop;
                }
            }
            State::Stop => {}
        }
        if self.state == State::Stop && t >= self.edge + 9.5 * t_bit {
            if l == 0 {
                out = Some(self.byte);
            }
            self.state = State::Idle;
        }
        self.prev = l;
        self.prev_t = Some(t);
        out
    }
}

/// Detecta a fase 1: [`TOGGLE_RUN`] trocas de nível seguidas, espaçadas de ~1 bit.
/// Dispara uma vez por sequência.
pub struct ToggleDetector {
    bit_ms: f64,
    prev: u8,
    run: u32,
    last_flip: Option<f64>,
}

impl ToggleDetector {
    pub fn new(bit_ms: f64) -> Self {
        Self { bit_ms, prev: 0, run: 0, last_flip: None }
    }

    pub fn push(&mut self, t: f64, level: u8) -> bool {
        let l = (level != 0) as u8;
        let mut hit = false;
        if l != self.prev {
            if let Some(last) = self.last_flip {
                let dt = t - last;
                if dt > self.bit_ms * 0.55 && dt < self.bit_ms * 1.6 {
                    self.run += 1;
                } else {
                    self.run = 0;
                }
                if self.run == TOGGLE_RUN {
                    hit = true;
                }
            }
            self.last_flip = Some(t);
        }
        self.prev = l;
        hit
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// Gerador pseudoaleatório determinístico para os testes.
    pub struct Lcg(pub u64);
    impl Lcg {
        pub fn next(&mut self) -> f64 {
            self.0 = self.0.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            (self.0 >> 11) as f64 / (1u64 << 53) as f64
        }
    }

    /// Amostra `bits` com período médio `sample_ms` e tremor `jitter_ms`.
    pub fn sample(bits: &[u8], bit_ms: f64, sample_ms: f64, jitter_ms: f64, seed: u64) -> Vec<(f64, u8)> {
        let mut rng = Lcg(seed);
        let t0 = 500.0 + rng.next() * 100.0;
        let end = t0 + bits.len() as f64 * bit_ms + 1000.0;
        let mut out = Vec::new();
        let mut t = 0.0;
        while t < end {
            let i = ((t - t0) / bit_ms).floor();
            let l = if i >= 0.0 && (i as usize) < bits.len() { bits[i as usize] } else { 0 };
            out.push((t, l));
            t += sample_ms + (rng.next() - 0.5) * jitter_ms;
        }
        out
    }

    fn roundtrip(bytes: &[u8], bit_ms: f64, sample_ms: f64, jitter: f64, seed: u64) -> Vec<u8> {
        let mut u = Uart::new(bit_ms);
        sample(&bytes_to_bits(bytes), bit_ms, sample_ms, jitter, seed)
            .into_iter()
            .filter_map(|(t, l)| u.push(t, l))
            .collect()
    }

    #[test]
    fn camera_30fps_with_jitter() {
        let data: Vec<u8> = (0..=255).collect();
        for seed in 1..6 {
            let got = roundtrip(&data, 150.0, 33.3, 15.0, seed);
            assert!(got.ends_with(&data), "seed {seed}");
        }
    }

    #[test]
    fn fast_bits_100ms() {
        let data = b"Novo Messenger";
        assert!(roundtrip(data, 100.0, 33.3, 10.0, 7).ends_with(data));
    }

    #[test]
    fn audio_blocks_40ms() {
        let data = b"som rapido";
        assert!(roundtrip(data, 40.0, 8.0, 2.0, 3).ends_with(data));
    }

    #[test]
    fn toggle_detects_right_rhythm_only() {
        let feed = |tx_ms: f64, rx_ms: f64| {
            let mut d = ToggleDetector::new(rx_ms);
            sample(&toggle_bits(3000.0, tx_ms), tx_ms, 33.3, 0.0, 1).into_iter().any(|(t, l)| d.push(t, l))
        };
        assert!(feed(150.0, 150.0));
        assert!(!feed(400.0, 150.0));
    }

    #[test]
    fn preamble_alone_is_not_phase_1() {
        let mut d = ToggleDetector::new(100.0);
        let hits = sample(&bytes_to_bits(b"abc"), 100.0, 33.3, 0.0, 2).into_iter().filter(|&(t, l)| d.push(t, l)).count();
        assert_eq!(hits, 0);
    }
}
