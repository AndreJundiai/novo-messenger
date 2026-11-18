//! Backup cifrado com senha, sem nuvem (docs/PESQUISA.md, seção 7.2).
//!
//! `NMBK0001 ‖ sal 16 ‖ nonce 12 ‖ ChaCha20-Poly1305(chave, estado completo)`
//! com `chave = Argon2id(senha, sal)`. Sem a senha o arquivo não abre, e não
//! existe recuperação de senha.

use crate::chat::{Chat, ChatError};
use argon2::Argon2;
use chacha20poly1305::aead::{Aead, KeyInit, Payload};
use chacha20poly1305::{ChaCha20Poly1305, Key, Nonce};
use rand_core::{CryptoRngCore, OsRng};
use zeroize::Zeroize;

const MAGIC: &[u8; 8] = b"NMBK0001";
pub const MIN_PASSWORD_CHARS: usize = 6;

fn key(password: &str, salt: &[u8]) -> Result<ChaCha20Poly1305, ChatError> {
    let mut k = [0u8; 32];
    Argon2::default().hash_password_into(password.as_bytes(), salt, &mut k).map_err(|_| ChatError::InvalidData)?;
    let cipher = ChaCha20Poly1305::new(Key::from_slice(&k));
    k.zeroize();
    Ok(cipher)
}

pub fn export(chat: &Chat, password: &str) -> Result<Vec<u8>, ChatError> {
    export_with(chat, password, &mut OsRng)
}

pub fn export_with(chat: &Chat, password: &str, rng: &mut impl CryptoRngCore) -> Result<Vec<u8>, ChatError> {
    if password.chars().count() < MIN_PASSWORD_CHARS {
        return Err(ChatError::WeakPassword);
    }
    let mut salt = [0u8; 16];
    let mut nonce = [0u8; 12];
    rng.fill_bytes(&mut salt);
    rng.fill_bytes(&mut nonce);
    let mut out = MAGIC.to_vec();
    out.extend_from_slice(&salt);
    out.extend_from_slice(&nonce);
    let mut plain = chat.save();
    let ct = key(password, &salt)?.encrypt(Nonce::from_slice(&nonce), Payload { msg: &plain, aad: &out });
    plain.zeroize();
    out.extend(ct.map_err(|_| ChatError::InvalidData)?);
    Ok(out)
}

pub fn restore(data: &[u8], password: &str) -> Result<Chat, ChatError> {
    if data.len() < 8 + 16 + 12 + 16 || &data[..8] != MAGIC {
        return Err(ChatError::InvalidData);
    }
    let (header, ct) = data.split_at(8 + 16 + 12);
    let mut plain = key(password, &header[8..24])?
        .decrypt(Nonce::from_slice(&header[24..36]), Payload { msg: ct, aad: header })
        .map_err(|_| ChatError::WrongPassword)?;
    let chat = Chat::load(&plain);
    plain.zeroize();
    chat
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_wrong_password_and_tampering() {
        let mut a = Chat::new("Ana");
        let b = Chat::new("Bruno");
        a.add_contact(&b.my_card()).unwrap();
        a.send_text(b.my_key(), "guardar isto", 1).unwrap();
        let file = export(&a, "senha forte 123").unwrap();
        let back = restore(&file, "senha forte 123").unwrap();
        assert_eq!(back.my_key(), a.my_key());
        assert_eq!(back.messages(b.my_key()), a.messages(b.my_key()));
        assert_eq!(restore(&file, "senha errada").err(), Some(ChatError::WrongPassword));
        let mut bad = file.clone();
        let last = bad.len() - 1;
        bad[last] ^= 1;
        assert_eq!(restore(&bad, "senha forte 123").err(), Some(ChatError::WrongPassword));
        assert_eq!(restore(b"qualquer", "x").err(), Some(ChatError::InvalidData));
        assert_eq!(export(&a, "12345").err(), Some(ChatError::WeakPassword));
        assert!(!file.windows(12).any(|w| w == b"guardar isto"), "o texto não pode aparecer no arquivo");
    }
}
