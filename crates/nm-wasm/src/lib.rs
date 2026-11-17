//! Ponte WebAssembly do núcleo, sem dependências (sem wasm-bindgen).
//!
//! Convenção: entradas de texto/bytes chegam por ponteiros alocados com
//! `nm_alloc`; saídas ficam num buffer interno lido com `nm_out_ptr` (bytes)
//! ou `nm_out_f32_ptr` (áudio), e a função devolve o tamanho. O JavaScript
//! copia a saída antes da próxima chamada. A cola fica em `web/nm.js`.

use nm_core::fsk::{synth, FskConfig};
use nm_core::message::Message;
use nm_core::session::Receiver;
use nm_core::store::Store;
use nm_core::{b64, mesh, parse_qr_text, qr, qr_text, test_message, tx_bits, Payload, TEST_TEXT};
use std::cell::RefCell;

thread_local! {
    static RX: RefCell<Vec<Option<Receiver>>> = const { RefCell::new(Vec::new()) };
    static OUT: RefCell<Vec<u8>> = const { RefCell::new(Vec::new()) };
    static OUT_F32: RefCell<Vec<f32>> = const { RefCell::new(Vec::new()) };
    static STORE: RefCell<Store> = RefCell::new(Store::new());
}

fn set_out_str(s: String) -> usize {
    set_out(s.into_bytes())
}

fn msg_json(m: &Message) -> String {
    let mut o = String::new();
    m.write_json(&mut o);
    o
}

fn set_out(bytes: Vec<u8>) -> usize {
    let n = bytes.len();
    OUT.with(|o| *o.borrow_mut() = bytes);
    n
}

/// # Safety
/// `ptr..ptr+len` precisa ser memória válida desta instância (de `nm_alloc`).
unsafe fn bytes<'a>(ptr: *const u8, len: usize) -> &'a [u8] {
    if len == 0 {
        &[]
    } else {
        std::slice::from_raw_parts(ptr, len)
    }
}

unsafe fn text<'a>(ptr: *const u8, len: usize) -> &'a str {
    std::str::from_utf8(bytes(ptr, len)).unwrap_or("")
}

/// kind: 0 = fase 1 (1010…), 1 = fase 2 (bytes de teste),
/// 2 = mensagem de teste (fase 3), 3 = mensagem (id, lamport, ttl, texto).
fn with_payload<R>(kind: u32, id: u32, lamport: u32, ttl: u32, text: &str, f: impl FnOnce(Payload) -> R) -> R {
    match kind {
        0 => f(Payload::Toggle),
        1 => f(Payload::TestBytes),
        2 => f(Payload::Message(&test_message())),
        _ => f(Payload::Message(&Message::new(id, lamport as u16, ttl as u8, text))),
    }
}

fn with_rx<R>(h: u32, f: impl FnOnce(&mut Receiver) -> R) -> Option<R> {
    RX.with(|rx| rx.borrow_mut().get_mut(h.checked_sub(1)? as usize)?.as_mut().map(f))
}

fn add_rx(r: Receiver) -> u32 {
    RX.with(|rx| {
        let mut v = rx.borrow_mut();
        let idx = match v.iter().position(Option::is_none) {
            Some(i) => {
                v[i] = Some(r);
                i
            }
            None => {
                v.push(Some(r));
                v.len() - 1
            }
        };
        idx as u32 + 1
    })
}

// ---------- memória ----------

#[no_mangle]
pub extern "C" fn nm_alloc(len: usize) -> *mut u8 {
    let mut v = Vec::<u8>::with_capacity(len.max(1));
    let p = v.as_mut_ptr();
    std::mem::forget(v);
    p
}

/// # Safety
/// `ptr` e `len` precisam vir de uma chamada a `nm_alloc` com o mesmo `len`.
#[no_mangle]
pub unsafe extern "C" fn nm_free(ptr: *mut u8, len: usize) {
    drop(Vec::from_raw_parts(ptr, 0, len.max(1)));
}

#[no_mangle]
pub extern "C" fn nm_out_ptr() -> *const u8 {
    OUT.with(|o| o.borrow().as_ptr())
}

#[no_mangle]
pub extern "C" fn nm_out_f32_ptr() -> *const f32 {
    OUT_F32.with(|o| o.borrow().as_ptr())
}

#[no_mangle]
pub extern "C" fn nm_test_text() -> usize {
    set_out(TEST_TEXT.as_bytes().to_vec())
}

// ---------- recepção ----------

#[no_mangle]
pub extern "C" fn nm_rx_light(bit_ms: f64) -> u32 {
    add_rx(Receiver::light(bit_ms))
}

#[no_mangle]
pub extern "C" fn nm_rx_sound(sample_rate: f32, f0: f32, f1: f32, bit_ms: f64) -> u32 {
    add_rx(Receiver::sound(FskConfig::new(sample_rate, f0, f1, bit_ms)))
}

