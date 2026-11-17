//! Histórico local-first como CRDT (conjunto que só cresce, G-Set).
//!
//! Cada aparelho guarda suas mensagens. Juntar dois históricos é a união dos
//! conjuntos pelo `id`: o resultado é o mesmo em qualquer ordem e quantas
//! vezes for feito (comutativo, associativo e idempotente), então dá para
//! sincronizar por arquivo, Bluetooth ou qualquer canal, sem servidor.
//! A conversa é ordenada por (lamport, id), igual em todos os aparelhos.

use crate::message::Message;
use std::collections::HashMap;

/// Por onde a mensagem chegou (bits combináveis).
pub mod via {
    pub const ME: u8 = 1;
    pub const LIGHT: u8 = 2;
    pub const SOUND: u8 = 4;
    pub const LORA: u8 = 8;
    pub const QR: u8 = 16;
    pub const FILE: u8 = 32;
}

/// Janela para tratar o mesmo texto vindo do LoRa como cópia (ms).
const TEXT_DEDUP_MS: f64 = 60_000.0;
const EXPORT_MAGIC: &[u8; 6] = b"NMLOG2";
const SAVE_MAGIC: &[u8; 6] = b"NMDB02";

#[derive(Clone, Debug, PartialEq)]
pub struct Entry {
    pub msg: Message,
    pub via: u8,
    /// Quando este aparelho viu a mensagem pela primeira vez (ms desde 1970).
    pub first_seen: f64,
}

#[derive(Default)]
pub struct Store {
    entries: HashMap<u32, Entry>,
    clock: u16,
}

impl Store {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    fn tick(&mut self, seen: u16) -> u16 {
        self.clock = self.clock.max(seen).wrapping_add(1);
        self.clock
    }

    /// Cria uma mensagem deste aparelho.
    pub fn compose(&mut self, text: &str, ttl: u8, id: u32, now: f64) -> Message {
        let msg = Message::new(id, self.tick(0), ttl, text);
        self.entries.insert(msg.id, Entry { msg: msg.clone(), via: via::ME, first_seen: now });
        msg
    }

    /// Registra uma mensagem recebida. Devolve `true` se for nova.
    pub fn receive(&mut self, msg: Message, how: u8, now: f64) -> bool {
        self.tick(msg.lamport);
        match self.entries.get_mut(&msg.id) {
            Some(e) => {
                e.via |= how;
                false
            }
            None => {
                self.entries.insert(msg.id, Entry { msg, via: how, first_seen: now });
                true
            }
        }
    }

    /// Texto puro (do LoRa, sem cabeçalho). Se o mesmo texto chegou há pouco,
    /// é cópia: só marca o canal e devolve `None`.
    pub fn receive_text(&mut self, text: &str, how: u8, id: u32, now: f64) -> Option<Message> {
        let text = crate::message::sanitize(text);
        if let Some(e) = self.entries.values_mut().find(|e| e.msg.text == text && now - e.first_seen < TEXT_DEDUP_MS) {
            e.via |= how;
            return None;
        }
        let msg = Message::new(id, self.tick(0), 0, &text);
        self.entries.insert(id, Entry { msg: msg.clone(), via: how, first_seen: now });
        Some(msg)
    }

    /// Conversa em ordem (lamport, id).
    pub fn sorted(&self) -> Vec<&Entry> {
        let mut v: Vec<&Entry> = self.entries.values().collect();
        v.sort_by_key(|e| (e.msg.lamport, e.msg.id));
        v
    }

    /// Junta outro histórico a este (CRDT). Devolve quantas mensagens são novas.
    pub fn merge(&mut self, other: &Store, now: f64) -> usize {
        other.sorted().into_iter().filter(|e| self.receive(e.msg.clone(), via::FILE, now)).count()
    }

    /// Arquivo para levar a outro aparelho: só as mensagens, sem dados locais.
    pub fn export(&self) -> Vec<u8> {
        let mut out = EXPORT_MAGIC.to_vec();
        for e in self.sorted() {
            let b = e.msg.to_bytes();
            out.push(b.len() as u8);
            out.extend(b);
        }
        out
    }

    /// Junta um arquivo de [`Store::export`]. `None` se o arquivo for inválido.
    pub fn import(&mut self, data: &[u8], now: f64) -> Option<usize> {
        let mut other = Store::new();
        let mut rest = data.strip_prefix(EXPORT_MAGIC)?;
        while let Some((&len, tail)) = rest.split_first() {
            let (body, tail) = tail.split_at_checked(len as usize)?;
            let msg = Message::from_bytes(body)?;
            other.entries.insert(msg.id, Entry { msg, via: via::FILE, first_seen: now });
            rest = tail;
        }
        Some(self.merge(&other, now))
    }

