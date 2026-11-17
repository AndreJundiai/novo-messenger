//! Ponte WebAssembly do núcleo, sem dependências (sem wasm-bindgen).
//!
//! Convenção: entradas de texto/bytes chegam por ponteiros alocados com
//! `nm_alloc`; saídas ficam num buffer interno lido com `nm_out_ptr` (bytes)
//! ou `nm_out_f32_ptr` (áudio), e a função devolve o tamanho. O JavaScript
//! copia a saída antes da próxima chamada. A cola fica em `web/nm.js`.

use nm_core::fsk::{synth, FskConfig};
use nm_core::session::Receiver;
use nm_core::{mesh, tx_bits, Payload, TEST_TEXT};
use std::cell::RefCell;

thread_local! {
    static RX: RefCell<Vec<Option<Receiver>>> = const { RefCell::new(Vec::new()) };
    static OUT: RefCell<Vec<u8>> = const { RefCell::new(Vec::new()) };
    static OUT_F32: RefCell<Vec<f32>> = const { RefCell::new(Vec::new()) };
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

fn payload(kind: u32) -> Payload {
    match kind {
        0 => Payload::Toggle,
        1 => Payload::TestBytes,
        _ => Payload::Text,
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
pub unsafe extern "C" fn nm_tx_bits(kind: u32, ptr: *const u8, len: usize, bit_ms: f64) -> usize {
    set_out(tx_bits(payload(kind), text(ptr, len), bit_ms))
}

/// Áudio PCM pronto para tocar. Devolve o número de amostras (em `nm_out_f32_ptr`).
///
/// # Safety
/// Igual a `nm_tx_bits`.
#[no_mangle]
pub unsafe extern "C" fn nm_tx_synth(kind: u32, ptr: *const u8, len: usize, sample_rate: f32, f0: f32, f1: f32, bit_ms: f64, amplitude: f32) -> usize {
    let cfg = FskConfig::new(sample_rate, f0, f1, bit_ms);
    let pcm = synth(&tx_bits(payload(kind), text(ptr, len), bit_ms), &cfg, 300.0, amplitude.clamp(0.0, 1.0));
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
