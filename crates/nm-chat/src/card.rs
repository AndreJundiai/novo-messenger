//! Cartão de contato: o que o QR "Meu código" mostra.
//! Formato: `NMC1:` + Base64(chave pública 32 bytes ‖ nome UTF-8).

use crate::crypto::KeyBytes;
use nm_core::b64;

pub const PREFIX: &str = "NMC1:";
pub const MAX_NAME_BYTES: usize = 40;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Card {
    pub key: KeyBytes,
    pub name: String,
}

/// Limpa o nome: sem caracteres de controle, aparado, no máximo 40 bytes.
pub fn clean_name(name: &str) -> String {
    let s: String = name.chars().filter(|c| !c.is_control()).collect();
    let s = s.trim();
    let mut end = s.len().min(MAX_NAME_BYTES);
    while !s.is_char_boundary(end) {
        end -= 1;
    }
    s[..end].trim_end().to_string()
}

pub fn make_card(key: &KeyBytes, name: &str) -> String {
    let mut bytes = key.to_vec();
    bytes.extend_from_slice(clean_name(name).as_bytes());
    format!("{PREFIX}{}", b64::encode(&bytes))
}

pub fn parse_card(s: &str) -> Option<Card> {
    let bytes = b64::decode(s.trim().strip_prefix(PREFIX)?)?;
    if bytes.len() < 32 || bytes.len() > 32 + MAX_NAME_BYTES {
        return None;
    }
    let key: KeyBytes = bytes[..32].try_into().ok()?;
    let name = String::from_utf8(bytes[32..].to_vec()).ok()?;
    Some(Card { key, name: clean_name(&name) })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_and_limits() {
        let k = [9u8; 32];
        let c = make_card(&k, "  Barbearia do André ✂️\n ");
        assert!(c.starts_with(PREFIX));
        assert_eq!(parse_card(&c), Some(Card { key: k, name: "Barbearia do André ✂️".into() }));
        assert_eq!(parse_card(&make_card(&k, &"é".repeat(50))).unwrap().name.len(), 40);
        assert_eq!(parse_card("NM2:abc"), None);
        assert_eq!(parse_card("NMC1:AAAA"), None);
    }
}
