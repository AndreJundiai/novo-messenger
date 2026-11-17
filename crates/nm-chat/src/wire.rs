//! Conteúdo de dentro do envelope e leitura/escrita binária simples.

/// O que vai cifrado dentro do envelope.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Body {
    Text { id: u64, text: String },
    Delivered(Vec<u64>),
    Read(Vec<u64>),
}

const TEXT: u8 = 1;
const DELIVERED: u8 = 2;
const READ: u8 = 3;
/// Quantas confirmações cabem num envelope.
pub const MAX_ACKS: usize = 64;

impl Body {
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
        }
        w.0
    }

    pub fn decode(b: &[u8]) -> Option<Self> {
        let mut r = Reader::new(b);
        match r.u8()? {
            TEXT => {
                let id = r.u64()?;
                Some(Body::Text { id, text: String::from_utf8(r.rest().to_vec()).ok()? })
            }
            kind @ (DELIVERED | READ) => {
                let rest = r.rest();
                if !rest.len().is_multiple_of(8) || rest.is_empty() {
                    return None;
                }
                let ids = rest.chunks_exact(8).map(|c| u64::from_le_bytes(c.try_into().unwrap_or_default())).collect();
                Some(if kind == DELIVERED { Body::Delivered(ids) } else { Body::Read(ids) })
            }
            _ => None,
        }
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
        for b in [Body::Text { id: 7, text: "oi ✂️".into() }, Body::Delivered(vec![1, 2, u64::MAX]), Body::Read(vec![3])] {
            assert_eq!(Body::decode(&b.encode()), Some(b));
        }
        assert_eq!(Body::decode(&[2, 1, 2]), None);
        assert_eq!(Body::decode(&[9]), None);
        assert_eq!(Body::decode(&[]), None);
    }
}