#[no_mangle]
pub extern "C" fn nm_rx_free(h: u32) {
    RX.with(|rx| {
        if let Some(slot) = h.checked_sub(1).and_then(|i| rx.borrow_mut().get_mut(i as usize).map(|s| s.take())) {
            drop(slot);
        }
    });
}

#[no_mangle]
pub extern "C" fn nm_rx_muted(h: u32, muted: u32) {
    with_rx(h, |r| r.set_muted(muted != 0));
}

#[no_mangle]
pub extern "C" fn nm_rx_luma(h: u32, t_ms: f64, luma: f32) {
    with_rx(h, |r| r.push_luma(t_ms, luma));
}

/// # Safety
/// `ptr` aponta para `len` floats válidos (de `nm_alloc`).
#[no_mangle]
pub unsafe extern "C" fn nm_rx_pcm(h: u32, ptr: *const f32, len: usize) {
    let pcm = if len == 0 { &[][..] } else { std::slice::from_raw_parts(ptr, len) };
    with_rx(h, |r| r.push_pcm(pcm));
}

/// Eventos e pontos do gráfico em JSON (ver `Receiver::drain_json`).
#[no_mangle]
pub extern "C" fn nm_rx_drain(h: u32) -> usize {
    set_out(with_rx(h, |r| r.drain_json()).unwrap_or_default().into_bytes())
}

// ---------- transmissão ----------

/// Bits (um byte 0/1 por bit) para luz.
///
/// # Safety
/// `ptr..ptr+len` é texto UTF-8 em memória de `nm_alloc`.
#[no_mangle]
pub unsafe extern "C" fn nm_tx_bits(kind: u32, id: u32, lamport: u32, ttl: u32, ptr: *const u8, len: usize, bit_ms: f64) -> usize {
    set_out(with_payload(kind, id, lamport, ttl, text(ptr, len), |p| tx_bits(p, bit_ms)))
}

/// Áudio PCM pronto para tocar. Devolve o número de amostras (em `nm_out_f32_ptr`).
///
/// # Safety
/// Igual a `nm_tx_bits`.
#[no_mangle]
#[allow(clippy::too_many_arguments)]
pub unsafe extern "C" fn nm_tx_synth(
    kind: u32,
    id: u32,
    lamport: u32,
    ttl: u32,
    ptr: *const u8,
    len: usize,
    sample_rate: f32,
    f0: f32,
    f1: f32,
    bit_ms: f64,
    amplitude: f32,
) -> usize {
    let cfg = FskConfig::new(sample_rate, f0, f1, bit_ms);
    let bits = with_payload(kind, id, lamport, ttl, text(ptr, len), |p| tx_bits(p, bit_ms));
    let pcm = synth(&bits, &cfg, 300.0, amplitude.clamp(0.0, 1.0));
    let n = pcm.len();
    OUT_F32.with(|o| *o.borrow_mut() = pcm);
    n
}

// ---------- LoRa / Meshtastic ----------

/// # Safety
/// Igual a `nm_tx_bits`.
#[no_mangle]
pub unsafe extern "C" fn nm_mesh_text(ptr: *const u8, len: usize, packet_id: u32) -> usize {
    set_out(mesh::encode_text(text(ptr, len), packet_id))
}

#[no_mangle]
pub extern "C" fn nm_mesh_want_config(id: u32) -> usize {
    set_out(mesh::want_config(id))
}

/// JSON `{"from":123,"text":"…"}` ou tamanho 0 se não for mensagem de texto.
///
/// # Safety
/// `ptr..ptr+len` em memória de `nm_alloc`.
#[no_mangle]
pub unsafe extern "C" fn nm_mesh_decode(ptr: *const u8, len: usize) -> usize {
    let Some(m) = mesh::decode_from_radio(bytes(ptr, len)) else { return set_out(Vec::new()) };
    let mut o = format!("{{\"from\":{},\"text\":", m.from);
    nm_core::json::string(&mut o, &m.text);
    o.push('}');
    set_out(o.into_bytes())
}

/// UUIDs Bluetooth do Meshtastic, em JSON.
#[no_mangle]
pub extern "C" fn nm_mesh_uuids() -> usize {
    set_out(
        format!(
            "{{\"service\":\"{}\",\"to\":\"{}\",\"from\":\"{}\",\"num\":\"{}\"}}",
            mesh::SERVICE_UUID,
            mesh::TO_RADIO_UUID,
            mesh::FROM_RADIO_UUID,
            mesh::FROM_NUM_UUID
        )
        .into_bytes(),
    )
}

// ---------- QR Code ----------

