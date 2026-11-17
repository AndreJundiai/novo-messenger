//! Identidade e criptografia de ponta a ponta.
//!
//! Cada pessoa é uma chave X25519 gerada no aparelho; não há telefone nem
//! servidor. Uma mensagem é selada para um destinatário assim:
//!
//! ```text
//! envelope = [versão 1] [dica do destinatário 4] [dica do remetente 4] [chave efêmera 32] [cifrado + tag 16]
//! chave    = HKDF-SHA256(sal = cabeçalho, DH(efêmera, destinatário) ‖ DH(remetente, destinatário))
//! cifra    = ChaCha20-Poly1305, cabeçalho como dado autenticado
//! ```
//!
//! - O primeiro DH usa uma chave nova a cada mensagem, então a chave de cifra
//!   nunca se repete (o nonce pode ser fixo).
//! - O segundo DH prova quem enviou: só o dono da chave do remetente chega à
//!   mesma chave de cifra. Por isso só contatos adicionados conseguem falar
//!   com você.
//! - Quem repassa o envelope não lê nem altera nada (o tag falharia).
//! - Limite atual: se a chave longa do *destinatário* vazar, as mensagens
//!   antigas para ele podem ser lidas. Uma catraca de chaves (Double Ratchet)
//!   resolve isso numa fase seguinte.

use chacha20poly1305::aead::{Aead, KeyInit, Payload};
use chacha20poly1305::{ChaCha20Poly1305, Key, Nonce};
use hkdf::Hkdf;
use rand_core::CryptoRngCore;
use sha2::{Digest, Sha256};
use x25519_dalek::{PublicKey, StaticSecret};
use zeroize::Zeroize;

pub const VERSION: u8 = 1;
const HEADER: usize = 1 + 4 + 4 + 32;
/// Bytes que o envelope acrescenta ao conteúdo.
pub const OVERHEAD: usize = HEADER + 16;

pub type KeyBytes = [u8; 32];

/// 4 bytes que identificam uma chave sem revelá-la por inteiro.
pub fn hint(key: &KeyBytes) -> [u8; 4] {
    let h = Sha256::new().chain_update(b"nm-hint").chain_update(key).finalize();
    [h[0], h[1], h[2], h[3]]
}

pub struct Identity {
    secret: StaticSecret,
    public: KeyBytes,
}

impl Identity {
    pub fn generate(rng: &mut impl CryptoRngCore) -> Self {
        Self::from_secret(StaticSecret::random_from_rng(rng).to_bytes())
    }

    pub fn from_secret(mut bytes: KeyBytes) -> Self {
        let secret = StaticSecret::from(bytes);
        bytes.zeroize();
        let public = PublicKey::from(&secret).to_bytes();
        Self { secret, public }
    }

    pub fn public(&self) -> &KeyBytes {
        &self.public
    }

    pub fn secret_bytes(&self) -> KeyBytes {
        self.secret.to_bytes()
    }
}

fn derive(header: &[u8], dh1: &[u8; 32], dh2: &[u8; 32]) -> ChaCha20Poly1305 {
    let mut ikm = [0u8; 64];
    ikm[..32].copy_from_slice(dh1);
    ikm[32..].copy_from_slice(dh2);
    let mut key = [0u8; 32];
    Hkdf::<Sha256>::new(Some(header), &ikm).expand(b"nm-chat v1 key", &mut key).expect("32 bytes cabem no HKDF");
    let cipher = ChaCha20Poly1305::new(Key::from_slice(&key));
    ikm.zeroize();
    key.zeroize();
    cipher
}

/// Sela `plaintext` de `me` para `to`. `None` se a chave do destinatário for inválida.
pub fn seal(me: &Identity, to: &KeyBytes, plaintext: &[u8], rng: &mut impl CryptoRngCore) -> Option<Vec<u8>> {
    let eph = StaticSecret::random_from_rng(rng);
    let to_pk = PublicKey::from(*to);
    let dh1 = eph.diffie_hellman(&to_pk);
    let dh2 = me.secret.diffie_hellman(&to_pk);
    if !dh1.was_contributory() || !dh2.was_contributory() {
        return None;
    }
    let mut out = Vec::with_capacity(OVERHEAD + plaintext.len());
    out.push(VERSION);
    out.extend_from_slice(&hint(to));
    out.extend_from_slice(&hint(&me.public));
    out.extend_from_slice(PublicKey::from(&eph).as_bytes());
    let cipher = derive(&out, dh1.as_bytes(), dh2.as_bytes());
    let ct = cipher.encrypt(Nonce::from_slice(&[0u8; 12]), Payload { msg: plaintext, aad: &out }).ok()?;
    out.extend(ct);
    Some(out)
}

