//! Núcleo do Novo Messenger.
//!
//! Uma única implementação do protocolo e do processamento de sinal, usada
//! pelo navegador (WebAssembly) e pelos binários nativos.
//!
//! Camadas, de baixo para cima:
//! - [`light`] e [`fsk`]: transformam amostras físicas (brilho da câmera, áudio PCM) em nível 0/1.
//! - [`uart`]: transforma níveis 0/1 com tempo em bytes, e bytes em bits para transmitir.
//! - [`frame`]: delimita mensagens (STX, texto UTF-8, checksum, ETX).
//! - [`session`]: junta tudo e produz eventos, incluindo as fases de teste.
//! - [`mesh`]: codifica e decodifica mensagens de texto do rádio Meshtastic (LoRa).

pub mod frame;
pub mod fsk;
pub mod json;
pub mod light;
pub mod mesh;
pub mod session;
pub mod uart;

/// Bytes enviados na fase 2: padrões alternados e "ABC".
pub const TEST_BYTES: [u8; 7] = [0x55, 0xAA, 0x0F, 0xF0, 0x41, 0x42, 0x43];
/// Texto enviado na fase 3.
pub const TEST_TEXT: &str = "TESTE-OK 123";
/// Tamanho máximo do texto de uma mensagem, em bytes UTF-8.
pub const MAX_TEXT_BYTES: usize = 200;

/// O que transmitir em cada fase.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Payload {
    /// Fase 1: 1010… por cerca de 4 s.
    Toggle,
    /// Fase 2: [`TEST_BYTES`] sem quadro.
    TestBytes,
    /// Fases 3 e 4: um quadro com texto.
    Text,
}

/// Sequência de bits (0/1) para transmitir por luz ou som.
pub fn tx_bits(payload: Payload, text: &str, bit_ms: f64) -> Vec<u8> {
    match payload {
        Payload::Toggle => uart::toggle_bits(4000.0, bit_ms),
        Payload::TestBytes => uart::bytes_to_bits(&TEST_BYTES),
        Payload::Text => uart::bytes_to_bits(&frame::encode(text)),
    }
}
