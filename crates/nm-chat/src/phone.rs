//! Número de telefone: formato padrão e "resumo" para o diretório opcional.
//!
//! O Novo Messenger não precisa de número. Quem quiser ser encontrado pela
//! agenda dos outros publica só o resumo do número ([`hash`]), nunca o número.
//!
//! Limite honesto: números de telefone são poucos (cerca de 10¹¹ no mundo),
//! então quem tiver muito tempo e computador consegue testar todos. O resumo
//! usa Argon2id justamente para deixar isso lento e caro, mas não impossível.
//! Por isso o diretório é opcional e vem desligado.

use argon2::{Algorithm, Argon2, Params, Version};

const SALT: &[u8] = b"novo-messenger-diretorio-v1";

/// Número no formato internacional só com dígitos (ex.: `5511987654321`).
/// Números sem código de país são tratados como do Brasil (55).
pub fn normalize(raw: &str, default_country: &str) -> Option<String> {
    let plus = raw.trim_start().starts_with('+');
    let mut d: String = raw.chars().filter(|c| c.is_ascii_digit()).collect();
    if plus {
        // já vem com código do país
    } else if let Some(rest) = d.strip_prefix("00") {
        d = rest.to_string();
    } else {
        // Brasil: 0 + operadora (2 dígitos) + DDD + número, ex. 0 15 11 98765-4321
        if (d.len() == 13 || d.len() == 14) && d.starts_with('0') {
            d = d[3..].to_string();
        } else if d.starts_with('0') && (d.len() == 11 || d.len() == 12) {
            d = d[1..].to_string();
        }
        if d.len() == 10 || d.len() == 11 {
            d = format!("{default_country}{d}");
        }
    }
    (10..=15).contains(&d.len()).then_some(d)
}

/// Resumo do número (hexadecimal) usado no diretório. Lento de propósito.
pub fn hash(e164: &str) -> String {
    let params = Params::new(8 * 1024, 2, 1, Some(32)).expect("parâmetros válidos");
    let mut out = [0u8; 32];
    Argon2::new(Algorithm::Argon2id, Version::V0x13, params)
        .hash_password_into(e164.as_bytes(), SALT, &mut out)
        .expect("resumo de 32 bytes");
    out.iter().map(|b| format!("{b:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_brazilian_formats() {
        for raw in ["(11) 98765-4321", "11987654321", "+55 11 98765-4321", "0055 11 98765 4321", "011 98765-4321", "0 15 11 98765-4321", "015 11 98765-4321"] {
            assert_eq!(normalize(raw, "55").as_deref(), Some("5511987654321"), "{raw}");
        }
        assert_eq!(normalize("(11) 3333-4444", "55").as_deref(), Some("551133334444"));
        assert_eq!(normalize("0 21 11 3333-4444", "55").as_deref(), Some("551133334444"), "fixo com operadora");
        assert_eq!(normalize("+1 415 555 0100", "55").as_deref(), Some("14155550100"));
        assert_eq!(normalize("190", "55"), None, "número curto (emergência) fica de fora");
        assert_eq!(normalize("abc", "55"), None);
    }

    #[test]
    fn hash_is_stable_and_distinct() {
        let a = hash("5511987654321");
        assert_eq!(a.len(), 64);
        assert_eq!(a, hash("5511987654321"));
        assert_ne!(a, hash("5511987654322"));
    }
}
