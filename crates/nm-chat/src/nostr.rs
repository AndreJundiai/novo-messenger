//! Transporte pela internet usando relays públicos do Nostr (NIP-01).
//!
//! Nostr é uma rede aberta de servidores ("relays") que guardam e repassam
//! eventos assinados. Não é preciso ter servidor próprio nem conta.
//!
//! - O conteúdo do evento é o envelope já cifrado de ponta a ponta
//!   ([`crate::crypto`]); o relay só vê bytes que não consegue ler.
//! - Cada evento é assinado com uma chave **descartável**, então os relays não
//!   conseguem ligar as mensagens de uma pessoa entre si.
//! - O destinatário é marcado por uma "rota": um resumo SHA-256 da chave dele,
//!   que não revela a chave. Ele pede ao relay só os eventos da própria rota.
//! - Usamos o tipo 4 (mensagem direta), que os relays aceitam e indexam pela
//!   tag `p`, e pedimos para apagar após 7 dias (NIP-40).

use crate::crypto::KeyBytes;
use k256::schnorr::SigningKey;
use nm_core::b64;
use rand_core::{CryptoRngCore, OsRng};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

pub const KIND: u64 = 4;
const TTL_SECONDS: u64 = 7 * 24 * 3600;
/// Prefixo do conteúdo, para reconhecer eventos do Novo Messenger.
const CONTENT_PREFIX: &str = "nm1:";

/// Relays públicos usados por padrão (podem ser trocados no app).
pub const DEFAULT_RELAYS: &[&str] = &["wss://relay.damus.io", "wss://nos.lol", "wss://relay.primal.net"];

fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

/// Rota de quem recebe: resumo da chave pública, em hexadecimal.
pub fn route(key: &KeyBytes) -> String {
    hex(&Sha256::new().chain_update(b"nm-route-v1").chain_update(key).finalize())
}

/// Mensagem `["EVENT", {...}]` pronta para mandar a um relay.
pub fn event(to: &KeyBytes, envelope: &[u8], now_s: u64) -> String {
    event_with(to, envelope, now_s, &mut OsRng)
}

pub fn event_with(to: &KeyBytes, envelope: &[u8], now_s: u64, rng: &mut impl CryptoRngCore) -> String {
    let sk = SigningKey::random(rng);
    let pubkey = hex(&sk.verifying_key().to_bytes());
    let tags = json!([["p", route(to)], ["expiration", (now_s + TTL_SECONDS).to_string()]]);
    let content = format!("{CONTENT_PREFIX}{}", b64::encode(envelope));
    // Identificador do evento = SHA-256 da serialização exata definida na NIP-01.
    let serial = json!([0, pubkey, now_s, KIND, tags, content]).to_string();
    let id: [u8; 32] = Sha256::digest(serial.as_bytes()).into();
    let mut aux = [0u8; 32];
    rng.fill_bytes(&mut aux);
    let sig = sk.sign_raw(&id, &aux).expect("assinatura Schnorr de 32 bytes");
    json!(["EVENT", {
        "id": hex(&id),
        "pubkey": pubkey,
        "created_at": now_s,
        "kind": KIND,
        "tags": tags,
        "content": content,
        "sig": hex(&sig.to_bytes()),
    }])
    .to_string()
}

/// Pedido `["REQ", ...]` dos eventos para `me` desde `since_s`.
pub fn request(subscription: &str, me: &KeyBytes, since_s: u64) -> String {
    json!(["REQ", subscription, {"kinds": [KIND], "#p": [route(me)], "since": since_s}]).to_string()
}

/// O que um relay respondeu.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RelayMessage {
    /// Um envelope do Novo Messenger e a hora (segundos) do evento.
    Envelope { data: Vec<u8>, created_at: u64 },
    /// O relay terminou de mandar os eventos guardados.
    EndOfStored,
    /// Resposta a um evento enviado.
    Accepted { ok: bool, reason: String },
    Notice(String),
    /// Qualquer outra coisa (inclusive eventos que não são nossos).
    Other,
}

