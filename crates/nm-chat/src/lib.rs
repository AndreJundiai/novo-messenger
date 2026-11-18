//! Conversas do Novo Messenger: identidade sem telefone, criptografia de ponta
//! a ponta, conversas e confirmações (✓ enviada · ✓✓ entregue · ✓✓ azul lida).
//!
//! Não depende de nenhum transporte: o app entrega os envelopes por onde
//! puder (Bluetooth, Wi-Fi direto, LoRa, luz, som, QR) e devolve o que
//! receber para [`Chat::receive`].

pub mod backup;
pub mod card;
pub mod chat;
pub mod crypto;
pub mod nostr;
pub mod shield;
mod wire;

pub use card::{parse_card, Card};
pub use chat::{Chat, ChatError, ChatMessage, Contact, Conversation, Outgoing, Received, Status};
pub use crypto::{Identity, KeyBytes};

/// Identificador legível de uma chave: 64 dígitos hexadecimais.
pub fn key_id(k: &KeyBytes) -> String {
    k.iter().map(|b| format!("{b:02x}")).collect()
}

pub fn parse_key_id(s: &str) -> Option<KeyBytes> {
    let s = s.trim();
    if s.len() != 64 || !s.is_ascii() {
        return None;
    }
    let mut k = [0u8; 32];
    for (i, b) in k.iter_mut().enumerate() {
        *b = u8::from_str_radix(&s[i * 2..i * 2 + 2], 16).ok()?;
    }
    Some(k)
}

/// Código curto para conferir pessoalmente que a chave é a certa
/// (ex.: "4F2A 91C0 7B3E"), igual ao "código de segurança" de outros apps.
pub fn safety_code(k: &KeyBytes) -> String {
    use sha2::{Digest, Sha256};
    let h = Sha256::new().chain_update(b"nm-safety").chain_update(k).finalize();
    format!("{:02X}{:02X} {:02X}{:02X} {:02X}{:02X}", h[0], h[1], h[2], h[3], h[4], h[5])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn key_id_roundtrip() {
        let k: KeyBytes = core::array::from_fn(|i| i as u8 * 7);
        assert_eq!(parse_key_id(&key_id(&k)), Some(k));
        assert_eq!(parse_key_id("abc"), None);
        assert_eq!(safety_code(&k).len(), 14);
        assert_ne!(safety_code(&k), safety_code(&[1u8; 32]));
    }
}
