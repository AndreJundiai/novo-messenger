'use strict';
/*
 * Cola entre o app e o núcleo Rust (nm_core.wasm). Sem dependências.
 * O módulo não importa nada do navegador: só memória e funções puras.
 */
async function loadNM(url = 'nm_core.wasm') {
  const imports = {};
  let inst;
  try {
    ({ instance: inst } = await WebAssembly.instantiateStreaming(fetch(url), imports));
  } catch (_) {
    const buf = await (await fetch(url)).arrayBuffer();
    ({ instance: inst } = await WebAssembly.instantiate(buf, imports));
  }
  const x = inst.exports;
  const te = new TextEncoder(), td = new TextDecoder();
  const mem = () => x.memory.buffer; // a memória pode crescer: sempre pegue a visão nova
  const out = (n) => new Uint8Array(mem(), x.nm_out_ptr(), n).slice();
  const outText = (n) => td.decode(out(n));
  // Copia bytes para a memória do wasm, chama f(ptr, len) e libera.
  const withBytes = (bytes, f) => {
    const p = x.nm_alloc(bytes.length);
    new Uint8Array(mem(), p, bytes.length).set(bytes);
    try { return f(p, bytes.length); } finally { x.nm_free(p, bytes.length); }
  };
  const withText = (s, f) => withBytes(te.encode(s), f);
  const KIND = { toggle: 0, bytes: 1, text: 2 };

  class Receiver {
    constructor(h) { this.h = h; this.cap = 0; this.ptr = 0; }
    pushLuma(tMs, luma) { x.nm_rx_luma(this.h, tMs, luma); }
    pushPcm(f32) {
      const bytes = f32.length * 4;
      if (bytes > this.cap) {
        if (this.ptr) x.nm_free(this.ptr, this.cap);
        this.cap = Math.max(bytes, 16384); this.ptr = x.nm_alloc(this.cap);
      }
      new Float32Array(mem(), this.ptr, f32.length).set(f32);
      x.nm_rx_pcm(this.h, this.ptr, f32.length);
    }
    setMuted(m) { x.nm_rx_muted(this.h, m ? 1 : 0); }
    /** {st:{l,a,b,sig}, s:[[t,v,l]...], e:[{k,v}...]} */
    drain() { return JSON.parse(outText(x.nm_rx_drain(this.h))); }
    free() { if (this.ptr) x.nm_free(this.ptr, this.cap); x.nm_rx_free(this.h); this.h = 0; }
  }

  return {
    TEST_TEXT: outText(x.nm_test_text()),
    MESH: JSON.parse(outText(x.nm_mesh_uuids())),
    lightReceiver: (bitMs) => new Receiver(x.nm_rx_light(bitMs)),
    soundReceiver: (rate, f0, f1, bitMs) => new Receiver(x.nm_rx_sound(rate, f0, f1, bitMs)),
    txBits: (kind, text, bitMs) => withText(text, (p, n) => out(x.nm_tx_bits(KIND[kind], p, n, bitMs))),
    txSynth: (kind, text, rate, f0, f1, bitMs, amp = 0.8) => withText(text, (p, n) => {
      const len = x.nm_tx_synth(KIND[kind], p, n, rate, f0, f1, bitMs, amp);
      return new Float32Array(mem(), x.nm_out_f32_ptr(), len).slice();
    }),
    meshText: (text, id) => withText(text, (p, n) => out(x.nm_mesh_text(p, n, id))),
    meshWantConfig: (id) => out(x.nm_mesh_want_config(id)),
    meshDecode: (u8) => withBytes(u8, (p, n) => { const len = x.nm_mesh_decode(p, n); return len ? JSON.parse(outText(len)) : null; }),
  };
}