pub fn parse(text: &str) -> RelayMessage {
    let Ok(Value::Array(v)) = serde_json::from_str::<Value>(text) else { return RelayMessage::Other };
    match v.first().and_then(Value::as_str) {
        Some("EVENT") => {
            let Some(ev) = v.get(2) else { return RelayMessage::Other };
            let content = ev.get("content").and_then(Value::as_str).unwrap_or("");
            let created_at = ev.get("created_at").and_then(Value::as_u64).unwrap_or(0);
            match content.strip_prefix(CONTENT_PREFIX).and_then(b64::decode) {
                Some(data) if ev.get("kind").and_then(Value::as_u64) == Some(KIND) => RelayMessage::Envelope { data, created_at },
                _ => RelayMessage::Other,
            }
        }
        Some("EOSE") => RelayMessage::EndOfStored,
        Some("OK") => RelayMessage::Accepted {
            ok: v.get(2).and_then(Value::as_bool).unwrap_or(false),
            reason: v.get(3).and_then(Value::as_str).unwrap_or("").to_string(),
        },
        Some("NOTICE") => RelayMessage::Notice(v.get(1).and_then(Value::as_str).unwrap_or("").to_string()),
        _ => RelayMessage::Other,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use k256::schnorr::{Signature, VerifyingKey};

    fn unhex(s: &str) -> Vec<u8> {
        (0..s.len()).step_by(2).map(|i| u8::from_str_radix(&s[i..i + 2], 16).unwrap()).collect()
    }

    #[test]
    fn event_is_valid_nip01() {
        let to = [7u8; 32];
        let env = b"envelope cifrado".to_vec();
        let msg: Value = serde_json::from_str(&event(&to, &env, 1_800_000_000)).unwrap();
        assert_eq!(msg[0], "EVENT");
        let ev = &msg[1];
        // id confere com a serialização da NIP-01
        let serial = json!([0, ev["pubkey"], ev["created_at"], ev["kind"], ev["tags"], ev["content"]]).to_string();
        assert_eq!(ev["id"].as_str().unwrap(), hex(&Sha256::digest(serial.as_bytes())));
        // assinatura Schnorr (BIP-340) válida para a chave do evento
        let vk = VerifyingKey::from_bytes(&unhex(ev["pubkey"].as_str().unwrap())).unwrap();
        let sig = Signature::try_from(unhex(ev["sig"].as_str().unwrap()).as_slice()).unwrap();
        vk.verify_raw(&unhex(ev["id"].as_str().unwrap()), &sig).unwrap();
        assert_eq!(ev["tags"][0][1].as_str().unwrap(), route(&to));
        assert_eq!(ev["tags"][1][1].as_str().unwrap(), (1_800_000_000u64 + TTL_SECONDS).to_string());
        // e o relay devolvendo o mesmo evento vira o envelope de novo
        let relayed = json!(["EVENT", "sub1", ev]).to_string();
        assert_eq!(parse(&relayed), RelayMessage::Envelope { data: env, created_at: 1_800_000_000 });
    }

    #[test]
    fn each_event_uses_a_fresh_key() {
        let a: Value = serde_json::from_str(&event(&[1; 32], b"x", 1)).unwrap();
        let b: Value = serde_json::from_str(&event(&[1; 32], b"x", 1)).unwrap();
        assert_ne!(a[1]["pubkey"], b[1]["pubkey"]);
    }

    #[test]
    fn request_and_other_relay_messages() {
        let r: Value = serde_json::from_str(&request("nm", &[2; 32], 100)).unwrap();
        assert_eq!(r[0], "REQ");
        assert_eq!(r[2]["#p"][0].as_str().unwrap(), route(&[2; 32]));
        assert_eq!(r[2]["since"], 100);
        assert_eq!(parse(r#"["EOSE","nm"]"#), RelayMessage::EndOfStored);
        assert_eq!(parse(r#"["OK","abc",false,"blocked: spam"]"#), RelayMessage::Accepted { ok: false, reason: "blocked: spam".into() });
        assert_eq!(parse(r#"["NOTICE","oi"]"#), RelayMessage::Notice("oi".into()));
        assert_eq!(parse(r#"["EVENT","s",{"kind":1,"content":"nota comum"}]"#), RelayMessage::Other);
        assert_eq!(parse("não é json"), RelayMessage::Other);
        assert_ne!(route(&[1; 32]), route(&[2; 32]));
    }
}
