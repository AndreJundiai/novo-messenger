//! Quadro de mensagem: `STX (0x02)` + texto UTF-8 + checksum + `ETX (0x03)`.
//!
//! O checksum é o XOR dos bytes do texto, forçado para 0x80..=0xFF. Assim ele
//! nunca se confunde com STX ou ETX, e o texto (sem caracteres de controle)
//! também não.

use crate::MAX_TEXT_BYTES;

pub const STX: u8 = 0x02;
pub const ETX: u8 = 0x03;
const MAX_FRAME: usize = 400;

pub fn checksum(data: &[u8]) -> u8 {
    0x80 | (data.iter().fold(0u8, |a, b| a ^ b) & 0x7F)
}

/// Limpa o texto: troca caracteres de controle por espaço, apara as pontas e
/// corta em [`MAX_TEXT_BYTES`] sem quebrar um caractere UTF-8 ao meio.
pub fn sanitize(text: &str) -> String {
    let cleaned: String = text.chars().map(|c| if c.is_control() { ' ' } else { c }).collect();
    let trimmed = cleaned.trim();
    let mut end = trimmed.len().min(MAX_TEXT_BYTES);
    while !trimmed.is_char_boundary(end) {
        end -= 1;
    }
    trimmed[..end].to_string()
}

pub fn encode(text: &str) -> Vec<u8> {
    let text = sanitize(text);
    let data = text.as_bytes();
    let mut out = Vec::with_capacity(data.len() + 3);
    out.push(STX);
    out.extend_from_slice(data);
    out.push(checksum(data));
    out.push(ETX);
    out
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BadFrame {
    Checksum,
    InvalidUtf8,
    TooLong,
}

impl BadFrame {
    pub fn describe(&self) -> &'static str {
        match self {
            BadFrame::Checksum => "checksum errado",
            BadFrame::InvalidUtf8 => "texto inválido",
            BadFrame::TooLong => "quadro longo demais",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FrameEvent {
    Text(String),
    Bad(BadFrame),
}

#[derive(Default)]
pub struct FrameDecoder {
    buf: Option<Vec<u8>>,
}

impl FrameDecoder {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn push(&mut self, b: u8) -> Option<FrameEvent> {
        if b == STX {
            self.buf = Some(Vec::with_capacity(64));
            return None;
        }
        let buf = self.buf.as_mut()?;
        if b == ETX {
            let mut buf = self.buf.take()?;
            let cs = buf.pop()?;
            if buf.is_empty() {
                return None;
            }
            if checksum(&buf) != cs {
                return Some(FrameEvent::Bad(BadFrame::Checksum));
            }
            return Some(match String::from_utf8(buf) {
                Ok(s) => FrameEvent::Text(s),
                Err(_) => FrameEvent::Bad(BadFrame::InvalidUtf8),
            });
        }
        buf.push(b);
        if buf.len() > MAX_FRAME {
            self.buf = None;
            return Some(FrameEvent::Bad(BadFrame::TooLong));
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn decode_all(bytes: &[u8]) -> Vec<FrameEvent> {
        let mut d = FrameDecoder::new();
        bytes.iter().filter_map(|&b| d.push(b)).collect()
    }

    #[test]
    fn roundtrip_unicode() {
        let f = encode("Olá, corte às 15h ✂️");
        assert_eq!(decode_all(&f), vec![FrameEvent::Text("Olá, corte às 15h ✂️".into())]);
    }

    #[test]
    fn checksum_never_collides_with_delimiters() {
        for x in 0..=255u8 {
            let c = checksum(&[x]);
            assert!(c >= 0x80);
        }
    }

    #[test]
    fn corrupted_frame_is_rejected() {
        let mut f = encode("teste");
        f[3] ^= 0x01;
        assert_eq!(decode_all(&f), vec![FrameEvent::Bad(BadFrame::Checksum)]);
    }

    #[test]
    fn garbage_before_frame_is_ignored() {
        let mut v = vec![0xAA, 0x55, 0x41];
        v.extend(encode("ok"));
        assert_eq!(decode_all(&v), vec![FrameEvent::Text("ok".into())]);
    }

    #[test]
    fn sanitize_cuts_on_char_boundary() {
        let s = "é".repeat(150); // 300 bytes
        let out = sanitize(&s);
        assert!(out.len() <= MAX_TEXT_BYTES);
        assert!(out.chars().all(|c| c == 'é'));
        assert_eq!(sanitize("a\u{0}b\n"), "a b");
    }
}
