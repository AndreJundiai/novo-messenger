//! Conversas, envio, recebimento e confirmações.
//!
//! Ciclo de uma mensagem enviada:
//! `Pending` (salva aqui) → `Sent` ✓ (entregue a algum transporte) →
//! `Delivered` ✓✓ (o destinatário devolveu "entregue") → `Read` ✓✓ azul.
//!
//! Enquanto não chega a confirmação, o envelope fica na caixa de saída e é
//! reenviado a cada nova conexão. Quem recebe ignora cópias pelo `id`, mas
//! confirma de novo (a confirmação anterior pode ter se perdido).

use crate::card::{clean_name, make_card, parse_card};
use crate::crypto::{self, Identity, KeyBytes};
use crate::wire::{Body, Reader, Writer, MAX_ACKS};
use rand_core::{CryptoRngCore, OsRng};
use std::collections::{BTreeMap, HashMap};

/// Maior texto de uma mensagem, em bytes UTF-8.
pub const MAX_TEXT_BYTES: usize = 4000;
const SAVE_MAGIC: &[u8; 8] = b"NMCHAT01";

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
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Conversation {
    pub peer: KeyBytes,
    pub name: String,
    pub last_text: String,
    pub last_at_ms: u64,
    pub last_outgoing: bool,
    pub last_status: Status,
    pub unread: u32,
}

/// Um envelope pronto para sair. `message_id` existe quando ele carrega uma
/// mensagem (para marcar ✓ depois que sair).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Outgoing {
    pub message_id: Option<u64>,
    pub data: Vec<u8>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Received {
    /// Mensagem nova (não vem em cópias repetidas).
    pub message: Option<ChatMessage>,
    /// Mensagens enviadas cujo estado mudou (entregue ou lida).
    pub updated: Vec<u64>,
    /// O envelope não era para este aparelho (a fase 2 vai guardar e repassar).
    pub not_for_me: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ChatError {
    InvalidCard,
    OwnCard,
    UnknownContact,
    EmptyText,
    TextTooLong,
    InvalidData,
}

impl ChatError {
    pub fn describe(&self) -> &'static str {
        match self {
            ChatError::InvalidCard => "esse código não é de um contato do Novo Messenger",
            ChatError::OwnCard => "esse é o seu próprio código",
            ChatError::UnknownContact => "contato não encontrado",
            ChatError::EmptyText => "mensagem vazia",
            ChatError::TextTooLong => "mensagem longa demais",
            ChatError::InvalidData => "dados salvos inválidos",
        }
    }
}

pub struct Chat {
    me: Identity,
    name: String,
    contacts: BTreeMap<KeyBytes, Contact>,
    messages: BTreeMap<u64, ChatMessage>,
    /// Envelopes de mensagens enviadas ainda sem confirmação de entrega.
    sealed: HashMap<u64, Vec<u8>>,
    /// Confirmações a enviar na próxima oportunidade (saem uma vez).
    acks: Vec<Vec<u8>>,
}

impl Chat {
    pub fn new(name: &str) -> Self {
        Self::with_identity(Identity::generate(&mut OsRng), name)
    }

    pub fn with_identity(me: Identity, name: &str) -> Self {
        Self { me, name: clean_name(name), contacts: BTreeMap::new(), messages: BTreeMap::new(), sealed: HashMap::new(), acks: Vec::new() }
    }

