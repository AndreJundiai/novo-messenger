//! Conversas, envio, recebimento e confirmações.
//!
//! Ciclo de uma mensagem enviada:
//! `Pending` (salva aqui) → `Sent` ✓ (entregue a algum transporte) →
//! `Delivered` ✓✓ (o destinatário devolveu "entregue") → `Read` ✓✓ azul.
//!
//! Tudo que sai (mensagens, edições, exclusões, reações) fica na caixa de
//! saída até chegar a confirmação de entrega, e é reenviado a cada nova
//! conexão, por qualquer transporte (Bluetooth, Wi-Fi, internet, rádio).
//! Quem recebe ignora cópias pelo `id`, mas confirma de novo.

use crate::card::{clean_name, make_card, parse_card, MAX_NAME_BYTES};
use crate::crypto::{self, Identity, KeyBytes};
use crate::shield;
use crate::wire::{Body, Reader, Writer, MAX_ACKS, MAX_EMOJI_BYTES};
use nm_core::b64;
use rand_core::{CryptoRngCore, OsRng, RngCore};
use std::collections::{BTreeMap, HashMap};

/// Maior texto de uma mensagem, em bytes UTF-8.
pub const MAX_TEXT_BYTES: usize = 4000;
const SAVE_V1: &[u8; 8] = b"NMCHAT01";
const SAVE_V2: &[u8; 8] = b"NMCHAT02";
const SAVE_V3: &[u8; 8] = b"NMCHAT03";
/// Código de confirmação por SMS: `NMV1:` + Base64(chave 32 ‖ código 10 ‖ nome).
pub const VERIFY_PREFIX: &str = "NMV1:";
const CODE_LEN: usize = 10;
const CODE_ALPHABET: &[u8; 32] = b"ABCDEFGHJKLMNPQRSTUVWXYZ23456789";

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Status {
    Pending,
    Sent,
    Delivered,
    Read,
}

