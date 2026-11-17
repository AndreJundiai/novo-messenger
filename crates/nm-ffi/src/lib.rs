//! Ligação do núcleo Rust com os apps nativos (Kotlin no Android, Swift no
//! iOS), gerada pelo UniFFI. Tudo que o app precisa passa por [`Messenger`].
//!
//! Contatos e conversas são identificados por `peer_id`: a chave pública em
//! 64 dígitos hexadecimais.

use nm_chat::{key_id, parse_key_id, safety_code, Chat, ChatError, ChatMessage, Contact, Conversation, Status};
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
}

#[derive(Clone, Debug, uniffi::Record)]
pub struct ConversationInfo {
    pub peer_id: String,
    pub name: String,
    pub last_text: String,
    pub last_at_ms: u64,
    pub last_outgoing: bool,
    pub last_status: MessageStatus,
    pub unread: u32,
}

#[derive(Clone, Debug, uniffi::Record)]
pub struct OutgoingPacket {
    pub message_id: Option<u64>,
    pub data: Vec<u8>,
}

#[derive(Clone, Debug, uniffi::Record)]
pub struct ReceiveResult {
    pub message: Option<MessageInfo>,
    pub updated: Vec<u64>,
    pub not_for_me: bool,
}

/// QR Code: `size` × `size` módulos, `modules[y * size + x]` = 1 escuro.
#[derive(Clone, Debug, uniffi::Record)]
pub struct QrImage {
    pub size: u32,
    pub modules: Vec<u8>,
}

fn contact_info(c: &Contact) -> ContactInfo {
    ContactInfo { peer_id: key_id(&c.key), name: c.name.clone(), safety_code: safety_code(&c.key) }
}

fn message_info(m: &ChatMessage) -> MessageInfo {
    MessageInfo { id: m.id, peer_id: key_id(&m.peer), outgoing: m.outgoing, text: m.text.clone(), at_ms: m.at_ms, status: m.status.into(), read: m.read }
}

fn conversation_info(c: &Conversation) -> ConversationInfo {
    ConversationInfo {
        peer_id: key_id(&c.peer),
        name: c.name.clone(),
        last_text: c.last_text.clone(),
        last_at_ms: c.last_at_ms,
        last_outgoing: c.last_outgoing,
        last_status: c.last_status.into(),
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

    /// Estado completo (inclui a chave secreta): guarde só no armazenamento privado do app.
    pub fn save(&self) -> Vec<u8> {
        self.chat().save()
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

    pub fn add_contact(&self, card: String) -> Result<ContactInfo, NmError> {
        Ok(contact_info(&self.chat().add_contact(&card)?))
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

    pub fn conversations(&self) -> Vec<ConversationInfo> {
        self.chat().conversations().iter().map(conversation_info).collect()
    }

    pub fn messages(&self, peer_id: String) -> Result<Vec<MessageInfo>, NmError> {
        Ok(self.chat().messages(&peer(&peer_id)?).iter().map(message_info).collect())
    }

    pub fn send_text(&self, peer_id: String, text: String, now_ms: u64) -> Result<MessageInfo, NmError> {
        Ok(message_info(&self.chat().send_text(&peer(&peer_id)?, &text, now_ms)?))
    }

    /// O que mandar ao conectar com outro aparelho.
    pub fn outbox(&self) -> Vec<OutgoingPacket> {
        self.chat().outbox().into_iter().map(|o| OutgoingPacket { message_id: o.message_id, data: o.data }).collect()
    }

    /// A mensagem saiu por algum transporte (✓). Devolve `true` se mudou.
    pub fn mark_sent(&self, message_id: u64) -> bool {
        self.chat().mark_sent(message_id)
    }

    pub fn receive(&self, data: Vec<u8>, now_ms: u64) -> ReceiveResult {
        let r = self.chat().receive(&data, now_ms);
        ReceiveResult { message: r.message.as_ref().map(message_info), updated: r.updated, not_for_me: r.not_for_me }
    }

    /// Marca a conversa como lida e prepara as confirmações (✓✓ azul do outro lado).
    pub fn mark_read(&self, peer_id: String) -> Result<u32, NmError> {
        Ok(self.chat().mark_read(&peer(&peer_id)?) as u32)
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
        assert_eq!(bc.name, "Bruno");
        let sent = a.send_text(bc.peer_id.clone(), "oi".into(), 1).unwrap();
        assert_eq!(sent.status, MessageStatus::Pending);
        for p in a.outbox() {
            if let Some(id) = p.message_id {
                a.mark_sent(id);
            }
            let r = b.receive(p.data, 2);
            assert_eq!(r.message.unwrap().text, "oi");
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
    }
}