    /// Estado completo deste aparelho (inclui canais e horários), para salvar.
    pub fn save(&self) -> Vec<u8> {
        let mut out = SAVE_MAGIC.to_vec();
        out.extend_from_slice(&self.clock.to_le_bytes());
        for e in self.sorted() {
            let b = e.msg.to_bytes();
            out.push(e.via);
            out.extend_from_slice(&e.first_seen.to_le_bytes());
            out.push(b.len() as u8);
            out.extend(b);
        }
        out
    }

    pub fn load(data: &[u8]) -> Option<Store> {
        let rest = data.strip_prefix(SAVE_MAGIC)?;
        let (clock, mut rest) = rest.split_at_checked(2)?;
        let mut s = Store { clock: u16::from_le_bytes([clock[0], clock[1]]), ..Store::default() };
        while !rest.is_empty() {
            let (head, tail) = rest.split_at_checked(10)?;
            let first_seen = f64::from_le_bytes(head[1..9].try_into().ok()?);
            let (body, tail) = tail.split_at_checked(head[9] as usize)?;
            let msg = Message::from_bytes(body)?;
            s.entries.insert(msg.id, Entry { msg, via: head[0], first_seen });
            rest = tail;
        }
        Some(s)
    }

    pub fn clear(&mut self) {
        self.entries.clear();
    }

    /// `[{"id":…,"lamport":…,"ttl":…,"text":"…","via":…,"at":…}, …]`
    pub fn to_json(&self) -> String {
        let mut o = String::from("[");
        for (i, e) in self.sorted().into_iter().enumerate() {
            if i > 0 {
                o.push(',');
            }
            e.msg.write_json(&mut o);
            o.pop();
            o.push_str(&format!(",\"via\":{},\"at\":", e.via));
            crate::json::number(&mut o, e.first_seen);
            o.push('}');
        }
        o.push(']');
        o
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn texts(s: &Store) -> Vec<String> {
        s.sorted().iter().map(|e| e.msg.text.clone()).collect()
    }

    #[test]
    fn merge_is_commutative_and_idempotent() {
        let mut a = Store::new();
        let mut b = Store::new();
        a.compose("oi", 1, 10, 0.0);
        b.compose("olá", 1, 20, 0.0);
        let m = a.compose("tudo bem?", 1, 11, 0.0);
        b.receive(m, via::SOUND, 1.0);
        b.compose("tudo", 1, 21, 2.0);

        let mut ab = Store::new();
        ab.merge(&a, 0.0);
        ab.merge(&b, 0.0);
        let mut ba = Store::new();
        ba.merge(&b, 0.0);
        ba.merge(&a, 0.0);
        assert_eq!(texts(&ab), texts(&ba));
        assert_eq!(ab.merge(&a, 0.0) + ab.merge(&b, 0.0), 0);
        assert_eq!(ab.len(), 4);
        // "tudo" foi escrita depois de receber "tudo bem?", então vem depois.
        let order = texts(&ab);
        assert!(order.iter().position(|t| t == "tudo bem?") < order.iter().position(|t| t == "tudo"));
    }

    #[test]
    fn duplicate_from_two_channels_marks_both() {
        let mut s = Store::new();
        let m = Message::new(5, 1, 0, "x");
        assert!(s.receive(m.clone(), via::LIGHT, 0.0));
        assert!(!s.receive(m, via::SOUND, 0.0));
        assert_eq!(s.sorted()[0].via, via::LIGHT | via::SOUND);
    }

    #[test]
    fn lora_text_dedup_window() {
        let mut s = Store::new();
        assert!(s.receive_text("oi", via::LORA, 1, 0.0).is_some());
        assert!(s.receive_text("oi", via::SOUND, 2, 5_000.0).is_none());
        assert!(s.receive_text("oi", via::LORA, 3, 120_000.0).is_some());
        assert_eq!(s.len(), 2);
    }

    #[test]
    fn export_import_and_save_load() {
        let mut a = Store::new();
        a.compose("um", 2, 1, 100.0);
        a.receive(Message::new(2, 9, 0, "dois ✂️"), via::LORA, 200.0);
        let mut b = Store::new();
        assert_eq!(b.import(&a.export(), 300.0), Some(2));
        assert_eq!(texts(&b), texts(&a));
        assert!(b.import(b"lixo", 0.0).is_none());
        assert!(b.import(&[b"NMLOG2".as_slice(), &[50, 1]].concat(), 0.0).is_none());

        let loaded = Store::load(&a.save()).unwrap();
        assert_eq!(loaded.sorted(), a.sorted());
        assert_eq!(loaded.clock, a.clock);
        assert!(Store::load(b"NMDB02").is_none());
    }
}
