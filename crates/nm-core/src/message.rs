//! Mensagem: o que viaja dentro de um quadro (luz, som, QR) e o que fica no
//! histórico.
//!
//! Formato (7 bytes de cabeçalho + texto UTF-8):
//! `[versão:4 | ttl:4] [id: u32 LE] [lamport: u16 LE] [texto…]`
//!
//! - `id` aleatório identifica a mensagem em todos os aparelhos (sem cópias).
//! - `lamport` é um relógio lógico: ordena a conversa igual em todo lugar,
//!   sem depender do relógio de cada aparelho.
//! - `ttl` diz quantas vezes ela ainda pode ser repassada por pontes.

pub const VERSION: u8 = 2;
pub const HEADER: usize = 7;
pub const MAX_TTL: u8 = 15;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Message {
    pub id: u32,
    pub lamport: u16,
    pub ttl: u8,
    pub text: String,
}

/// Limpa o texto: troca caracteres de controle por espaço, apara as pontas e
/// corta em [`crate::MAX_TEXT_BYTES`] sem quebrar um caractere UTF-8 ao meio.
pub fn sanitize(text: &str) -> String {
    let cleaned: String = text.chars().map(|c| if c.is_control() { ' ' } else { c }).collect();
    let trimmed = cleaned.trim();
    let mut end = trimmed.len().min(crate::MAX_TEXT_BYTES);
    while !trimmed.is_char_boundary(end) {
        end -= 1;
    }
    trimmed[..end].to_string()
}

impl Message {
    pub fn new(id: u32, lamport: u16, ttl: u8, text: &str) -> Self {
        Self { id, lamport, ttl: ttl.min(MAX_TTL), text: sanitize(text) }
    }

    pub fn to_bytes(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(HEADER + self.text.len());
        out.push((VERSION << 4) | (self.ttl & 0x0F));
        out.extend_from_slice(&self.id.to_le_bytes());
        out.extend_from_slice(&self.lamport.to_le_bytes());
        out.extend_from_slice(self.text.as_bytes());
        out
    }

    pub fn from_bytes(b: &[u8]) -> Option<Self> {
        if b.len() < HEADER || b[0] >> 4 != VERSION {
            return None;
        }
        Some(Self {
            ttl: b[0] & 0x0F,
            id: u32::from_le_bytes(b[1..5].try_into().ok()?),
            lamport: u16::from_le_bytes(b[5..7].try_into().ok()?),
            text: String::from_utf8(b[HEADER..].to_vec()).ok()?,
        })
    }

    /// Cópia para repassar adiante, se ainda puder.
    pub fn relayed(&self) -> Option<Self> {
        (self.ttl > 0).then(|| Self { ttl: self.ttl - 1, ..self.clone() })
    }

    pub fn write_json(&self, o: &mut String) {
        o.push_str(&format!("{{\"id\":{},\"lamport\":{},\"ttl\":{},\"text\":", self.id, self.lamport, self.ttl));
        crate::json::string(o, &self.text);
        o.push('}');
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip() {
        let m = Message::new(0xDEAD_BEEF, 513, 3, "Olá ✂️");
        assert_eq!(Message::from_bytes(&m.to_bytes()), Some(m.clone()));
        assert_eq!(m.relayed().unwrap().ttl, 2);
        assert_eq!(Message::new(1, 1, 0, "x").relayed(), None);
    }

    #[test]
    fn rejects_other_versions_and_short_input() {
        let mut b = Message::new(1, 1, 1, "x").to_bytes();
        b[0] = 0x11;
        assert_eq!(Message::from_bytes(&b), None);
        assert_eq!(Message::from_bytes(&[0x20, 1, 2]), None);
    }

    #[test]
    fn sanitize_cuts_on_char_boundary() {
        let out = sanitize(&"é".repeat(150));
        assert!(out.len() <= crate::MAX_TEXT_BYTES && out.chars().all(|c| c == 'é'));
        assert_eq!(sanitize("a\u{0}b\n"), "a b");
        assert_eq!(Message::new(1, 1, 99, "x").ttl, MAX_TTL);
    }
}
