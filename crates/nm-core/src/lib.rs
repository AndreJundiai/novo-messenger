//! Núcleo do Novo Messenger.
//!
//! Uma única implementação do protocolo e do processamento de sinal, usada
//! pelo navegador (WebAssembly) e pelos binários nativos.
//!
//! Camadas, de baixo para cima:
//! - [`light`] e [`fsk`]: transformam amostras físicas (brilho da câmera, áudio PCM) em nível 0/1.
//! - [`uart`]: transforma níveis 0/1 com tempo em bytes, e bytes em bits para transmitir.
//! - [`frame`] + [`rs`]: delimitam o bloco de dados e corrigem bytes errados (Reed-Solomon).
//! - [`message`]: mensagem com id, relógio lógico e TTL de repasse.
//! - [`session`]: junta tudo e produz eventos, incluindo as fases de teste.
//! - [`store`]: histórico local-first (CRDT) com exportação e mesclagem.
//! - [`qr`]: gerador de QR Code (a mensagem inteira em uma imagem).
//! - [`mesh`]: codifica e decodifica mensagens de texto do rádio Meshtastic (LoRa).

pub mod b64;
pub mod frame;
pub mod fsk;
pub mod json;
pub mod light;
pub mod mesh;
pub mod message;
pub mod qr;
pub mod rs;
pub mod session;
pub mod store;
pub mod uart;

use message::Message;

/// Bytes enviados na fase 2: padrões alternados e "ABC".
pub const TEST_BYTES: [u8; 7] = [0x55, 0xAA, 0x0F, 0xF0, 0x41, 0x42, 0x43];
/// Texto enviado na fase 3.
pub const TEST_TEXT: &str = "TESTE-OK 123";
/// Tamanho máximo do texto de uma mensagem, em bytes UTF-8.
pub const MAX_TEXT_BYTES: usize = 200;
/// Prefixo do texto de um QR Code do Novo Messenger.
pub const QR_PREFIX: &str = "NM2:";

/// Mensagem da fase 3 (id 0: nunca entra no histórico).
pub fn test_message() -> Message {
    Message::new(0, 0, 0, TEST_TEXT)
}

/// O que transmitir.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Payload<'a> {
    /// Fase 1: 1010… por cerca de 4 s.
    Toggle,
    /// Fase 2: [`TEST_BYTES`] sem quadro.
    TestBytes,
    /// Fases 3 e 4: uma mensagem num quadro com correção de erros.
    Message(&'a Message),
}

/// Sequência de bits (0/1) para transmitir por luz ou som.
pub fn tx_bits(payload: Payload, bit_ms: f64) -> Vec<u8> {
    match payload {
        Payload::Toggle => uart::toggle_bits(4000.0, bit_ms),
        Payload::TestBytes => uart::bytes_to_bits(&TEST_BYTES),
        Payload::Message(m) => uart::bytes_to_bits(&frame::encode(&m.to_bytes())),
    }
}

/// Texto do QR Code de uma mensagem: prefixo + quadro com correção, em Base64.
pub fn qr_text(m: &Message) -> String {
    format!("{QR_PREFIX}{}", b64::encode(&frame::encode(&m.to_bytes())))
}

/// Lê o texto de um QR Code do Novo Messenger. Devolve a mensagem e quantos
/// bytes o Reed-Solomon corrigiu.
pub fn parse_qr_text(s: &str) -> Option<(Message, usize)> {
    let bytes = b64::decode(s.trim().strip_prefix(QR_PREFIX)?)?;
    let mut dec = frame::FrameDecoder::new();
    bytes.iter().find_map(|&b| match dec.push(b) {
        Some(frame::FrameEvent::Data { data, fixed }) => Some(Message::from_bytes(&data).map(|m| (m, fixed))),
        Some(frame::FrameEvent::Bad(_)) => Some(None),
        None => None,
    })?
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn qr_text_roundtrip() {
        let m = Message::new(7, 3, 2, "Corte às 15h ✂️");
        let s = qr_text(&m);
        assert!(s.starts_with(QR_PREFIX));
        assert_eq!(parse_qr_text(&s), Some((m, 0)));
        assert_eq!(parse_qr_text("outra coisa"), None);
        assert_eq!(parse_qr_text("NM2:!!"), None);
    }
}
