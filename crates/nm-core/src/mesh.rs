//! Mensagens de texto do Meshtastic (rádio LoRa) em protobuf, sem dependências.
//!
//! Só o necessário: `ToRadio{packet}` com `MeshPacket{to, decoded: Data{portnum, payload}, id}`,
//! `ToRadio{want_config_id}` para abrir a sessão, e a leitura de
//! `FromRadio{packet}` com texto (portnum 1 = TEXT_MESSAGE_APP).

pub const SERVICE_UUID: &str = "6ba1b218-15a8-461f-9fa8-5dcae273eafd";
pub const TO_RADIO_UUID: &str = "f75c76d2-129e-4dad-a1dd-7866124401e7";
pub const FROM_RADIO_UUID: &str = "2c55e69e-4993-11ed-b878-0242ac120002";
pub const FROM_NUM_UUID: &str = "ed9da18c-a800-4f66-a670-aa7547e34453";

pub const BROADCAST: u32 = 0xFFFF_FFFF;
const TEXT_MESSAGE_APP: u64 = 1;
/// Limite de texto do Meshtastic (bytes UTF-8).
pub const MAX_TEXT: usize = 220;

fn varint(mut n: u64, out: &mut Vec<u8>) {
    while n > 0x7F {
        out.push((n as u8 & 0x7F) | 0x80);
        n >>= 7;
    }
    out.push(n as u8);
}

fn len_field(field: u32, bytes: &[u8], out: &mut Vec<u8>) {
    varint(((field << 3) | 2) as u64, out);
    varint(bytes.len() as u64, out);
    out.extend_from_slice(bytes);
}

fn fixed32(field: u32, n: u32, out: &mut Vec<u8>) {
    varint(((field << 3) | 5) as u64, out);
    out.extend_from_slice(&n.to_le_bytes());
}

/// `ToRadio{want_config_id}`: inicia a sessão com o rádio.
pub fn want_config(id: u32) -> Vec<u8> {
    let mut out = vec![0x18];
    varint(id as u64, &mut out);
    out
}

/// `ToRadio{packet}` com uma mensagem de texto para todos (canal principal).
pub fn encode_text(text: &str, packet_id: u32) -> Vec<u8> {
    let mut end = text.len().min(MAX_TEXT);
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    let mut data = vec![0x08];
    varint(TEXT_MESSAGE_APP, &mut data);
    len_field(2, &text.as_bytes()[..end], &mut data);
    let mut pkt = Vec::new();
    fixed32(2, BROADCAST, &mut pkt);
    len_field(4, &data, &mut pkt);
    fixed32(6, packet_id, &mut pkt);
    let mut out = Vec::new();
    len_field(1, &pkt, &mut out);
    out
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Value<'a> {
    Varint(u64),
    Bytes(&'a [u8]),
    Fixed32(u32),
    Fixed64,
}

/// Percorre os campos de uma mensagem protobuf. Para no primeiro byte inválido.
struct Fields<'a> {
    buf: &'a [u8],
    pos: usize,
}

impl<'a> Fields<'a> {
    fn new(buf: &'a [u8]) -> Self {
        Self { buf, pos: 0 }
    }

    fn varint(&mut self) -> Option<u64> {
        let mut r = 0u64;
        for shift in (0..64).step_by(7) {
            let b = *self.buf.get(self.pos)?;
            self.pos += 1;
            r |= ((b & 0x7F) as u64) << shift;
            if b & 0x80 == 0 {
                return Some(r);
            }
        }
        None
    }

    fn take(&mut self, n: usize) -> Option<&'a [u8]> {
        let end = self.pos.checked_add(n)?;
        let s = self.buf.get(self.pos..end)?;
        self.pos = end;
        Some(s)
    }
}

impl<'a> Iterator for Fields<'a> {
    type Item = (u32, Value<'a>);
    fn next(&mut self) -> Option<Self::Item> {
        if self.pos >= self.buf.len() {
            return None;
        }
        let key = self.varint()?;
        let field = (key >> 3) as u32;
        let v = match key & 7 {
            0 => Value::Varint(self.varint()?),
            1 => {
                self.take(8)?;
                Value::Fixed64
            }
            2 => {
                let n = self.varint()? as usize;
                Value::Bytes(self.take(n)?)
            }
            5 => Value::Fixed32(u32::from_le_bytes(self.take(4)?.try_into().ok()?)),
            _ => return None,
        };
        Some((field, v))
    }
}

fn bytes_field(buf: &[u8], field: u32) -> Option<&[u8]> {
    Fields::new(buf).find_map(|(f, v)| match v {
        Value::Bytes(b) if f == field => Some(b),
        _ => None,
    })
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MeshText {
    pub from: u32,
    pub text: String,
}

/// Lê um `FromRadio`. Devolve o texto se for uma mensagem de texto.
pub fn decode_from_radio(buf: &[u8]) -> Option<MeshText> {
    let packet = bytes_field(buf, 2)?;
    let mut from = 0;
    let mut decoded = None;
    for (f, v) in Fields::new(packet) {
        match (f, v) {
            (1, Value::Fixed32(n)) => from = n,
            (4, Value::Bytes(b)) => decoded = Some(b),
            _ => {}
        }
    }
    let mut port = 0;
    let mut payload = None;
    for (f, v) in Fields::new(decoded?) {
        match (f, v) {
            (1, Value::Varint(n)) => port = n,
            (2, Value::Bytes(b)) => payload = Some(b),
            _ => {}
        }
    }
    if port != TEXT_MESSAGE_APP {
        return None;
    }
    Some(MeshText { from, text: String::from_utf8_lossy(payload?).into_owned() })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn varint_encoding() {
        let mut v = Vec::new();
        varint(300, &mut v);
        assert_eq!(v, [0xAC, 0x02]);
    }

    #[test]
    fn text_roundtrip_through_from_radio() {
        // Monta um FromRadio{id=5, packet=MeshPacket{from, decoded}} a partir do ToRadio gerado.
        let to_radio = encode_text("oi pelo LoRa ✂️", 42);
        let mut pkt = bytes_field(&to_radio, 1).unwrap().to_vec();
        fixed32(1, 0x1234_5678, &mut pkt);
        let mut from_radio = vec![0x08, 0x05];
        len_field(2, &pkt, &mut from_radio);
        assert_eq!(
            decode_from_radio(&from_radio),
            Some(MeshText { from: 0x1234_5678, text: "oi pelo LoRa ✂️".into() })
        );
    }

    #[test]
    fn non_text_and_garbage_are_ignored() {
        assert_eq!(decode_from_radio(&[0x08, 0x05]), None);
        assert_eq!(decode_from_radio(&[0x12, 0xFF, 0x01]), None);
        assert_eq!(decode_from_radio(&[0xFF; 12]), None);
    }

    #[test]
    fn long_text_is_cut_on_char_boundary() {
        let msg = "é".repeat(200);
        let out = encode_text(&msg, 1);
        let pkt = bytes_field(&out, 1).unwrap();
        let data = bytes_field(pkt, 4).unwrap();
        let text = bytes_field(data, 2).unwrap();
        assert!(text.len() <= MAX_TEXT);
        assert!(std::str::from_utf8(text).is_ok());
    }
}
