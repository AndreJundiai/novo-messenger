//! Cadeia completa de recepção de um canal (luz ou som) e as fases de teste.
//!
//! amostras físicas → nível 0/1 → bytes → quadros → eventos
//!
//! Fases: 1 sinal 1010… no ritmo certo · 2 bytes de teste exatos ·
//! 3 quadro de teste íntegro · 4 mensagem livre.

use crate::frame::{BadFrame, FrameDecoder, FrameEvent};
use crate::fsk::{FskConfig, FskDemod, FskSample};
use crate::light::LightDetector;
use crate::uart::{ToggleDetector, Uart};
use crate::{json, TEST_BYTES, TEST_TEXT};
use std::collections::VecDeque;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Event {
    Byte(u8),
    /// Fase (1 a 4) que acabou de passar.
    Phase(u8),
    Message(String),
    Bad(BadFrame),
}

/// Último estado do sinal, para a interface.
#[derive(Clone, Copy, Debug, Default)]
pub struct Status {
    pub level: u8,
    /// Luz: brilho 0..255. Som: amplitude do tom "0" em dBFS.
    pub a: f32,
    /// Luz: contraste 0..255. Som: amplitude do tom "1" em dBFS.
    pub b: f32,
    pub has_signal: bool,
}

enum Front {
    Light(LightDetector),
    Sound(FskDemod, Vec<FskSample>),
}

pub struct Receiver {
    front: Front,
    uart: Uart,
    toggle: ToggleDetector,
    frames: FrameDecoder,
    recent: VecDeque<u8>,
    muted: bool,
    events: Vec<Event>,
    scope: Vec<(f64, f32, u8)>,
    status: Status,
}

fn db(a: f32) -> f32 {
    20.0 * a.max(1e-6).log10()
}

impl Receiver {
    fn with_front(front: Front, bit_ms: f64) -> Self {
        Self {
            front,
            uart: Uart::new(bit_ms),
            toggle: ToggleDetector::new(bit_ms),
            frames: FrameDecoder::new(),
            recent: VecDeque::with_capacity(TEST_BYTES.len() + 1),
            muted: false,
            events: Vec::new(),
            scope: Vec::new(),
            status: Status::default(),
        }
    }

    pub fn light(bit_ms: f64) -> Self {
        Self::with_front(Front::Light(LightDetector::new()), bit_ms)
    }

    pub fn sound(cfg: FskConfig) -> Self {
        Self::with_front(Front::Sound(FskDemod::new(cfg), Vec::new()), cfg.bit_ms)
    }

    /// Enquanto o próprio aparelho transmite, o gráfico continua mas nada é decodificado.
    pub fn set_muted(&mut self, muted: bool) {
        self.muted = muted;
    }

    /// Luz: uma medição de brilho (0..255) no instante `t_ms`.
    pub fn push_luma(&mut self, t_ms: f64, luma: f32) {
        let Front::Light(det) = &mut self.front else { return };
        let s = det.push(t_ms, luma);
        self.status = Status { level: s.level, a: s.luma, b: s.contrast, has_signal: s.contrast > crate::light::MIN_CONTRAST };
        self.on_level(t_ms, s.level, s.norm);
    }

    /// Som: amostras PCM mono (-1..1), contínuas desde o início da escuta.
    pub fn push_pcm(&mut self, pcm: &[f32]) {
        let Front::Sound(demod, scratch) = &mut self.front else { return };
        scratch.clear();
        demod.push(pcm, scratch);
        let samples = std::mem::take(scratch);
        for s in &samples {
            self.status = Status { level: s.level, a: db(s.amp0), b: db(s.amp1), has_signal: s.value != 0.0 };
            self.on_level(s.t_ms, s.level, 0.5 + s.value * 0.5);
        }
        if let Front::Sound(_, scratch) = &mut self.front {
            *scratch = samples;
        }
    }

    fn on_level(&mut self, t: f64, level: u8, norm: f32) {
        self.scope.push((t, norm.clamp(0.0, 1.0), level));
        if self.scope.len() > 2048 {
            self.scope.drain(..1024);
        }
        if self.muted {
            return;
        }
        if self.toggle.push(t, level) {
            self.events.push(Event::Phase(1));
        }
        let Some(byte) = self.uart.push(t, level) else { return };
        self.events.push(Event::Byte(byte));
        if self.recent.len() == TEST_BYTES.len() {
            self.recent.pop_front();
        }
        self.recent.push_back(byte);
        if self.recent.iter().eq(TEST_BYTES.iter()) {
            self.events.push(Event::Phase(2));
        }
        match self.frames.push(byte) {
            Some(FrameEvent::Text(t)) if t == TEST_TEXT => self.events.push(Event::Phase(3)),
            Some(FrameEvent::Text(t)) => {
                self.events.push(Event::Message(t));
                self.events.push(Event::Phase(4));
            }
            Some(FrameEvent::Bad(b)) => self.events.push(Event::Bad(b)),
            None => {}
        }
    }