impl Status {
    fn from_u8(v: u8) -> Option<Self> {
        Some(match v {
            0 => Status::Pending,
            1 => Status::Sent,
            2 => Status::Delivered,
            3 => Status::Read,
            _ => return None,
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Contact {
    pub key: KeyBytes,
    pub name: String,
    /// Adicionado lendo o QR ao vivo (selo "conferido pessoalmente").
    pub added_in_person: bool,
    /// Número de telefone ligado a este contato (formato internacional, só dígitos).
    pub phone: Option<String>,
    /// O dono do número confirmou pelo código enviado por SMS.
    pub phone_verified: bool,
}

impl Contact {
    fn new(key: KeyBytes, name: String, added_in_person: bool) -> Self {
        Self { key, name, added_in_person, phone: None, phone_verified: false }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ChatMessage {
    pub id: u64,
    pub peer: KeyBytes,
    pub outgoing: bool,
    pub text: String,
    pub at_ms: u64,
    /// Para mensagens enviadas.
    pub status: Status,
    /// Para mensagens recebidas: já foi lida aqui.
    pub read: bool,
    pub reply_to: Option<u64>,
    pub edited: bool,
    /// Apagada para todos (o texto some).
    pub deleted: bool,
    pub my_reaction: Option<String>,
    pub peer_reaction: Option<String>,
    /// A pessoa já fechou o alerta de golpe desta mensagem.
    pub alert_dismissed: bool,
}

impl ChatMessage {
    fn new(id: u64, peer: KeyBytes, outgoing: bool, text: String, at_ms: u64, reply_to: Option<u64>) -> Self {
        Self {
            id,
            peer,
            outgoing,
            text,
            at_ms,
            status: if outgoing { Status::Pending } else { Status::Delivered },
            read: outgoing,
            reply_to,
            edited: false,
            deleted: false,
            my_reaction: None,
            peer_reaction: None,
            alert_dismissed: false,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Conversation {
    pub peer: KeyBytes,
    pub name: String,
    pub last_text: String,
    pub last_at_ms: u64,
    pub last_outgoing: bool,
    pub last_status: Status,
    pub last_deleted: bool,
    pub unread: u32,
}

/// Um envelope pronto para sair, e para quem ele vai (para o transporte pela
/// internet saber a rota). `message_id` existe quando ele carrega uma
/// mensagem (para marcar ✓ depois que sair).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Outgoing {
    pub message_id: Option<u64>,
    pub to: KeyBytes,
    pub data: Vec<u8>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Received {
    /// Mensagem nova (não vem em cópias repetidas).
    pub message: Option<ChatMessage>,
    /// Mensagens enviadas cujo estado mudou (entregue ou lida).
    pub updated: Vec<u64>,
    /// Mensagens editadas, apagadas ou com reação nova.
    pub changed: Vec<u64>,
    /// O envelope não era para este aparelho (a fase 2 vai guardar e repassar).
    pub not_for_me: bool,
    /// Um contato confirmou o número pelo SMS.
    pub contact_verified: Option<KeyBytes>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ChatError {
    InvalidCard,
    OwnCard,
    UnknownContact,
    UnknownMessage,
    NotYours,
    EmptyText,
    TextTooLong,
    InvalidData,
    WeakPassword,
    WrongPassword,
}

impl ChatError {
    pub fn describe(&self) -> &'static str {
        match self {
            ChatError::InvalidCard => "esse código não é de um contato do Novo Messenger",
            ChatError::OwnCard => "esse é o seu próprio código",
            ChatError::UnknownContact => "contato não encontrado",
            ChatError::UnknownMessage => "mensagem não encontrada",
            ChatError::NotYours => "só dá para mudar mensagens que você enviou",
            ChatError::EmptyText => "mensagem vazia",
            ChatError::TextTooLong => "mensagem longa demais",
            ChatError::InvalidData => "arquivo inválido ou corrompido",
            ChatError::WeakPassword => "a senha precisa ter pelo menos 6 caracteres",
            ChatError::WrongPassword => "senha incorreta",
        }
    }
}

struct Sealed {
    to: KeyBytes,
    at_ms: u64,
    data: Vec<u8>,
}

pub struct Chat {
    me: Identity,
    name: String,
    money_alerts: bool,
    contacts: BTreeMap<KeyBytes, Contact>,
    messages: BTreeMap<u64, ChatMessage>,
    /// Tudo que saiu e ainda não teve confirmação de entrega, pelo `id`.
    sealed: HashMap<u64, Sealed>,
    /// Confirmações a enviar na próxima oportunidade (saem uma vez).
    acks: Vec<(KeyBytes, Vec<u8>)>,
    /// Códigos de SMS enviados e ainda não confirmados: contato → (número, código).
    pending_verify: HashMap<KeyBytes, (String, String)>,
}

fn check_text(text: &str) -> Result<String, ChatError> {
    let t = text.trim();
    if t.is_empty() {
        return Err(ChatError::EmptyText);
    }
    if t.len() > MAX_TEXT_BYTES {
        return Err(ChatError::TextTooLong);
    }
    Ok(t.to_string())
}

impl Chat {
    pub fn new(name: &str) -> Self {
        Self::with_identity(Identity::generate(&mut OsRng), name)
    }

    pub fn with_identity(me: Identity, name: &str) -> Self {
        Self {
            me,
            name: clean_name(name),
            money_alerts: true,
            contacts: BTreeMap::new(),
            messages: BTreeMap::new(),
            sealed: HashMap::new(),
            acks: Vec::new(),
            pending_verify: HashMap::new(),
        }
    }

    pub fn my_key(&self) -> &KeyBytes {
        self.me.public()
    }

    /// Segredo da identidade, para derivar a chave do diretório. Não exponha ao app.
    pub fn identity_secret(&self) -> [u8; 32] {
        self.me.secret_bytes()
    }

    pub fn my_name(&self) -> &str {
        &self.name
    }

    pub fn set_my_name(&mut self, name: &str) {
        self.name = clean_name(name);
    }

    /// Texto do QR "Meu código".
    pub fn my_card(&self) -> String {
        make_card(self.me.public(), &self.name)
    }

    // ---------- contatos ----------

    /// Adiciona (ou atualiza o nome de) um contato a partir do QR dele, lido ao vivo.
    pub fn add_contact(&mut self, card: &str) -> Result<Contact, ChatError> {
        let c = parse_card(card).ok_or(ChatError::InvalidCard)?;
        if &c.key == self.me.public() {
            return Err(ChatError::OwnCard);
        }
        let name = if c.name.is_empty() { "Sem nome".to_string() } else { c.name };
        let contact = self.contacts.entry(c.key).or_insert_with(|| Contact::new(c.key, name.clone(), true));
        contact.name = name;
        contact.added_in_person = true;
        Ok(contact.clone())
    }

    /// Contato encontrado pelo número no diretório: entra como "não confirmado"
    /// até o dono do número responder ao SMS ([`Chat::verification_code`]).
    pub fn add_found_contact(&mut self, card: &str, phone: &str) -> Result<Contact, ChatError> {
        let c = parse_card(card).ok_or(ChatError::InvalidCard)?;
        if &c.key == self.me.public() {
            return Err(ChatError::OwnCard);
        }
        let name = if c.name.is_empty() { "Sem nome".to_string() } else { c.name };
        let contact = self.contacts.entry(c.key).or_insert_with(|| Contact::new(c.key, name, false));
        if contact.phone.as_deref() != Some(phone) {
            contact.phone = Some(phone.to_string());
            contact.phone_verified = false;
        }
        Ok(contact.clone())
    }

    /// Texto para mandar por SMS ao número do contato. Só quem tem aquele chip
    /// recebe; quando ele colar o código no app, o número fica confirmado.
    pub fn verification_code(&mut self, key: &KeyBytes) -> Result<String, ChatError> {
        let phone = self.contacts.get(key).and_then(|c| c.phone.clone()).ok_or(ChatError::UnknownContact)?;
        let code: String = (0..CODE_LEN).map(|_| CODE_ALPHABET[(OsRng.next_u32() % 32) as usize] as char).collect();
        self.pending_verify.insert(*key, (phone, code.clone()));
        let mut payload = self.me.public().to_vec();
        payload.extend_from_slice(code.as_bytes());
        payload.extend_from_slice(self.name.as_bytes());
        Ok(format!("{VERIFY_PREFIX}{}", b64::encode(&payload)))
    }

    /// Código colado pela pessoa: um cartão (`NMC1:`) recebido por outro meio, ou
    /// o código de confirmação (`NMV1:`) que chegou por SMS. Nos dois casos o
    /// contato não foi conferido pessoalmente.
    pub fn accept_code(&mut self, text: &str, now_ms: u64) -> Result<Contact, ChatError> {
        let text = text.trim();
        if let Some(c) = parse_card(text) {
            if &c.key == self.me.public() {
                return Err(ChatError::OwnCard);
            }
            let name = if c.name.is_empty() { "Sem nome".to_string() } else { c.name };
            let contact = self.contacts.entry(c.key).or_insert_with(|| Contact::new(c.key, name, false));
            return Ok(contact.clone());
        }
        let bytes = text.strip_prefix(VERIFY_PREFIX).and_then(b64::decode).ok_or(ChatError::InvalidCard)?;
        if bytes.len() < 32 + CODE_LEN || bytes.len() > 32 + CODE_LEN + MAX_NAME_BYTES {
            return Err(ChatError::InvalidCard);
        }
        let key: KeyBytes = bytes[..32].try_into().map_err(|_| ChatError::InvalidCard)?;
        if &key == self.me.public() {
            return Err(ChatError::OwnCard);
        }
        let code = String::from_utf8(bytes[32..32 + CODE_LEN].to_vec()).map_err(|_| ChatError::InvalidCard)?;
        let name = clean_name(&String::from_utf8_lossy(&bytes[32 + CODE_LEN..]));
        let name = if name.is_empty() { "Sem nome".to_string() } else { name };
        let contact = self.contacts.entry(key).or_insert_with(|| Contact::new(key, name, false)).clone();
        let id = self.new_id(&mut OsRng);
        self.queue(&key, &Body::Verify { id, code }, now_ms, &mut OsRng)?;
        Ok(contact)
    }

    /// Contatos com nome igual ou parecido com `name`, exceto `except`
    /// (para avisar sobre o golpe do "troquei de número").
    pub fn similar_contacts(&self, name: &str, except: Option<&KeyBytes>) -> Vec<Contact> {
        self.contacts.values().filter(|c| Some(&c.key) != except && shield::similar_names(&c.name, name)).cloned().collect()
    }

    pub fn rename_contact(&mut self, key: &KeyBytes, name: &str) -> Result<(), ChatError> {
        let c = self.contacts.get_mut(key).ok_or(ChatError::UnknownContact)?;
        c.name = clean_name(name);
        Ok(())
    }

    pub fn contacts(&self) -> Vec<Contact> {
        let mut v: Vec<Contact> = self.contacts.values().cloned().collect();
        v.sort_by_key(|c| c.name.to_lowercase());
        v
    }

    pub fn contact(&self, key: &KeyBytes) -> Option<&Contact> {
        self.contacts.get(key)
    }

    // ---------- escudo contra golpes ----------

    pub fn money_alerts(&self) -> bool {
        self.money_alerts
    }

    pub fn set_money_alerts(&mut self, on: bool) {
        self.money_alerts = on;
    }

    /// Mostrar o alerta de pedido de dinheiro nesta mensagem?
    pub fn money_alert(&self, m: &ChatMessage) -> bool {
        self.money_alerts && !m.outgoing && !m.deleted && !m.alert_dismissed && shield::money_request(&m.text)
    }

    pub fn dismiss_alert(&mut self, id: u64) -> bool {
        match self.messages.get_mut(&id) {
            Some(m) if !m.alert_dismissed => {
                m.alert_dismissed = true;
                true
            }
            _ => false,
        }
    }

    // ---------- enviar ----------

    fn new_id(&self, rng: &mut impl CryptoRngCore) -> u64 {
        loop {
            let id = rng.next_u64();
            if id != 0 && !self.messages.contains_key(&id) && !self.sealed.contains_key(&id) {
                return id;
            }
        }
    }

    fn queue(&mut self, to: &KeyBytes, body: &Body, now_ms: u64, rng: &mut impl CryptoRngCore) -> Result<(), ChatError> {
        let id = body.id().ok_or(ChatError::InvalidData)?;
        let data = crypto::seal(&self.me, to, &body.encode(), rng).ok_or(ChatError::UnknownContact)?;
        self.sealed.insert(id, Sealed { to: *to, at_ms: now_ms, data });
        Ok(())
    }

    pub fn send_text(&mut self, to: &KeyBytes, text: &str, now_ms: u64) -> Result<ChatMessage, ChatError> {
        self.send_with(to, text, None, now_ms, &mut OsRng)
    }

    /// Envia `text` respondendo à mensagem `reply_to` da mesma conversa.
    pub fn send_reply(&mut self, to: &KeyBytes, text: &str, reply_to: u64, now_ms: u64) -> Result<ChatMessage, ChatError> {
        self.send_with(to, text, Some(reply_to), now_ms, &mut OsRng)
    }

    pub fn send_with(&mut self, to: &KeyBytes, text: &str, reply_to: Option<u64>, now_ms: u64, rng: &mut impl CryptoRngCore) -> Result<ChatMessage, ChatError> {
        if !self.contacts.contains_key(to) {
            return Err(ChatError::UnknownContact);
        }
        let text = check_text(text)?;
        if let Some(r) = reply_to {
            if self.messages.get(&r).is_none_or(|m| m.peer != *to) {
                return Err(ChatError::UnknownMessage);
            }
        }
        let id = self.new_id(rng);
        self.queue(to, &Body::Message { id, reply_to, text: text.clone() }, now_ms, rng)?;
        let msg = ChatMessage::new(id, *to, true, text, now_ms, reply_to);
        self.messages.insert(id, msg.clone());
        Ok(msg)
    }

    fn own_message(&self, id: u64) -> Result<&ChatMessage, ChatError> {
        let m = self.messages.get(&id).ok_or(ChatError::UnknownMessage)?;
        if !m.outgoing {
            return Err(ChatError::NotYours);
        }
        Ok(m)
    }

    /// Edita uma mensagem enviada, aqui e no outro aparelho.
    pub fn edit_message(&mut self, id: u64, text: &str, now_ms: u64) -> Result<(), ChatError> {
        let m = self.own_message(id)?;
        if m.deleted {
            return Err(ChatError::UnknownMessage);
        }
        let (peer, text) = (m.peer, check_text(text)?);
        let edit_id = self.new_id(&mut OsRng);
        self.queue(&peer, &Body::Edit { id: edit_id, target: id, text: text.clone() }, now_ms, &mut OsRng)?;
        let m = self.messages.get_mut(&id).ok_or(ChatError::UnknownMessage)?;
        m.text = text.clone();
        m.edited = true;
        // Se a original ainda não saiu, manda já corrigida.
        if let Some(s) = self.sealed.get(&id) {
            let to = s.to;
            let reply_to = m.reply_to;
            self.queue(&to, &Body::Message { id, reply_to, text }, now_ms, &mut OsRng)?;
        }
        Ok(())
    }

    /// Apaga para todos: o texto some aqui e no outro aparelho.
    pub fn delete_for_everyone(&mut self, id: u64, now_ms: u64) -> Result<(), ChatError> {
        let peer = self.own_message(id)?.peer;
        let del_id = self.new_id(&mut OsRng);
        self.queue(&peer, &Body::Delete { id: del_id, target: id }, now_ms, &mut OsRng)?;
        self.sealed.remove(&id);
        if let Some(m) = self.messages.get_mut(&id) {
            m.deleted = true;
            m.text.clear();
        }
        Ok(())
    }

    /// Apaga só deste aparelho.
    pub fn delete_for_me(&mut self, id: u64) -> bool {
        self.sealed.remove(&id);
        self.messages.remove(&id).is_some()
    }

    /// Reage a uma mensagem da conversa (emoji vazio tira a reação).
    pub fn react(&mut self, id: u64, emoji: &str, now_ms: u64) -> Result<(), ChatError> {
        let emoji = emoji.trim();
        if emoji.len() > MAX_EMOJI_BYTES {
            return Err(ChatError::TextTooLong);
        }
        let m = self.messages.get(&id).ok_or(ChatError::UnknownMessage)?;
        if m.deleted {
            return Err(ChatError::UnknownMessage);
        }
        let peer = m.peer;
        let react_id = self.new_id(&mut OsRng);
        self.queue(&peer, &Body::React { id: react_id, target: id, emoji: emoji.to_string() }, now_ms, &mut OsRng)?;
        if let Some(m) = self.messages.get_mut(&id) {
            m.my_reaction = (!emoji.is_empty()).then(|| emoji.to_string());
        }
        Ok(())
    }

    /// Tudo que deve sair na próxima conexão: confirmações (saem uma vez) e o
    /// que ainda não teve confirmação de entrega (sai até ser confirmado).
    pub fn outbox(&mut self) -> Vec<Outgoing> {
        let mut out: Vec<Outgoing> = std::mem::take(&mut self.acks).into_iter().map(|(to, data)| Outgoing { message_id: None, to, data }).collect();
        let mut pending: Vec<(&u64, &Sealed)> = self.sealed.iter().collect();
        pending.sort_by_key(|(id, s)| (s.at_ms, **id));
        out.extend(pending.into_iter().map(|(id, s)| Outgoing {
            message_id: self.messages.contains_key(id).then_some(*id),
            to: s.to,
            data: s.data.clone(),
        }));
        out
    }

    /// Há algo esperando para sair?
    pub fn has_outgoing(&self) -> bool {
        !self.acks.is_empty() || !self.sealed.is_empty()
    }

    /// O app conseguiu entregar a mensagem a um transporte: ✓.
    pub fn mark_sent(&mut self, id: u64) -> bool {
        match self.messages.get_mut(&id) {
            Some(m) if m.outgoing && m.status == Status::Pending => {
                m.status = Status::Sent;
                true
            }
            _ => false,
        }
    }

    fn queue_ack(&mut self, to: &KeyBytes, body: Body) {
        if let Some(env) = crypto::seal(&self.me, to, &body.encode(), &mut OsRng) {
            self.acks.push((*to, env));
        }
    }

    // ---------- receber ----------

    /// Processa um envelope que chegou por qualquer transporte.
    pub fn receive(&mut self, data: &[u8], now_ms: u64) -> Received {
        if !crypto::is_for(&self.me, data) {
            return Received { not_for_me: true, ..Received::default() };
        }
        let Some((from, plain)) = crypto::open(&self.me, self.contacts.keys(), data) else { return Received::default() };
        let Some(body) = Body::decode(&plain) else { return Received::default() };
        let mut r = Received::default();
        match body {
            Body::Text { id, text } => self.on_message(&from, id, None, text, now_ms, &mut r),
            Body::Message { id, reply_to, text } => self.on_message(&from, id, reply_to, text, now_ms, &mut r),
            Body::Edit { id, target, text } => {
                self.queue_ack(&from, Body::Delivered(vec![id]));
                if let Some(m) = self.messages.get_mut(&target) {
                    if !m.outgoing && m.peer == from && !m.deleted && m.text != text {
                        m.text = text;
                        m.edited = true;
                        m.alert_dismissed = false;
                        r.changed.push(target);
                    }
                }
            }
            Body::Delete { id, target } => {
                self.queue_ack(&from, Body::Delivered(vec![id]));
                if let Some(m) = self.messages.get_mut(&target) {
                    if !m.outgoing && m.peer == from && !m.deleted {
                        m.deleted = true;
                        m.text.clear();
                        r.changed.push(target);
                    }
                }
            }
            Body::React { id, target, emoji } => {
                self.queue_ack(&from, Body::Delivered(vec![id]));
                if let Some(m) = self.messages.get_mut(&target) {
                    let new = (!emoji.is_empty()).then_some(emoji);
                    if m.peer == from && m.peer_reaction != new {
                        m.peer_reaction = new;
                        r.changed.push(target);
                    }
                }
            }
            Body::Verify { id, code } => {
                self.queue_ack(&from, Body::Delivered(vec![id]));
                if self.pending_verify.get(&from).is_some_and(|(_, c)| *c == code) {
                    if let (Some((phone, _)), Some(c)) = (self.pending_verify.remove(&from), self.contacts.get_mut(&from)) {
                        c.phone = Some(phone);
                        c.phone_verified = true;
                        r.contact_verified = Some(from);
                    }
                }
            }
            Body::Delivered(ids) => self.apply_acks(&from, &ids, Status::Delivered, &mut r.updated),
            Body::Read(ids) => self.apply_acks(&from, &ids, Status::Read, &mut r.updated),
        }
        r
    }

    fn on_message(&mut self, from: &KeyBytes, id: u64, reply_to: Option<u64>, text: String, now_ms: u64, r: &mut Received) {
        match self.messages.get(&id).map(|m| (m.peer, m.read)) {
            Some((peer, read)) if peer == *from => {
                // Cópia: confirma de novo, porque a confirmação anterior pode ter se perdido.
                self.queue_ack(from, if read { Body::Read(vec![id]) } else { Body::Delivered(vec![id]) });
            }
            Some(_) => {} // id já usado por outra conversa: ignora
            None => {
                let reply_to = reply_to.filter(|r| self.messages.get(r).is_some_and(|m| m.peer == *from));
                let msg = ChatMessage::new(id, *from, false, text, now_ms, reply_to);
                self.messages.insert(id, msg.clone());
                self.queue_ack(from, Body::Delivered(vec![id]));
                r.message = Some(msg);
            }
        }
    }

    /// Confirmações de `from` só valem para o que foi enviado a `from`.
    fn apply_acks(&mut self, from: &KeyBytes, ids: &[u64], status: Status, updated: &mut Vec<u64>) {
        for id in ids {
            if self.sealed.get(id).is_some_and(|s| s.to == *from) {
                self.sealed.remove(id);
            }
            let Some(m) = self.messages.get_mut(id) else { continue };
            if !m.outgoing || m.peer != *from {
                continue;
            }
            if m.status < status {
                m.status = status;
                updated.push(*id);
            }
        }
    }

    /// Marca como lidas as mensagens recebidas de `peer` e prepara as
    /// confirmações de leitura. Devolve quantas foram marcadas.
    pub fn mark_read(&mut self, peer: &KeyBytes) -> usize {
        let ids: Vec<u64> = self
            .messages
            .values_mut()
            .filter(|m| !m.outgoing && m.peer == *peer && !m.read)
            .map(|m| {
                m.read = true;
                m.id
            })
            .collect();
        for chunk in ids.chunks(MAX_ACKS) {
            self.queue_ack(peer, Body::Read(chunk.to_vec()));
        }
        ids.len()
    }

    // ---------- consultar ----------

    pub fn message(&self, id: u64) -> Option<&ChatMessage> {
        self.messages.get(&id)
    }

    /// Mensagens de uma conversa, da mais antiga para a mais nova.
    pub fn messages(&self, peer: &KeyBytes) -> Vec<ChatMessage> {
        let mut v: Vec<ChatMessage> = self.messages.values().filter(|m| m.peer == *peer).cloned().collect();
        v.sort_by_key(|m| (m.at_ms, m.id));
        v
    }

    /// Busca em todas as conversas, ignorando acentos e maiúsculas. Mais recentes primeiro.
    pub fn search(&self, query: &str) -> Vec<ChatMessage> {
        let q = shield::normalize(query);
        if q.is_empty() {
            return Vec::new();
        }
        let mut v: Vec<ChatMessage> = self.messages.values().filter(|m| !m.deleted && shield::normalize(&m.text).contains(&q)).cloned().collect();
        v.sort_by_key(|m| std::cmp::Reverse((m.at_ms, m.id)));
        v.truncate(100);
        v
    }

    /// Conversas, da mais recente para a mais antiga. Contatos sem mensagens
    /// não aparecem (estão em [`Chat::contacts`]).
    pub fn conversations(&self) -> Vec<Conversation> {
        let mut by_peer: HashMap<KeyBytes, (&ChatMessage, u32)> = HashMap::new();
        for m in self.messages.values() {
            let e = by_peer.entry(m.peer).or_insert((m, 0));
            if (m.at_ms, m.id) > (e.0.at_ms, e.0.id) {
                e.0 = m;
            }
            if !m.outgoing && !m.read {
                e.1 += 1;
            }
        }
        let mut v: Vec<Conversation> = by_peer
            .into_iter()
            .map(|(peer, (m, unread))| Conversation {
                peer,
                name: self.contacts.get(&peer).map_or_else(|| "Contato removido".to_string(), |c| c.name.clone()),
                last_text: m.text.clone(),
                last_at_ms: m.at_ms,
                last_outgoing: m.outgoing,
                last_status: m.status,
                last_deleted: m.deleted,
                unread,
            })
            .collect();
        v.sort_by_key(|c| std::cmp::Reverse(c.last_at_ms));
        v
    }

    // ---------- salvar e restaurar ----------

    pub fn save(&self) -> Vec<u8> {
        let mut w = Writer::default();
        w.raw(SAVE_V3);
        w.raw(&self.me.secret_bytes());
        w.bytes(self.name.as_bytes());
        w.u8(self.money_alerts as u8);
        w.u32(self.contacts.len() as u32);
        for c in self.contacts.values() {
            w.raw(&c.key);
            w.bytes(c.name.as_bytes());
            w.u8(c.added_in_person as u8);
            w.bytes(c.phone.as_deref().unwrap_or("").as_bytes());
            w.u8(c.phone_verified as u8);
        }
        w.u32(self.messages.len() as u32);
        for m in self.messages.values() {
            w.u64(m.id);
            w.raw(&m.peer);
            let flags = m.outgoing as u8
                | (m.read as u8) << 1
                | (m.edited as u8) << 2
                | (m.deleted as u8) << 3
                | (m.alert_dismissed as u8) << 4
                | (m.reply_to.is_some() as u8) << 5;
            w.u8(flags);
            w.u8(m.status as u8);
            w.u64(m.at_ms);
            if let Some(r) = m.reply_to {
                w.u64(r);
            }
            w.bytes(m.text.as_bytes());
            w.bytes(m.my_reaction.as_deref().unwrap_or("").as_bytes());
            w.bytes(m.peer_reaction.as_deref().unwrap_or("").as_bytes());
        }
        w.u32(self.sealed.len() as u32);
        for (id, s) in &self.sealed {
            w.u64(*id);
            w.raw(&s.to);
            w.u64(s.at_ms);
            w.bytes(&s.data);
        }
        w.u32(self.acks.len() as u32);
        for (to, a) in &self.acks {
            w.raw(to);
            w.bytes(a);
        }
        w.u32(self.pending_verify.len() as u32);
        for (k, (phone, code)) in &self.pending_verify {
            w.raw(k);
            w.bytes(phone.as_bytes());
            w.bytes(code.as_bytes());
        }
        w.0
    }

    pub fn load(data: &[u8]) -> Result<Chat, ChatError> {
        match data.get(..8) {
            Some(m) if m == SAVE_V3 => Self::read_v23(data, true),
            Some(m) if m == SAVE_V2 => Self::read_v23(data, false),
            Some(m) if m == SAVE_V1 => Self::read_v1(data),
            _ => None,
        }
        .ok_or(ChatError::InvalidData)
    }

    /// Formatos 2 e 3 (o 3 acrescenta o telefone dos contatos e os códigos pendentes).
    fn read_v23(data: &[u8], v3: bool) -> Option<Chat> {
        let mut r = Reader::new(&data[8..]);
        let me = Identity::from_secret(r.key()?);
        let name = r.string()?;
        let mut chat = Chat::with_identity(me, &name);
        chat.money_alerts = r.u8()? != 0;
        for _ in 0..r.u32()? {
            let key = r.key()?;
            let name = r.string()?;
            let mut c = Contact::new(key, name, r.u8()? != 0);
            if v3 {
                c.phone = Some(r.string()?).filter(|p| !p.is_empty());
                c.phone_verified = r.u8()? != 0;
            }
            chat.contacts.insert(key, c);
        }
        let opt = |s: String| (!s.is_empty()).then_some(s);
        for _ in 0..r.u32()? {
            let id = r.u64()?;
            let peer = r.key()?;
            let flags = r.u8()?;
            let status = Status::from_u8(r.u8()?)?;
            let at_ms = r.u64()?;
            let reply_to = if flags & 32 != 0 { Some(r.u64()?) } else { None };
            let mut m = ChatMessage::new(id, peer, flags & 1 != 0, r.string()?, at_ms, reply_to);
            m.status = status;
            m.read = flags & 2 != 0;
            m.edited = flags & 4 != 0;
            m.deleted = flags & 8 != 0;
            m.alert_dismissed = flags & 16 != 0;
            m.my_reaction = opt(r.string()?);
            m.peer_reaction = opt(r.string()?);
            chat.messages.insert(id, m);
        }
        for _ in 0..r.u32()? {
            let id = r.u64()?;
            let to = r.key()?;
            let at_ms = r.u64()?;
            chat.sealed.insert(id, Sealed { to, at_ms, data: r.bytes()?.to_vec() });
        }
        for _ in 0..r.u32()? {
            let to = r.key()?;
            chat.acks.push((to, r.bytes()?.to_vec()));
        }
        if v3 {
            for _ in 0..r.u32()? {
                let k = r.key()?;
                let phone = r.string()?;
                chat.pending_verify.insert(k, (phone, r.string()?));
            }
        }
        r.done().then_some(chat)
    }

    /// Formato da fase 1: sem selo, sem reações, envelopes sem destinatário.
    fn read_v1(data: &[u8]) -> Option<Chat> {
        let mut r = Reader::new(&data[8..]);
        let me = Identity::from_secret(r.key()?);
        let name = r.string()?;
        let mut chat = Chat::with_identity(me, &name);
        for _ in 0..r.u32()? {
            let key = r.key()?;
            // Na fase 1 a única forma de adicionar era pelo QR.
            chat.contacts.insert(key, Contact::new(key, r.string()?, true));
        }
        for _ in 0..r.u32()? {
            let id = r.u64()?;
            let peer = r.key()?;
            let flags = r.u8()?;
            let status = Status::from_u8(r.u8()?)?;
            let at_ms = r.u64()?;
            let mut m = ChatMessage::new(id, peer, flags & 1 != 0, r.string()?, at_ms, None);
            m.status = status;
            m.read = flags & 2 != 0;
            chat.messages.insert(id, m);
        }
        for _ in 0..r.u32()? {
            let id = r.u64()?;
            let env = r.bytes()?.to_vec();
            if let Some(m) = chat.messages.get(&id) {
                let (to, at_ms) = (m.peer, m.at_ms);
                chat.sealed.insert(id, Sealed { to, at_ms, data: env });
            }
        }
        for _ in 0..r.u32()? {
            r.bytes()?; // confirmações antigas sem destinatário: descartadas (são reenviadas sob demanda)
        }
        r.done().then_some(chat)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Dois aparelhos que já trocaram QR.
    fn pair() -> (Chat, Chat) {
        let mut a = Chat::new("Ana");
        let mut b = Chat::new("Bruno");
        a.add_contact(&b.my_card()).unwrap();
        b.add_contact(&a.my_card()).unwrap();
        (a, b)
    }

    /// Entrega tudo da caixa de saída de `from` para `to` (uma "conexão").
    fn deliver(from: &mut Chat, to: &mut Chat, now: u64) -> Vec<Received> {
        let dest = *to.my_key();
        from.outbox()
            .into_iter()
            .filter(|o| o.to == dest) // o que é para outra pessoa não passa por esta "conexão"
            .map(|o| {
                if let Some(id) = o.message_id {
                    from.mark_sent(id);
                }
                to.receive(&o.data, now)
            })
            .collect()
    }

    /// Ida e volta até as duas caixas ficarem vazias.
    fn sync(a: &mut Chat, b: &mut Chat, now: u64) {
        for _ in 0..4 {
            deliver(a, b, now);
            deliver(b, a, now);
        }
    }

    #[test]
    fn full_tick_cycle() {
        let (mut a, mut b) = pair();
        let bk = *b.my_key();
        let ak = *a.my_key();
        let m = a.send_text(&bk, "Oi Bruno, corte às 15h?", 1000).unwrap();
        assert_eq!(a.messages(&bk)[0].status, Status::Pending);

        let got = deliver(&mut a, &mut b, 2000);
        assert_eq!(got[0].message.as_ref().unwrap().text, "Oi Bruno, corte às 15h?");
        assert_eq!(a.messages(&bk)[0].status, Status::Sent); // ✓
        assert_eq!(b.conversations()[0].unread, 1);

        let back = deliver(&mut b, &mut a, 3000);
        assert_eq!(back[0].updated, vec![m.id]);
        assert_eq!(a.messages(&bk)[0].status, Status::Delivered); // ✓✓
        assert!(a.outbox().is_empty(), "entregue: não reenvia mais");
        assert!(!a.has_outgoing());

        assert_eq!(b.mark_read(&ak), 1);
        assert_eq!(b.conversations()[0].unread, 0);
        deliver(&mut b, &mut a, 4000);
        assert_eq!(a.messages(&bk)[0].status, Status::Read); // ✓✓ azul
    }

    #[test]
    fn lost_ack_is_recovered_by_resend() {
        let (mut a, mut b) = pair();
        let bk = *b.my_key();
        a.send_text(&bk, "oi", 1).unwrap();
        deliver(&mut a, &mut b, 2);
        b.outbox(); // confirmação perdida no caminho
        let again = deliver(&mut a, &mut b, 3);
        assert!(again[0].message.is_none());
        assert_eq!(b.messages(a.my_key()).len(), 1);
        deliver(&mut b, &mut a, 4);
        assert_eq!(a.messages(&bk)[0].status, Status::Delivered);
    }

    #[test]
    fn reply_edit_delete_and_react_reach_the_other_side() {
        let (mut a, mut b) = pair();
        let (ak, bk) = (*a.my_key(), *b.my_key());
        let q = a.send_text(&bk, "Corte às 15h?", 1).unwrap();
        sync(&mut a, &mut b, 2);

        let r = b.send_reply(&ak, "Pode ser às 16h?", q.id, 3).unwrap();
        sync(&mut a, &mut b, 4);
        assert_eq!(a.message(r.id).unwrap().reply_to, Some(q.id));

        a.edit_message(q.id, "Corte às 16h então", 5).unwrap();
        sync(&mut a, &mut b, 6);
        let seen = b.message(q.id).unwrap();
        assert!(seen.edited && seen.text == "Corte às 16h então");

        b.react(q.id, "👍", 7).unwrap();
        sync(&mut a, &mut b, 8);
        assert_eq!(a.message(q.id).unwrap().peer_reaction.as_deref(), Some("👍"));
        assert_eq!(b.message(q.id).unwrap().my_reaction.as_deref(), Some("👍"));
        b.react(q.id, "", 9).unwrap();
        sync(&mut a, &mut b, 10);
        assert_eq!(a.message(q.id).unwrap().peer_reaction, None);

        a.delete_for_everyone(q.id, 11).unwrap();
        sync(&mut a, &mut b, 12);
        let gone = b.message(q.id).unwrap();
        assert!(gone.deleted && gone.text.is_empty());
        assert!(!a.has_outgoing() && !b.has_outgoing(), "tudo confirmado, nada pendente");

        // Regras: não dá para editar/apagar a mensagem do outro.
        assert_eq!(b.edit_message(q.id, "x", 13), Err(ChatError::NotYours));
        assert_eq!(a.edit_message(r.id, "x", 13), Err(ChatError::NotYours));
        assert_eq!(a.delete_for_everyone(r.id, 13), Err(ChatError::NotYours));
        assert_eq!(a.send_reply(&bk, "x", 999, 13).map(|_| ()), Err(ChatError::UnknownMessage));
        assert!(a.delete_for_me(r.id));
        assert!(a.message(r.id).is_none());
    }

    #[test]
    fn forged_edit_from_someone_else_is_ignored() {
        let (mut a, mut b) = pair();
        let mut c = Chat::new("Carla");
        b.add_contact(&c.my_card()).unwrap();
        c.add_contact(&b.my_card()).unwrap();
        let bk = *b.my_key();
        let m = a.send_text(&bk, "original da Ana", 1).unwrap();
        sync(&mut a, &mut b, 2);
        // Carla (contato do Bruno) tenta editar e apagar a mensagem da Ana.
        let body = Body::Edit { id: 77, target: m.id, text: "adulterada".into() };
        let env = crypto::seal(&c.me, &bk, &body.encode(), &mut OsRng).unwrap();
        b.receive(&env, 3);
        let body = Body::Delete { id: 78, target: m.id };
        let env = crypto::seal(&c.me, &bk, &body.encode(), &mut OsRng).unwrap();
        b.receive(&env, 3);
        assert_eq!(b.message(m.id).unwrap().text, "original da Ana");
        assert!(!b.message(m.id).unwrap().deleted);
    }

    #[test]
    fn money_alert_and_similar_names() {
        let (mut a, mut b) = pair();
        let ak = *a.my_key();
        let golpe = b.send_text(&ak, "me manda um pix urgente", 1).unwrap();
        let normal = b.send_text(&ak, "comprei pão", 2).unwrap();
        sync(&mut a, &mut b, 3);
        assert!(a.money_alert(a.message(golpe.id).unwrap()));
        assert!(!a.money_alert(a.message(normal.id).unwrap()));
        assert!(!b.money_alert(b.message(golpe.id).unwrap()), "não alerta em mensagem enviada");
        assert!(a.dismiss_alert(golpe.id));
        assert!(!a.money_alert(a.message(golpe.id).unwrap()));
        a.set_money_alerts(false);
        let g2 = b.send_text(&ak, "faz uma transferência", 4).unwrap();
        sync(&mut a, &mut b, 5);
        assert!(!a.money_alert(a.message(g2.id).unwrap()));

        let impostor = Chat::new("bruno");
        let c = parse_card(&impostor.my_card()).unwrap();
        let sim = a.similar_contacts(&c.name, Some(&c.key));
        assert_eq!(sim.len(), 1);
        assert_eq!(sim[0].name, "Bruno");
        assert!(a.contacts()[0].added_in_person);
    }

    #[test]
    fn phone_verification_by_sms_code() {
        let mut a = Chat::new("Ana");
        let mut b = Chat::new("Bruno");
        // A encontra B pelo número no diretório.
        let found = a.add_found_contact(&b.my_card(), "5511987654321").unwrap();
        assert!(!found.added_in_person && !found.phone_verified);
        let sms = a.verification_code(b.my_key()).unwrap();
        assert!(sms.starts_with(VERIFY_PREFIX));
        // B recebe o SMS no chip dele e cola o código: adiciona A e responde cifrado.
        let added = b.accept_code(&sms, 1).unwrap();
        assert_eq!(&added.key, a.my_key());
        assert!(!added.added_in_person);
        let mut verified = None;
        for o in b.outbox() {
            verified = verified.or(a.receive(&o.data, 2).contact_verified);
        }
        assert_eq!(verified, Some(*b.my_key()));
        let c = a.contact(b.my_key()).unwrap();
        assert!(c.phone_verified && c.phone.as_deref() == Some("5511987654321"));
        // Salvar e abrir de novo mantém o número confirmado.
        assert!(Chat::load(&a.save()).unwrap().contact(b.my_key()).unwrap().phone_verified);
    }

    #[test]
    fn impostor_in_directory_cannot_verify() {
        // Eve publicou o número da Mãe com a chave dela. A Ana adiciona e manda o SMS:
        // o SMS vai para o chip da Mãe de verdade, que não é a Eve.
        let mut ana = Chat::new("Ana");
        let mut eve = Chat::new("Mãe");
        let mut mae = Chat::new("Mãe");
        ana.add_found_contact(&eve.my_card(), "5511900000000").unwrap();
        let sms = ana.verification_code(eve.my_key()).unwrap();
        mae.accept_code(&sms, 1).unwrap(); // a Mãe verdadeira recebeu o SMS
        for o in mae.outbox() {
            assert_eq!(ana.receive(&o.data, 2).contact_verified, None, "a mãe de verdade não é contato da Ana");
        }
        // Eve tenta adivinhar o código.
        eve.add_contact(&ana.my_card()).unwrap();
        let body = Body::Verify { id: 9, code: "AAAAAAAAAA".into() };
        let env = crypto::seal(&eve.me, ana.my_key(), &body.encode(), &mut OsRng).unwrap();
        assert_eq!(ana.receive(&env, 3).contact_verified, None);
        assert!(!ana.contact(eve.my_key()).unwrap().phone_verified);
        assert!(ana.accept_code("NMV1:lixo", 4).is_err());
        assert!(ana.accept_code(&ana.my_card(), 4).is_err());
    }

    #[test]
    fn search_ignores_accents() {
        let (mut a, b) = pair();
        a.send_text(b.my_key(), "Reunião na barbearia amanhã", 1).unwrap();
        a.send_text(b.my_key(), "outra coisa", 2).unwrap();
        assert_eq!(a.search("REUNIAO").len(), 1);
        assert_eq!(a.search("barbe").len(), 1);
        assert!(a.search("  ").is_empty());
    }

    #[test]
    fn strangers_and_forged_acks_are_ignored() {
        let (mut a, b) = pair();
        let mut eve = Chat::new("Eve");
        eve.add_contact(&a.my_card()).unwrap();
        let bk = *b.my_key();
        eve.send_text(a.my_key(), "clique aqui", 1).unwrap();
        for o in eve.outbox() {
            assert_eq!(a.receive(&o.data, 2), Received::default());
        }
        a.send_text(&bk, "segredo", 3).unwrap();
        for o in a.outbox() {
            assert!(eve.receive(&o.data, 4).not_for_me);
        }
        assert_eq!(a.send_text(eve.my_key(), "x", 5), Err(ChatError::UnknownContact));
        assert_eq!(a.send_text(&bk, "   ", 5), Err(ChatError::EmptyText));
        assert_eq!(a.send_text(&bk, &"x".repeat(MAX_TEXT_BYTES + 1), 5), Err(ChatError::TextTooLong));
        assert_eq!(a.add_contact(&a.my_card()), Err(ChatError::OwnCard));
        assert_eq!(a.add_contact("qualquer coisa"), Err(ChatError::InvalidCard));
    }

    #[test]
    fn conversations_sorted_and_named() {
        let (mut a, mut b) = pair();
        let mut c = Chat::new("Carla");
        a.add_contact(&c.my_card()).unwrap();
        c.add_contact(&a.my_card()).unwrap();
        a.send_text(b.my_key(), "primeira", 10).unwrap();
        a.send_text(c.my_key(), "segunda", 20).unwrap();
        let conv = a.conversations();
        assert_eq!(conv.iter().map(|c| c.name.as_str()).collect::<Vec<_>>(), ["Carla", "Bruno"]);
        assert!(conv[0].last_outgoing && conv[0].last_status == Status::Pending);
        deliver(&mut a, &mut b, 30);
        assert_eq!(b.conversations()[0].name, "Ana");
    }

    #[test]
    fn save_and_load_keeps_everything() {
        let (mut a, mut b) = pair();
        let bk = *b.my_key();
        let q = a.send_text(&bk, "salva aí ✂️", 1).unwrap();
        sync(&mut a, &mut b, 2);
        b.react(q.id, "❤️", 3).unwrap();
        let r = a.send_reply(&bk, "respondendo", q.id, 4).unwrap();
        a.set_money_alerts(false);
        let a2 = Chat::load(&a.save()).unwrap();
        assert_eq!(a2.my_key(), a.my_key());
        assert_eq!(a2.my_name(), "Ana");
        assert_eq!(a2.messages(&bk), a.messages(&bk));
        assert_eq!(a2.contacts(), a.contacts());
        assert!(!a2.money_alerts());
        assert_eq!(a2.message(r.id).unwrap().reply_to, Some(q.id));
        // A restaurada continua reenviando a pendente.
        let mut a2 = a2;
        sync(&mut a2, &mut b, 5);
        assert_eq!(a2.message(r.id).unwrap().status, Status::Delivered);
        assert_eq!(a2.message(q.id).unwrap().peer_reaction.as_deref(), Some("❤️"));
        assert!(Chat::load(b"lixo").is_err());
        let mut cut = a.save();
        cut.pop();
        assert!(Chat::load(&cut).is_err());
    }

    #[test]
    fn loads_phase_1_save_files() {
        // Arquivo no formato da fase 1, montado à mão: 1 contato, 1 mensagem pendente.
        let me = Identity::generate(&mut OsRng);
        let peer = Chat::new("Bruno");
        let pk = *peer.my_key();
        let env = crypto::seal(&me, &pk, &Body::Text { id: 5, text: "oi".into() }.encode(), &mut OsRng).unwrap();
        let mut w = Writer::default();
        w.raw(SAVE_V1);
        w.raw(&me.secret_bytes());
        w.bytes(b"Ana");
        w.u32(1);
        w.raw(&pk);
        w.bytes(b"Bruno");
        w.u32(1);
        w.u64(5);
        w.raw(&pk);
        w.u8(1 | 2);
        w.u8(0);
        w.u64(100);
        w.bytes(b"oi");
        w.u32(1);
        w.u64(5);
        w.bytes(&env);
        w.u32(0);
        let mut old = Chat::load(&w.0).unwrap();
        assert!(old.contacts()[0].added_in_person);
        let out = old.outbox();
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].to, pk);
        assert_eq!(out[0].message_id, Some(5));
    }
}