/// QR da mensagem (kind 2 = teste, 3 = mensagem). Saída: `[tamanho, módulos 0/1 linha a linha…]`.
///
/// # Safety
/// Igual a `nm_tx_bits`.
#[no_mangle]
pub unsafe extern "C" fn nm_qr(kind: u32, id: u32, lamport: u32, ttl: u32, ptr: *const u8, len: usize) -> usize {
    let msg = if kind == 2 { test_message() } else { Message::new(id, lamport as u16, ttl as u8, text(ptr, len)) };
    let Some(q) = qr::encode(qr_text(&msg).as_bytes()) else { return set_out(Vec::new()) };
    let mut out = Vec::with_capacity(1 + q.size * q.size);
    out.push(q.size as u8);
    for y in 0..q.size {
        for x in 0..q.size {
            out.push(q.get(x, y) as u8);
        }
    }
    set_out(out)
}

/// Lê o texto de um QR. JSON `{"id":…,…,"text":"…","fixed":n,"test":bool}` ou tamanho 0.
///
/// # Safety
/// Igual a `nm_tx_bits`.
#[no_mangle]
pub unsafe extern "C" fn nm_qr_parse(ptr: *const u8, len: usize) -> usize {
    let Some((m, fixed)) = parse_qr_text(text(ptr, len)) else { return set_out(Vec::new()) };
    let mut o = msg_json(&m);
    o.pop();
    o.push_str(&format!(",\"fixed\":{fixed},\"test\":{}}}", m.id == 0 && m.text == TEST_TEXT));
    set_out_str(o)
}

// ---------- histórico (CRDT) ----------

/// Cria e guarda uma mensagem deste aparelho. Devolve o JSON da mensagem.
///
/// # Safety
/// Igual a `nm_tx_bits`.
#[no_mangle]
pub unsafe extern "C" fn nm_store_compose(ptr: *const u8, len: usize, ttl: u32, id: u32, now: f64) -> usize {
    let m = STORE.with(|s| s.borrow_mut().compose(text(ptr, len), ttl as u8, id, now));
    set_out_str(msg_json(&m))
}

/// Guarda uma mensagem recebida. Devolve 1 se for nova.
///
/// # Safety
/// Igual a `nm_tx_bits`.
#[no_mangle]
pub unsafe extern "C" fn nm_store_receive(id: u32, lamport: u32, ttl: u32, ptr: *const u8, len: usize, via: u32, now: f64) -> u32 {
    let m = Message::new(id, lamport as u16, ttl as u8, text(ptr, len));
    STORE.with(|s| s.borrow_mut().receive(m, via as u8, now)) as u32
}

/// Guarda texto puro (LoRa). JSON da mensagem criada, ou tamanho 0 se for cópia.
///
/// # Safety
/// Igual a `nm_tx_bits`.
#[no_mangle]
pub unsafe extern "C" fn nm_store_receive_text(ptr: *const u8, len: usize, via: u32, id: u32, now: f64) -> usize {
    match STORE.with(|s| s.borrow_mut().receive_text(text(ptr, len), via as u8, id, now)) {
        Some(m) => set_out_str(msg_json(&m)),
        None => set_out(Vec::new()),
    }
}

#[no_mangle]
pub extern "C" fn nm_store_list() -> usize {
    set_out_str(STORE.with(|s| s.borrow().to_json()))
}

/// Estado para salvar no aparelho, em Base64.
#[no_mangle]
pub extern "C" fn nm_store_save() -> usize {
    set_out_str(b64::encode(&STORE.with(|s| s.borrow().save())))
}

/// Restaura o estado salvo (Base64). Devolve 1 se deu certo.
///
/// # Safety
/// Igual a `nm_tx_bits`.
#[no_mangle]
pub unsafe extern "C" fn nm_store_load(ptr: *const u8, len: usize) -> u32 {
    match b64::decode(text(ptr, len)).and_then(|b| Store::load(&b)) {
        Some(loaded) => {
            STORE.with(|s| *s.borrow_mut() = loaded);
            1
        }
        None => 0,
    }
}

/// Arquivo da conversa para levar a outro aparelho.
#[no_mangle]
pub extern "C" fn nm_store_export() -> usize {
    set_out(STORE.with(|s| s.borrow().export()))
}

/// Junta um arquivo exportado. Devolve quantas mensagens são novas, ou -1 se o arquivo for inválido.
///
/// # Safety
/// `ptr..ptr+len` em memória de `nm_alloc`.
#[no_mangle]
pub unsafe extern "C" fn nm_store_import(ptr: *const u8, len: usize, now: f64) -> i32 {
    STORE.with(|s| s.borrow_mut().import(bytes(ptr, len), now)).map_or(-1, |n| n as i32)
}

#[no_mangle]
pub extern "C" fn nm_store_clear() {
    STORE.with(|s| s.borrow_mut().clear());
}
