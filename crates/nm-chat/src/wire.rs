//! Conteúdo de dentro do envelope e leitura/escrita binária simples.

/// O que vai cifrado dentro do envelope.
///
/// Todo conteúdo (menos as confirmações) tem um `id` próprio, e quem recebe
/// devolve "entregue" com esse `id`. Assim, edições, exclusões e reações
/// também são reenviadas até chegar.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Body {
    /// Formato antigo (fase 1), ainda aceito.
    Text { id: u64, text: String },
    Delivered(Vec<u64>),
    Read(Vec<u64>),
    /// Mensagem, opcionalmente respondendo a outra.
    Message { id: u64, reply_to: Option<u64>, text: String },
    Edit { id: u64, target: u64, text: String },
    Delete { id: u64, target: u64 },
    /// Emoji vazio = tirar a reação.
    React { id: u64, target: u64, emoji: String },
    /// Prova de que quem envia recebeu o SMS de confirmação no próprio número.
    Verify { id: u64, code: String },
}

const TEXT: u8 = 1;
const DELIVERED: u8 = 2;
const READ: u8 = 3;
const MESSAGE: u8 = 4;
const EDIT: u8 = 5;
const DELETE: u8 = 6;
const REACT: u8 = 7;
const VERIFY: u8 = 8;
/// Quantas confirmações cabem num envelope.
pub const MAX_ACKS: usize = 64;
/// Maior emoji de reação, em bytes (alguns emojis compostos passam de 20).
pub const MAX_EMOJI_BYTES: usize = 32;

fn utf8(b: &[u8]) -> Option<String> {
    String::from_utf8(b.to_vec()).ok()
}

impl Body {
    /// `id` do conteúdo (para confirmar a entrega); `None` nas confirmações.
    pub fn id(&self) -> Option<u64> {
        match self {
            Body::Text { id, .. } | Body::Message { id, .. } | Body::Edit { id, .. } | Body::Delete { id, .. } | Body::React { id, .. } | Body::Verify { id, .. } => Some(*id),
            Body::Delivered(_) | Body::Read(_) => None,
        }
    }

    pub fn encode(&self) -> Vec<u8> {
        let mut w = Writer::default();
        match self {
            Body::Text { id, text } => {
                w.u8(TEXT);
                w.u64(*id);
                w.raw(text.as_bytes());
            }
            Body::Delivered(ids) | Body::Read(ids) => {
                w.u8(if matches!(self, Body::Delivered(_)) { DELIVERED } else { READ });
                for id in ids.iter().take(MAX_ACKS) {
                    w.u64(*id);
                }
            }
            Body::Message { id, reply_to, text } => {
                w.u8(MESSAGE);
                w.u64(*id);
                match reply_to {
                    Some(r) => {
                        w.u8(1);
                        w.u64(*r);
                    }
                    None => w.u8(0),
                }
                w.raw(text.as_bytes());
            }
            Body::Edit { id, target, text } => {
                w.u8(EDIT);
                w.u64(*id);
                w.u64(*target);
                w.raw(text.as_bytes());
            }
            Body::Delete { id, target } => {
                w.u8(DELETE);
                w.u64(*id);
                w.u64(*target);
            }
            Body::React { id, target, emoji } => {
                w.u8(REACT);
                w.u64(*id);
                w.u64(*target);
                w.raw(emoji.as_bytes());
            }
            Body::Verify { id, code } => {
                w.u8(VERIFY);
                w.u64(*id);
                w.raw(code.as_bytes());
            }
        }
        w.0
    }

    pub fn decode(b: &[u8]) -> Option<Self> {
        let mut r = Reader::new(b);
        Some(match r.u8()? {
            TEXT => {
                let id = r.u64()?;
                Body::Text { id, text: utf8(r.rest())? }
            }
            kind @ (DELIVERED | READ) => {
                let rest = r.rest();
                if !rest.len().is_multiple_of(8) || rest.is_empty() {
                    return None;
                }
                let ids = rest.chunks_exact(8).map(|c| u64::from_le_bytes(c.try_into().unwrap_or_default())).collect();
                if kind == DELIVERED {
                    Body::Delivered(ids)
                } else {
                    Body::Read(ids)
                }
            }
            MESSAGE => {
                let id = r.u64()?;
                let reply_to = match r.u8()? {
                    0 => None,
                    1 => Some(r.u64()?),
                    _ => return None,
                };
                Body::Message { id, reply_to, text: utf8(r.rest())? }
            }
            EDIT => {
                let (id, target) = (r.u64()?, r.u64()?);
                Body::Edit { id, target, text: utf8(r.rest())? }
            }
            DELETE => {
                let (id, target) = (r.u64()?, r.u64()?);
                if !r.done() {
                    return None;
                }
                Body::Delete { id, target }
            }
            REACT => {
                let (id, target) = (r.u64()?, r.u64()?);
                let emoji = utf8(r.rest())?;
                if emoji.len() > MAX_EMOJI_BYTES {
                    return None;
                }
                Body::React { id, target, emoji }
            }
            VERIFY => {
                let id = r.u64()?;
                Body::Verify { id, code: utf8(r.rest())? }
            }
            _ => return None,
        })
    }
}