    pub fn my_key(&self) -> &KeyBytes {
        self.me.public()
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

    /// Adiciona (ou atualiza o nome de) um contato a partir do QR dele.
    pub fn add_contact(&mut self, card: &str) -> Result<Contact, ChatError> {
        let c = parse_card(card).ok_or(ChatError::InvalidCard)?;
        if &c.key == self.me.public() {
            return Err(ChatError::OwnCard);
        }
        let name = if c.name.is_empty() { "Sem nome".to_string() } else { c.name };
        let contact = Contact { key: c.key, name };
        self.contacts.insert(c.key, contact.clone());
        Ok(contact)
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

    fn seal(&self, to: &KeyBytes, body: &Body, rng: &mut impl CryptoRngCore) -> Option<Vec<u8>> {
        crypto::seal(&self.me, to, &body.encode(), rng)
    }

    pub fn send_text(&mut self, to: &KeyBytes, text: &str, now_ms: u64) -> Result<ChatMessage, ChatError> {
        self.send_text_with(to, text, now_ms, &mut OsRng)
    }

    pub fn send_text_with(&mut self, to: &KeyBytes, text: &str, now_ms: u64, rng: &mut impl CryptoRngCore) -> Result<ChatMessage, ChatError> {
        if !self.contacts.contains_key(to) {
            return Err(ChatError::UnknownContact);
        }
        let text = text.trim();
        if text.is_empty() {
            return Err(ChatError::EmptyText);
        }
        if text.len() > MAX_TEXT_BYTES {
            return Err(ChatError::TextTooLong);
        }
        let mut id = rng.next_u64();
        while id == 0 || self.messages.contains_key(&id) {
            id = rng.next_u64();
        }
        let env = self.seal(to, &Body::Text { id, text: text.to_string() }, rng).ok_or(ChatError::UnknownContact)?;
        let msg = ChatMessage { id, peer: *to, outgoing: true, text: text.to_string(), at_ms: now_ms, status: Status::Pending, read: true };
        self.messages.insert(id, msg.clone());
        self.sealed.insert(id, env);
        Ok(msg)
    }

    /// Tudo que deve sair na próxima conexão: confirmações (saem uma vez) e
    /// mensagens ainda sem confirmação de entrega (saem até serem confirmadas).
    pub fn outbox(&mut self) -> Vec<Outgoing> {
        let mut out: Vec<Outgoing> = std::mem::take(&mut self.acks).into_iter().map(|data| Outgoing { message_id: None, data }).collect();
        let mut pending: Vec<(&u64, &Vec<u8>)> = self.sealed.iter().collect();
        pending.sort_by_key(|(id, _)| self.messages.get(id).map_or(0, |m| m.at_ms));
        out.extend(pending.into_iter().map(|(id, data)| Outgoing { message_id: Some(*id), data: data.clone() }));
        out
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
        if let Some(env) = self.seal(to, &body, &mut OsRng) {
            self.acks.push(env);
        }
    }

    /// Processa um envelope que chegou por qualquer transporte.
    pub fn receive(&mut self, data: &[u8], now_ms: u64) -> Received {
        if !crypto::is_for(&self.me, data) {
            return Received { not_for_me: true, ..Received::default() };
        }
        let Some((from, plain)) = crypto::open(&self.me, self.contacts.keys(), data) else { return Received::default() };
        let Some(body) = Body::decode(&plain) else { return Received::default() };
        let mut r = Received::default();
        match body {
            Body::Text { id, text } => {
                let already = self.messages.get(&id).map(|m| (m.peer, m.read));
                match already {
                    Some((peer, read)) if peer == from => {
                        // Cópia: confirma de novo, porque a confirmação anterior pode ter se perdido.
                        self.queue_ack(&from, if read { Body::Read(vec![id]) } else { Body::Delivered(vec![id]) });
                    }
                    Some(_) => {} // id já usado por outra conversa: ignora
                    None => {
                        let msg = ChatMessage { id, peer: from, outgoing: false, text, at_ms: now_ms, status: Status::Delivered, read: false };
                        self.messages.insert(id, msg.clone());
                        self.queue_ack(&from, Body::Delivered(vec![id]));
                        r.message = Some(msg);
                    }
                }
            }
            Body::Delivered(ids) => self.apply_acks(&from, &ids, Status::Delivered, &mut r.updated),
            Body::Read(ids) => self.apply_acks(&from, &ids, Status::Read, &mut r.updated),
        }
        r
    }

    /// Confirmações de `from` só valem para mensagens enviadas a `from`.
    fn apply_acks(&mut self, from: &KeyBytes, ids: &[u64], status: Status, updated: &mut Vec<u64>) {
        for id in ids {
            let Some(m) = self.messages.get_mut(id) else { continue };
            if !m.outgoing || m.peer != *from {
                continue;
            }
            if m.status < status {
                m.status = status;
                updated.push(*id);
            }
            self.sealed.remove(id);
        }
    }

    /// Marca como lidas as mensagens recebidas de `peer` e prepara as
    /// confirmações de leitura. Devolve quantas foram marcadas.
    pub fn mark_read(&mut self, peer: &KeyBytes) -> usize {
        let ids: Vec<u64> = self.messages.values_mut().filter(|m| !m.outgoing && m.peer == *peer && !m.read).map(|m| {
            m.read = true;
            m.id
        }).collect();
        for chunk in ids.chunks(MAX_ACKS) {
            self.queue_ack(peer, Body::Read(chunk.to_vec()));
        }
        ids.len()
    }

    /// Mensagens de uma conversa, da mais antiga para a mais nova.
    pub fn messages(&self, peer: &KeyBytes) -> Vec<ChatMessage> {
        let mut v: Vec<ChatMessage> = self.messages.values().filter(|m| m.peer == *peer).cloned().collect();
        v.sort_by_key(|m| (m.at_ms, m.id));
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
                unread,
            })
            .collect();
        v.sort_by_key(|c| std::cmp::Reverse(c.last_at_ms));
        v
    }

    // ---------- salvar e restaurar ----------

