//! Quadro com correção de erros, para luz e som.
//!
//! `SYNC (02 D5)` + `L` + `!L` + `L bytes de dados` + `paridade Reed-Solomon`.
//!
//! O tamanho vai duas vezes (normal e invertido) para rejeitar sincronismos
//! falsos. A paridade cresce com o tamanho (¼ dos dados, entre 8 e 48 bytes)
//! e corrige até metade disso em bytes trocados, em qualquer posição.

use crate::rs;

pub const SYNC: [u8; 2] = [0x02, 0xD5];
/// Maior bloco de dados: cabeçalho da mensagem + texto máximo.
pub const MAX_DATA: usize = crate::message::HEADER + crate::MAX_TEXT_BYTES;

/// Bytes de paridade para `len` bytes de dados (a palavra nunca passa de 255).
pub fn nsym_for(len: usize) -> usize {
    (len / 4).clamp(8, 48).min(255 - len)
}

/// Monta o quadro. Dados acima de [`MAX_DATA`] são cortados.
pub fn encode(data: &[u8]) -> Vec<u8> {
    let data = &data[..data.len().min(MAX_DATA)];
    let len = data.len() as u8;
    let mut out = Vec::with_capacity(4 + data.len() + nsym_for(data.len()));
    out.extend_from_slice(&SYNC);
    out.push(len);
    out.push(!len);
    out.extend_from_slice(data);
    out.extend(rs::parity(data, nsym_for(data.len())));
    out
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BadFrame {
    Length,
    Uncorrectable,
}

impl BadFrame {
    pub fn describe(&self) -> &'static str {
        match self {
            BadFrame::Length => "tamanho inválido",
            BadFrame::Uncorrectable => "erros demais para corrigir",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FrameEvent {
    /// Dados íntegros; `fixed` bytes foram corrigidos pelo Reed-Solomon.
    Data { data: Vec<u8>, fixed: usize },
    Bad(BadFrame),
}

enum State {
    Hunt,
    Len,
    LenCheck(u8),
    Body { len: usize, total: usize, buf: Vec<u8> },
}

pub struct FrameDecoder {
    state: State,
    prev: u8,
}

impl Default for FrameDecoder {
    fn default() -> Self {
        Self { state: State::Hunt, prev: 0 }
    }
}

impl FrameDecoder {
    pub fn new() -> Self {
        Self::default()
    }

    /// Volta a procurar o sincronismo (ex.: depois de um silêncio longo).
    pub fn reset(&mut self) {
        *self = Self::default();
    }

    pub fn push(&mut self, b: u8) -> Option<FrameEvent> {
        let prev = std::mem::replace(&mut self.prev, b);
        match &mut self.state {
            State::Hunt => {
                if prev == SYNC[0] && b == SYNC[1] {
                    self.state = State::Len;
                }
                None
            }
            State::Len => {
                if b == 0 || b as usize > MAX_DATA {
                    self.state = State::Hunt;
                    return Some(FrameEvent::Bad(BadFrame::Length));
                }
                self.state = State::LenCheck(b);
                None
            }
            &mut State::LenCheck(len) => {
                if b != !len {
                    self.state = State::Hunt;
                    return Some(FrameEvent::Bad(BadFrame::Length));
                }
                let len = len as usize;
                let total = len + nsym_for(len);
                self.state = State::Body { len, total, buf: Vec::with_capacity(total) };
                None
            }
            State::Body { len, total, buf } => {
                buf.push(b);
                if buf.len() < *total {
                    return None;
                }
                let (len, mut cw) = (*len, std::mem::take(buf));
                self.state = State::Hunt;
                self.prev = 0;
                Some(match rs::correct(&mut cw, nsym_for(len)) {
                    Some(fixed) => {
                        cw.truncate(len);
                        FrameEvent::Data { data: cw, fixed }
                    }
                    None => FrameEvent::Bad(BadFrame::Uncorrectable),
                })
            }
        }
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
    fn roundtrip_with_garbage_before() {
        let data = "Olá ✂️".as_bytes();
        let mut v = vec![0xAA, 0x55, 0x02, 0x41];
        v.extend(encode(data));
        assert_eq!(decode_all(&v), vec![FrameEvent::Data { data: data.to_vec(), fixed: 0 }]);
    }

    #[test]
    fn corrects_flipped_bytes_including_sync_free_body() {
        let data: Vec<u8> = (0..100).collect();
        let mut f = encode(&data);
        let nsym = nsym_for(100); // 25 → corrige 12
        for i in 0..nsym / 2 {
            f[4 + i * 9] ^= 0x5A;
        }
        assert_eq!(decode_all(&f), vec![FrameEvent::Data { data, fixed: nsym / 2 }]);
    }

    #[test]
    fn reports_uncorrectable_and_bad_length() {
        let data = b"teste curto";
        let mut f = encode(data);
        for b in f.iter_mut().skip(4).take(10) {
            *b ^= 0xFF;
        }
        assert_eq!(decode_all(&f), vec![FrameEvent::Bad(BadFrame::Uncorrectable)]);
        assert_eq!(decode_all(&[0x02, 0xD5, 5, 5]), vec![FrameEvent::Bad(BadFrame::Length)]);
    }

    #[test]
    fn max_size_fits_one_codeword() {
        assert_eq!(MAX_DATA + nsym_for(MAX_DATA), 255);
        let data = vec![0x42; 400];
        let f = encode(&data);
        assert_eq!(decode_all(&f), vec![FrameEvent::Data { data: vec![0x42; MAX_DATA], fixed: 0 }]);
    }
}