/// `true` se o envelope é para `me` (sem tentar abrir).
pub fn is_for(me: &Identity, envelope: &[u8]) -> bool {
    envelope.len() >= OVERHEAD && envelope[0] == VERSION && envelope[1..5] == hint(&me.public)
}

/// Abre um envelope para `me`, testando os remetentes possíveis (contatos).
/// Devolve a chave de quem enviou e o conteúdo.
pub fn open<'a>(me: &Identity, senders: impl IntoIterator<Item = &'a KeyBytes>, envelope: &[u8]) -> Option<(KeyBytes, Vec<u8>)> {
    if !is_for(me, envelope) {
        return None;
    }
    let (header, ct) = envelope.split_at(HEADER);
    let from_hint: [u8; 4] = header[5..9].try_into().ok()?;
    let eph: KeyBytes = header[9..41].try_into().ok()?;
    let dh1 = me.secret.diffie_hellman(&PublicKey::from(eph));
    if !dh1.was_contributory() {
        return None;
    }
    senders.into_iter().filter(|k| hint(k) == from_hint).find_map(|k| {
        let dh2 = me.secret.diffie_hellman(&PublicKey::from(*k));
        let cipher = derive(header, dh1.as_bytes(), dh2.as_bytes());
        let pt = cipher.decrypt(Nonce::from_slice(&[0u8; 12]), Payload { msg: ct, aad: header }).ok()?;
        Some((*k, pt))
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand_core::OsRng;

    #[test]
    fn roundtrip_and_sender_is_authenticated() {
        let (a, b, eve) = (Identity::generate(&mut OsRng), Identity::generate(&mut OsRng), Identity::generate(&mut OsRng));
        let env = seal(&a, b.public(), b"oi", &mut OsRng).unwrap();
        assert_eq!(env.len(), OVERHEAD + 2);
        assert_eq!(open(&b, [a.public()], &env), Some((*a.public(), b"oi".to_vec())));
        // Eve não lê (não é para ela) e não se passa por A.
        assert_eq!(open(&eve, [a.public()], &env), None);
        let forged = seal(&eve, b.public(), b"sou a A", &mut OsRng).unwrap();
        let mut as_a = forged.clone();
        as_a[5..9].copy_from_slice(&hint(a.public()));
        assert_eq!(open(&b, [a.public()], &as_a), None);
        // Sem A nos contatos, B não abre.
        assert_eq!(open(&b, [eve.public()], &env), None);
    }

    #[test]
    fn any_tampering_is_rejected() {
        let (a, b) = (Identity::generate(&mut OsRng), Identity::generate(&mut OsRng));
        let env = seal(&a, b.public(), b"mensagem", &mut OsRng).unwrap();
        for i in 0..env.len() {
            let mut t = env.clone();
            t[i] ^= 0x01;
            assert_eq!(open(&b, [a.public()], &t), None, "byte {i}");
        }
    }

    #[test]
    fn same_text_gives_different_envelopes() {
        let (a, b) = (Identity::generate(&mut OsRng), Identity::generate(&mut OsRng));
        assert_ne!(seal(&a, b.public(), b"x", &mut OsRng), seal(&a, b.public(), b"x", &mut OsRng));
    }

    #[test]
    fn low_order_key_is_refused() {
        let a = Identity::generate(&mut OsRng);
        assert_eq!(seal(&a, &[0u8; 32], b"x", &mut OsRng), None);
    }

    #[test]
    fn secret_roundtrip() {
        let a = Identity::generate(&mut OsRng);
        assert_eq!(Identity::from_secret(a.secret_bytes()).public(), a.public());
    }
}