    pub fn save(&self) -> Vec<u8> {
        let mut w = Writer::default();
        w.raw(SAVE_MAGIC);
        w.raw(&self.me.secret_bytes());
        w.bytes(self.name.as_bytes());
        w.u32(self.contacts.len() as u32);
        for c in self.contacts.values() {
            w.raw(&c.key);
            w.bytes(c.name.as_bytes());
        }
        w.u32(self.messages.len() as u32);
        for m in self.messages.values() {
            w.u64(m.id);
            w.raw(&m.peer);
            w.u8(m.outgoing as u8 | (m.read as u8) << 1);
            w.u8(m.status as u8);
            w.u64(m.at_ms);
            w.bytes(m.text.as_bytes());
        }
        w.u32(self.sealed.len() as u32);
        for (id, env) in &self.sealed {
            w.u64(*id);
            w.bytes(env);
        }
        w.u32(self.acks.len() as u32);
        for a in &self.acks {
            w.bytes(a);
        }
        w.0
    }

    pub fn load(data: &[u8]) -> Result<Chat, ChatError> {
        Self::read(data).ok_or(ChatError::InvalidData)
    }

    fn read(data: &[u8]) -> Option<Chat> {
        let mut r = Reader::new(data);
        if r.take(8)? != SAVE_MAGIC {
            return None;
        }
        let me = Identity::from_secret(r.key()?);
        let name = r.string()?;
        let mut chat = Chat::with_identity(me, &name);
        for _ in 0..r.u32()? {
            let key = r.key()?;
            chat.contacts.insert(key, Contact { key, name: r.string()? });
        }
        for _ in 0..r.u32()? {
            let id = r.u64()?;
            let peer = r.key()?;
            let flags = r.u8()?;
            let status = Status::from_u8(r.u8()?)?;
            let at_ms = r.u64()?;
            let text = r.string()?;
            chat.messages.insert(id, ChatMessage { id, peer, outgoing: flags & 1 != 0, text, at_ms, status, read: flags & 2 != 0 });
        }
        for _ in 0..r.u32()? {
            let id = r.u64()?;
            chat.sealed.insert(id, r.bytes()?.to_vec());
        }
        for _ in 0..r.u32()? {
            chat.acks.push(r.bytes()?.to_vec());
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
        from.outbox()
            .into_iter()
            .map(|o| {
                if let Some(id) = o.message_id {
                    from.mark_sent(id);
                }
                to.receive(&o.data, now)
            })
            .collect()
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
        // Nova conexão: A reenvia, B ignora a cópia mas confirma de novo.
        let again = deliver(&mut a, &mut b, 3);
        assert!(again[0].message.is_none());
        assert_eq!(b.messages(a.my_key()).len(), 1);
        deliver(&mut b, &mut a, 4);
        assert_eq!(a.messages(&bk)[0].status, Status::Delivered);
    }

    #[test]
    fn strangers_and_forged_acks_are_ignored() {
        let (mut a, b) = pair();
        let mut eve = Chat::new("Eve");
        eve.add_contact(&a.my_card()).unwrap();
        let bk = *b.my_key();
        // Eve conhece A, mas A não conhece Eve: A não aceita.
        eve.send_text(a.my_key(), "clique aqui", 1).unwrap();
        for o in eve.outbox() {
            assert_eq!(a.receive(&o.data, 2), Received::default());
        }
        // Mensagem de A para B passando por Eve: Eve não abre.
        a.send_text(&bk, "segredo", 3).unwrap();
        for o in a.outbox() {
            assert!(eve.receive(&o.data, 4).not_for_me);
        }
        // Erros de uso.
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
        deliver(&mut a, &mut c, 30);
        assert_eq!(b.conversations()[0].name, "Ana");
    }

    #[test]
    fn save_and_load_keeps_everything() {
        let (mut a, mut b) = pair();
        let bk = *b.my_key();
        a.send_text(&bk, "salva aí ✂️", 1).unwrap();
        deliver(&mut a, &mut b, 2);
        let a2 = Chat::load(&a.save()).unwrap();
        assert_eq!(a2.my_key(), a.my_key());
        assert_eq!(a2.my_name(), "Ana");
        assert_eq!(a2.messages(&bk), a.messages(&bk));
        assert_eq!(a2.contacts(), a.contacts());
        // A restaurada continua reenviando a pendente e recebendo a confirmação.
        let mut a2 = a2;
        let b_saved = Chat::load(&b.save()).unwrap();
        let mut b2 = b_saved;
        deliver(&mut b2, &mut a2, 3);
        assert_eq!(a2.messages(&bk)[0].status, Status::Delivered);
        assert!(Chat::load(b"lixo").is_err());
        let mut cut = a.save();
        cut.pop();
        assert!(Chat::load(&cut).is_err());
    }
}