#[derive(Default)]
pub struct Writer(pub Vec<u8>);

impl Writer {
    pub fn u8(&mut self, v: u8) {
        self.0.push(v);
    }
    pub fn u32(&mut self, v: u32) {
        self.0.extend_from_slice(&v.to_le_bytes());
    }
    pub fn u64(&mut self, v: u64) {
        self.0.extend_from_slice(&v.to_le_bytes());
    }
    pub fn raw(&mut self, b: &[u8]) {
        self.0.extend_from_slice(b);
    }
    /// Bytes com tamanho (u32) na frente.
    pub fn bytes(&mut self, b: &[u8]) {
        self.u32(b.len() as u32);
        self.raw(b);
    }
}

pub struct Reader<'a> {
    b: &'a [u8],
    pos: usize,
}

impl<'a> Reader<'a> {
    pub fn new(b: &'a [u8]) -> Self {
        Self { b, pos: 0 }
    }
    pub fn take(&mut self, n: usize) -> Option<&'a [u8]> {
        let end = self.pos.checked_add(n)?;
        let s = self.b.get(self.pos..end)?;
        self.pos = end;
        Some(s)
    }
    pub fn u8(&mut self) -> Option<u8> {
        Some(self.take(1)?[0])
    }
    pub fn u32(&mut self) -> Option<u32> {
        Some(u32::from_le_bytes(self.take(4)?.try_into().ok()?))
    }
    pub fn u64(&mut self) -> Option<u64> {
        Some(u64::from_le_bytes(self.take(8)?.try_into().ok()?))
    }
    pub fn key(&mut self) -> Option<[u8; 32]> {
        self.take(32)?.try_into().ok()
    }
    pub fn bytes(&mut self) -> Option<&'a [u8]> {
        let n = self.u32()? as usize;
        self.take(n)
    }
    pub fn string(&mut self) -> Option<String> {
        String::from_utf8(self.bytes()?.to_vec()).ok()
    }
    pub fn rest(&mut self) -> &'a [u8] {
        let s = &self.b[self.pos..];
        self.pos = self.b.len();
        s
    }
    pub fn done(&self) -> bool {
        self.pos == self.b.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bodies_roundtrip() {
        for b in [
            Body::Text { id: 7, text: "oi ✂️".into() },
            Body::Delivered(vec![1, 2, u64::MAX]),
            Body::Read(vec![3]),
            Body::Message { id: 8, reply_to: Some(7), text: "respondendo".into() },
            Body::Message { id: 9, reply_to: None, text: String::new() },
            Body::Edit { id: 10, target: 8, text: "corrigido".into() },
            Body::Delete { id: 11, target: 8 },
            Body::React { id: 12, target: 8, emoji: "👍🏽".into() },
            Body::React { id: 13, target: 8, emoji: String::new() },
            Body::Verify { id: 14, code: "K7Q2M9XW4T".into() },
        ] {
            assert_eq!(Body::decode(&b.encode()), Some(b));
        }
        assert_eq!(Body::decode(&[2, 1, 2]), None);
        assert_eq!(Body::decode(&[9]), None);
        assert_eq!(Body::decode(&[]), None);
        assert_eq!(Body::decode(&[4, 0, 0, 0, 0, 0, 0, 0, 0, 2]), None, "marcador de resposta inválido");
        assert_eq!(Body::React { id: 1, target: 2, emoji: "x".repeat(40) }.encode().len(), 57);
        assert_eq!(Body::decode(&Body::React { id: 1, target: 2, emoji: "x".repeat(40) }.encode()), None);
    }
}
