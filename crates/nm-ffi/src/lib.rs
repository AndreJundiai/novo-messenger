//! Ligação do núcleo Rust com os apps nativos (Kotlin no Android, Swift no
//! iOS), gerada pelo UniFFI. Tudo que o app precisa passa por [`Messenger`].
//!
//! Contatos e conversas são identificados por `peer_id`: a chave pública em
//! 64 dígitos hexadecimais.

use nm_chat::nostr::{self, RelayMessage};
use nm_chat::{backup, key_id, parse_card, parse_key_id, phone, safety_code, Chat, ChatError, ChatMessage, Contact, Conversation, Status};
use std::sync::{Arc, Mutex, MutexGuard};

uniffi::setup_scaffolding!();

/// Erros chegam ao Kotlin/Swift com a mensagem em português (`message`).
#[derive(Debug, thiserror::Error, uniffi::Error)]
#[uniffi(flat_error)]
pub enum NmError {
    #[error("{0}")]
    Chat(String),
    #[error("identificador de contato inválido")]
    InvalidPeer,
}

impl From<ChatError> for NmError {
    fn from(e: ChatError) -> Self {
        NmError::Chat(e.describe().to_string())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum MessageStatus {
    /// Salva aqui, ainda não saiu (relógio).
    Pending,
    /// Saiu do aparelho (✓).
    Sent,
    /// O destinatário recebeu (✓✓).
    Delivered,
    /// O destinatário leu (✓✓ azul).
    Read,
}

impl From<Status> for MessageStatus {
    fn from(s: Status) -> Self {
        match s {
            Status::Pending => MessageStatus::Pending,
            Status::Sent => MessageStatus::Sent,
            Status::Delivered => MessageStatus::Delivered,
            Status::Read => MessageStatus::Read,
        }
    }
}

#[derive(Clone, Debug, uniffi::Record)]
pub struct ContactInfo {
    pub peer_id: String,
    pub name: String,
    /// Código para conferir pessoalmente que é a pessoa certa.
    pub safety_code: String,
    /// Adicionado lendo o QR ao vivo.
    pub added_in_person: bool,
    /// Número ligado ao contato (formato internacional, só dígitos).
    pub phone: Option<String>,
    /// O dono do número confirmou pelo código do SMS.
    pub phone_verified: bool,
}

#[derive(Clone, Debug, uniffi::Record)]
pub struct MessageInfo {
    pub id: u64,
    pub peer_id: String,
    pub outgoing: bool,
    pub text: String,
    pub at_ms: u64,
    pub status: MessageStatus,
    pub read: bool,
    pub reply_to: Option<u64>,
    pub edited: bool,
    pub deleted: bool,
    pub my_reaction: Option<String>,
    pub peer_reaction: Option<String>,
    /// Mostrar o alerta "pedido de dinheiro".
    pub money_alert: bool,
}

#[derive(Clone, Debug, uniffi::Record)]
pub struct ConversationInfo {
    pub peer_id: String,
    pub name: String,
    pub last_text: String,
    pub last_at_ms: u64,
    pub last_outgoing: bool,
    pub last_status: MessageStatus,
    pub last_deleted: bool,
    pub unread: u32,
}

#[derive(Clone, Debug, uniffi::Record)]
pub struct OutgoingPacket {
    pub message_id: Option<u64>,
    pub to_peer_id: String,
    pub data: Vec<u8>,
}

#[derive(Clone, Debug, uniffi::Record)]
pub struct ReceiveResult {
    pub message: Option<MessageInfo>,
    pub updated: Vec<u64>,
    pub changed: Vec<u64>,
    pub not_for_me: bool,
    /// `peer_id` do contato que acabou de confirmar o número.
    pub contact_verified: Option<String>,
}

/// QR Code: `size` × `size` módulos, `modules[y * size + x]` = 1 escuro.
#[derive(Clone, Debug, uniffi::Record)]
pub struct QrImage {
    pub size: u32,
    pub modules: Vec<u8>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum RelayKind {
    /// Chegou um envelope (`data`).
    Envelope,
    /// O relay terminou de mandar o que estava guardado.
    EndOfStored,
    /// Resposta a um envio (`ok`, `info`).
    Accepted,
    /// Entrada do diretório: `phone_hash` e o cartão em `info` (vazio = saiu do diretório).
    Directory,
    Notice,
    Other,
}

#[derive(Clone, Debug, uniffi::Record)]
pub struct RelayEvent {
    pub kind: RelayKind,
    pub data: Option<Vec<u8>>,
    pub created_at: u64,
    pub ok: bool,
    pub info: String,
    pub phone_hash: String,
}

fn contact_info(c: &Contact) -> ContactInfo {
    ContactInfo {
        peer_id: key_id(&c.key),
        name: c.name.clone(),
        safety_code: safety_code(&c.key),
        added_in_person: c.added_in_person,
        phone: c.phone.clone(),
        phone_verified: c.phone_verified,
    }
}

fn message_info(chat: &Chat, m: &ChatMessage) -> MessageInfo {
    MessageInfo {
        id: m.id,
        peer_id: key_id(&m.peer),
        outgoing: m.outgoing,
        text: m.text.clone(),
        at_ms: m.at_ms,
        status: m.status.into(),
        read: m.read,
        reply_to: m.reply_to,
        edited: m.edited,
        deleted: m.deleted,
        my_reaction: m.my_reaction.clone(),
        peer_reaction: m.peer_reaction.clone(),
        money_alert: chat.money_alert(m),
    }
}

fn conversation_info(c: &Conversation) -> ConversationInfo {
    ConversationInfo {
        peer_id: key_id(&c.peer),
        name: c.name.clone(),
        last_text: c.last_text.clone(),
        last_at_ms: c.last_at_ms,
        last_outgoing: c.last_outgoing,
        last_status: c.last_status.into(),
        last_deleted: c.last_deleted,
        unread: c.unread,
    }
}

fn peer(id: &str) -> Result<[u8; 32], NmError> {
    parse_key_id(id).ok_or(NmError::InvalidPeer)
}

/// Gera o QR Code de um texto (ex.: o cartão de contato).
#[uniffi::export]
pub fn qr_image(text: String) -> Option<QrImage> {
    let q = nm_core::qr::encode(text.as_bytes())?;
    let mut modules = Vec::with_capacity(q.size * q.size);
    for y in 0..q.size {
        for x in 0..q.size {
            modules.push(q.get(x, y) as u8);
        }
    }
    Some(QrImage { size: q.size as u32, modules })
}

/// Nome que vem dentro de um código de contato (para avisar de nome parecido antes de adicionar).
#[uniffi::export]
pub fn card_name(card: String) -> Option<String> {
    parse_card(&card).map(|c| c.name)
}

/// Relays públicos usados por padrão.
#[uniffi::export]
pub fn default_relays() -> Vec<String> {
    nostr::DEFAULT_RELAYS.iter().map(|s| s.to_string()).collect()
}

/// Número no formato internacional só com dígitos; sem código de país, assume o Brasil.
#[uniffi::export]
pub fn normalize_phone(raw: String) -> Option<String> {
    phone::normalize(&raw, "55")
}

/// Resumo do número para o diretório (lento de propósito: chame fora da tela principal).
#[uniffi::export]
pub fn phone_hash(e164: String) -> String {
    phone::hash(&e164)
}

/// Pedido aos relays das entradas do diretório para estes resumos.
#[uniffi::export]
pub fn directory_request(subscription: String, phone_hashes: Vec<String>) -> String {
    nostr::directory_request(&subscription, &phone_hashes)
}

/// Interpreta uma mensagem de texto vinda de um relay.
#[uniffi::export]
pub fn parse_relay_message(text: String) -> RelayEvent {
    let blank = RelayEvent { kind: RelayKind::Other, data: None, created_at: 0, ok: false, info: String::new(), phone_hash: String::new() };
    match nostr::parse(&text) {
        RelayMessage::Envelope { data, created_at } => RelayEvent { kind: RelayKind::Envelope, data: Some(data), created_at, ..blank },
        RelayMessage::Directory { phone_hash, card, created_at } => RelayEvent { kind: RelayKind::Directory, info: card, phone_hash, created_at, ..blank },
        RelayMessage::EndOfStored => RelayEvent { kind: RelayKind::EndOfStored, ..blank },
        RelayMessage::Accepted { ok, reason } => RelayEvent { kind: RelayKind::Accepted, ok, info: reason, ..blank },
        RelayMessage::Notice(n) => RelayEvent { kind: RelayKind::Notice, info: n, ..blank },
        RelayMessage::Other => blank,
    }
}

#[derive(uniffi::Object)]
pub struct Messenger {
    chat: Mutex<Chat>,
}

impl Messenger {
    fn chat(&self) -> MutexGuard<'_, Chat> {
        // Um pânico em outra chamada não deve travar o app para sempre.
        self.chat.lock().unwrap_or_else(|e| e.into_inner())
    }
}

#[uniffi::export]
impl Messenger {
    /// Cria uma identidade nova neste aparelho.
    #[uniffi::constructor]
    pub fn create(name: String) -> Arc<Self> {
        Arc::new(Self { chat: Mutex::new(Chat::new(&name)) })
    }

    /// Restaura de [`Messenger::save`].
    #[uniffi::constructor]
    pub fn restore(data: Vec<u8>) -> Result<Arc<Self>, NmError> {
        Ok(Arc::new(Self { chat: Mutex::new(Chat::load(&data)?) }))
    }

    /// Restaura de um backup cifrado com senha.
    #[uniffi::constructor]
    pub fn restore_backup(data: Vec<u8>, password: String) -> Result<Arc<Self>, NmError> {
        Ok(Arc::new(Self { chat: Mutex::new(backup::restore(&data, &password)?) }))
    }

    /// Estado completo (inclui a chave secreta): guarde só no armazenamento privado do app.
    pub fn save(&self) -> Vec<u8> {
        self.chat().save()
    }

    /// Backup cifrado com senha, para guardar fora do app.
    pub fn export_backup(&self, password: String) -> Result<Vec<u8>, NmError> {
        Ok(backup::export(&self.chat(), &password)?)
    }

    pub fn my_name(&self) -> String {
        self.chat().my_name().to_string()
    }

    pub fn set_my_name(&self, name: String) {
        self.chat().set_my_name(&name);
    }

    pub fn my_id(&self) -> String {
        key_id(self.chat().my_key())
    }

    pub fn my_safety_code(&self) -> String {
        safety_code(self.chat().my_key())
    }

    /// Texto do QR "Meu código".
    pub fn my_card(&self) -> String {
        self.chat().my_card()
    }

    // ---------- contatos e escudo ----------

    pub fn add_contact(&self, card: String) -> Result<ContactInfo, NmError> {
        Ok(contact_info(&self.chat().add_contact(&card)?))
    }

    /// Contatos com nome igual ou parecido ao do código, antes de adicioná-lo.
    pub fn similar_contacts(&self, card: String) -> Vec<ContactInfo> {
        let Some(c) = parse_card(&card) else { return Vec::new() };
        self.chat().similar_contacts(&c.name, Some(&c.key)).iter().map(contact_info).collect()
    }

    pub fn rename_contact(&self, peer_id: String, name: String) -> Result<(), NmError> {
        Ok(self.chat().rename_contact(&peer(&peer_id)?, &name)?)
    }

    pub fn contacts(&self) -> Vec<ContactInfo> {
        self.chat().contacts().iter().map(contact_info).collect()
    }

    pub fn contact(&self, peer_id: String) -> Option<ContactInfo> {
        let k = parse_key_id(&peer_id)?;
        self.chat().contact(&k).map(contact_info)
    }

    pub fn money_alerts(&self) -> bool {
        self.chat().money_alerts()
    }

    pub fn set_money_alerts(&self, on: bool) {
        self.chat().set_money_alerts(on);
    }

    pub fn dismiss_alert(&self, message_id: u64) -> bool {
        self.chat().dismiss_alert(message_id)
    }

    // ---------- conversas ----------

    pub fn conversations(&self) -> Vec<ConversationInfo> {
        self.chat().conversations().iter().map(conversation_info).collect()
    }

    pub fn messages(&self, peer_id: String) -> Result<Vec<MessageInfo>, NmError> {
        let chat = self.chat();
        Ok(chat.messages(&peer(&peer_id)?).iter().map(|m| message_info(&chat, m)).collect())
    }

    pub fn message(&self, message_id: u64) -> Option<MessageInfo> {
        let chat = self.chat();
        chat.message(message_id).map(|m| message_info(&chat, m))
    }

    pub fn search(&self, query: String) -> Vec<MessageInfo> {
        let chat = self.chat();
        chat.search(&query).iter().map(|m| message_info(&chat, m)).collect()
    }

    pub fn send_text(&self, peer_id: String, text: String, now_ms: u64) -> Result<MessageInfo, NmError> {
        let mut chat = self.chat();
        let m = chat.send_text(&peer(&peer_id)?, &text, now_ms)?;
        Ok(message_info(&chat, &m))
    }

    pub fn send_reply(&self, peer_id: String, text: String, reply_to: u64, now_ms: u64) -> Result<MessageInfo, NmError> {
        let mut chat = self.chat();
        let m = chat.send_reply(&peer(&peer_id)?, &text, reply_to, now_ms)?;
        Ok(message_info(&chat, &m))
    }

    pub fn edit_message(&self, message_id: u64, text: String, now_ms: u64) -> Result<(), NmError> {
        Ok(self.chat().edit_message(message_id, &text, now_ms)?)
    }

    pub fn delete_for_everyone(&self, message_id: u64, now_ms: u64) -> Result<(), NmError> {
        Ok(self.chat().delete_for_everyone(message_id, now_ms)?)
    }

    pub fn delete_for_me(&self, message_id: u64) -> bool {
        self.chat().delete_for_me(message_id)
    }

    /// Emoji vazio tira a reação.
    pub fn react(&self, message_id: u64, emoji: String, now_ms: u64) -> Result<(), NmError> {
        Ok(self.chat().react(message_id, &emoji, now_ms)?)
    }

    /// Marca a conversa como lida e prepara as confirmações (✓✓ azul do outro lado).
    pub fn mark_read(&self, peer_id: String) -> Result<u32, NmError> {
        Ok(self.chat().mark_read(&peer(&peer_id)?) as u32)
    }

    // ---------- transporte ----------

    pub fn has_outgoing(&self) -> bool {
        self.chat().has_outgoing()
    }

    /// O que mandar agora (por todos os transportes disponíveis).
    pub fn outbox(&self) -> Vec<OutgoingPacket> {
        self.chat().outbox().into_iter().map(|o| OutgoingPacket { message_id: o.message_id, to_peer_id: key_id(&o.to), data: o.data }).collect()
    }

    /// A mensagem saiu por algum transporte (✓). Devolve `true` se mudou.
    pub fn mark_sent(&self, message_id: u64) -> bool {
        self.chat().mark_sent(message_id)
    }

    pub fn receive(&self, data: Vec<u8>, now_ms: u64) -> ReceiveResult {
        let mut chat = self.chat();
        let r = chat.receive(&data, now_ms);
        ReceiveResult {
            message: r.message.as_ref().map(|m| message_info(&chat, m)),
            updated: r.updated,
            changed: r.changed,
            not_for_me: r.not_for_me,
            contact_verified: r.contact_verified.as_ref().map(key_id),
        }
    }

    /// Evento Nostr `["EVENT", …]` levando `data` para `to_peer_id` pela internet.
    pub fn relay_event(&self, to_peer_id: String, data: Vec<u8>, now_s: u64) -> Result<String, NmError> {
        Ok(nostr::event(&peer(&to_peer_id)?, &data, now_s))
    }

    /// Contato encontrado pelo número no diretório (fica "não confirmado").
    pub fn add_found_contact(&self, card: String, phone: String) -> Result<ContactInfo, NmError> {
        Ok(contact_info(&self.chat().add_found_contact(&card, &phone)?))
    }

    /// Texto do código de confirmação para mandar por SMS ao número do contato.
    pub fn verification_code(&self, peer_id: String) -> Result<String, NmError> {
        Ok(self.chat().verification_code(&peer(&peer_id)?)?)
    }

    /// Código colado pela pessoa (contato recebido por outro meio ou confirmação de SMS).
    pub fn accept_code(&self, text: String, now_ms: u64) -> Result<ContactInfo, NmError> {
        Ok(contact_info(&self.chat().accept_code(&text, now_ms)?))
    }

    /// Publica (ou, com `listed` falso, apaga) minha entrada no diretório para o resumo do meu número.
    pub fn directory_event(&self, phone_hash: String, listed: bool, now_s: u64) -> String {
        let chat = self.chat();
        let card = if listed { chat.my_card() } else { String::new() };
        nostr::directory_event(&chat.identity_secret(), &phone_hash, &card, now_s)
    }

    /// Pedido `["REQ", …]` das mensagens para este aparelho desde `since_s`.
    pub fn relay_request(&self, subscription: String, since_s: u64) -> String {
        nostr::request(&subscription, self.chat().my_key(), since_s)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn two_messengers_talk_through_the_ffi_surface() {
        let a = Messenger::create("Ana".into());
        let b = Messenger::create("Bruno".into());
        let bc = a.add_contact(b.my_card()).unwrap();
        b.add_contact(a.my_card()).unwrap();
        assert!(bc.added_in_person);
        let sent = a.send_text(bc.peer_id.clone(), "me manda um pix".into(), 1).unwrap();
        assert_eq!(sent.status, MessageStatus::Pending);
        assert!(!sent.money_alert, "não alerta em mensagem enviada");
        for p in a.outbox() {
            assert_eq!(p.to_peer_id, b.my_id());
            if let Some(id) = p.message_id {
                a.mark_sent(id);
            }
            let r = b.receive(p.data, 2);
            assert!(r.message.unwrap().money_alert);
        }
        for p in b.outbox() {
            assert_eq!(a.receive(p.data, 3).updated, vec![sent.id]);
        }
        assert_eq!(a.messages(bc.peer_id.clone()).unwrap()[0].status, MessageStatus::Delivered);
        let a2 = Messenger::restore(a.save()).unwrap();
        assert_eq!(a2.conversations()[0].name, "Bruno");
        assert!(matches!(a.messages("xyz".into()), Err(NmError::InvalidPeer)));
        assert!(Messenger::restore(vec![1, 2, 3]).is_err());
        let q = qr_image(a.my_card()).unwrap();
        assert_eq!(q.modules.len() as u32, q.size * q.size);
        assert_eq!(card_name(b.my_card()).as_deref(), Some("Bruno"));
        let impostor = Messenger::create("bruno".into());
        assert_eq!(a.similar_contacts(impostor.my_card()).len(), 1);
    }

    #[test]
    fn phone_directory_and_sms_confirmation() {
        let a = Messenger::create("Ana".into());
        let b = Messenger::create("Bruno".into());
        let num = normalize_phone("(11) 98765-4321".into()).unwrap();
        assert_eq!(num, "5511987654321");
        let h = phone_hash(num.clone());
        // B publica no diretório; o relay devolve para A, que acha o cartão de B.
        let ev: serde_json::Value = serde_json::from_str(&b.directory_event(h.clone(), true, 10)).unwrap();
        let found = parse_relay_message(serde_json::json!(["EVENT", "dir", ev[1]]).to_string());
        assert_eq!(found.kind, RelayKind::Directory);
        assert_eq!(found.phone_hash, h);
        let c = a.add_found_contact(found.info, num).unwrap();
        assert!(!c.phone_verified);
        let sms = a.verification_code(c.peer_id.clone()).unwrap();
        b.accept_code(sms, 11).unwrap();
        let mut ok = None;
        for p in b.outbox() {
            ok = ok.or(a.receive(p.data, 12).contact_verified);
        }
        assert_eq!(ok, Some(b.my_id()));
        assert!(a.contact(b.my_id()).unwrap().phone_verified);
        assert!(directory_request("dir".into(), vec![h]).contains("nm-dir:"));
    }

    #[test]
    fn internet_path_and_backup() {
        let a = Messenger::create("Ana".into());
        let b = Messenger::create("Bruno".into());
        a.add_contact(b.my_card()).unwrap();
        b.add_contact(a.my_card()).unwrap();
        a.send_text(b.my_id(), "pela internet".into(), 1).unwrap();
        // A manda pelo relay; o relay devolve o evento para B, que o recebe.
        let p = a.outbox().remove(0);
        let ev = a.relay_event(p.to_peer_id, p.data, 1_800_000_000).unwrap();
        let sent: serde_json::Value = serde_json::from_str(&ev).unwrap();
        let relayed = serde_json::json!(["EVENT", "nm", sent[1]]).to_string();
        let e = parse_relay_message(relayed);
        assert_eq!(e.kind, RelayKind::Envelope);
        let r = b.receive(e.data.unwrap(), 2);
        assert_eq!(r.message.unwrap().text, "pela internet");
        assert!(b.relay_request("nm".into(), 0).contains("\"#p\""));
        assert!(!default_relays().is_empty());

        let file = a.export_backup("senha123".into()).unwrap();
        assert!(matches!(Messenger::restore_backup(file.clone(), "errada".into()), Err(NmError::Chat(m)) if m == "senha incorreta"));
        assert_eq!(Messenger::restore_backup(file, "senha123".into()).unwrap().my_id(), a.my_id());
    }
}