    pub fn status(&self) -> Status {
        self.status
    }

    pub fn take_events(&mut self) -> Vec<Event> {
        std::mem::take(&mut self.events)
    }

    /// Esvazia eventos e pontos do gráfico como JSON:
    /// `{"st":{"l":0,"a":0,"b":0,"sig":false},"s":[[t,v,l],…],"e":[{"k":"byte","v":85},…]}`
    pub fn drain_json(&mut self) -> String {
        let mut o = String::with_capacity(64 + self.scope.len() * 24);
        let st = self.status;
        o.push_str("{\"st\":{\"l\":");
        o.push_str(if st.level != 0 { "1" } else { "0" });
        o.push_str(",\"a\":");
        json::number(&mut o, st.a as f64);
        o.push_str(",\"b\":");
        json::number(&mut o, st.b as f64);
        o.push_str(",\"sig\":");
        o.push_str(if st.has_signal { "true" } else { "false" });
        o.push_str("},\"s\":[");
        for (i, &(t, v, l)) in self.scope.iter().enumerate() {
            if i > 0 {
                o.push(',');
            }
            o.push('[');
            json::number(&mut o, t);
            o.push(',');
            json::number(&mut o, v as f64);
            o.push(',');
            o.push(if l != 0 { '1' } else { '0' });
            o.push(']');
        }
        o.push_str("],\"e\":[");
        for (i, e) in self.events.iter().enumerate() {
            if i > 0 {
                o.push(',');
            }
            match e {
                Event::Byte(b) => o.push_str(&format!("{{\"k\":\"byte\",\"v\":{b}}}")),
                Event::Phase(p) => o.push_str(&format!("{{\"k\":\"phase\",\"v\":{p}}}")),
                Event::Message(t) => {
                    o.push_str("{\"k\":\"msg\",\"v\":");
                    json::string(&mut o, t);
                    o.push('}');
                }
                Event::Bad(b) => {
                    o.push_str("{\"k\":\"bad\",\"v\":");
                    json::string(&mut o, b.describe());
                    o.push('}');
                }
            }
        }
        o.push_str("]}");
        self.scope.clear();
        self.events.clear();
        o
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fsk::synth;
    use crate::uart::tests::sample;
    use crate::{tx_bits, Payload};

    fn phases(events: &[Event]) -> Vec<u8> {
        events.iter().filter_map(|e| if let Event::Phase(p) = e { Some(*p) } else { None }).collect()
    }

    /// Simula a sessão inteira: as 4 fases em sequência, com silêncio entre elas.
    fn all_phases(bit_ms: f64) -> Vec<u8> {
        let gap = vec![0u8; (1000.0 / bit_ms) as usize];
        let mut bits = gap.clone();
        for (p, text) in [(Payload::Toggle, ""), (Payload::TestBytes, ""), (Payload::Text, TEST_TEXT), (Payload::Text, "Olá ✂️")] {
            bits.extend(tx_bits(p, text, bit_ms));
            bits.extend(&gap);
        }
        bits
    }

    #[test]
    fn light_session_passes_all_phases() {
        let mut rx = Receiver::light(120.0);
        for (t, l) in sample(&all_phases(120.0), 120.0, 33.3, 12.0, 5) {
            rx.push_luma(t, if l == 1 { 200.0 } else { 30.0 });
        }
        let ev = rx.take_events();
        assert_eq!(phases(&ev), vec![1, 2, 3, 4]);
        assert!(ev.contains(&Event::Message("Olá ✂️".into())));
    }

    #[test]
    fn sound_session_passes_all_phases() {
        let cfg = FskConfig::new(48_000.0, 1500.0, 2500.0, 60.0);
        let pcm = synth(&all_phases(60.0), &cfg, 300.0, 0.2);
        let mut rx = Receiver::sound(cfg);
        for c in pcm.chunks(128) {
            rx.push_pcm(c);
        }
        assert_eq!(phases(&rx.take_events()), vec![1, 2, 3, 4]);
    }

    #[test]
    fn muted_receiver_decodes_nothing() {
        let mut rx = Receiver::light(120.0);
        rx.set_muted(true);
        for (t, l) in sample(&all_phases(120.0), 120.0, 33.3, 0.0, 1) {
            rx.push_luma(t, if l == 1 { 200.0 } else { 30.0 });
        }
        assert!(rx.take_events().is_empty());
    }

    #[test]
    fn json_output_is_well_formed() {
        let mut rx = Receiver::light(100.0);
        rx.events.push(Event::Message("diz \"oi\"".into()));
        rx.push_luma(0.0, 10.0);
        let j = rx.drain_json();
        assert!(j.starts_with("{\"st\":{\"l\":0"));
        assert!(j.contains(r#"{"k":"msg","v":"diz \"oi\""}"#));
        assert!(j.ends_with("]}"));
    }
}
