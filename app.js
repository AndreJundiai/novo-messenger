'use strict';
/*
 * Novo Messenger — mensagens offline por luz, som e LoRa (Meshtastic).
 *
 * Luz e som usam o mesmo protocolo, igual a uma porta serial (UART):
 *   linha parada = 0; cada byte = bit de início (1) + 8 bits (LSB primeiro) + 2 bits de parada (0).
 *   Quadro: STX(0x02) + texto UTF-8 + checksum + ETX(0x03).
 *   O checksum fica sempre em 0x80..0xFF para nunca se confundir com STX/ETX.
 *
 * Fases de teste:
 *   1 Sinal      — o emissor pisca/apita 1010… e o receptor vê as trocas no ritmo certo
 *   2 Bytes      — o emissor manda bytes fixos sem quadro; o receptor confere byte a byte
 *   3 Mensagem   — o emissor manda um quadro de teste; o receptor confere o checksum
 *   4 Conversa   — mensagem livre de verdade
 */

const $ = (id) => document.getElementById(id);
const enc = new TextEncoder();
const dec = new TextDecoder();
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

const TEST_BYTES = [0x55, 0xaa, 0x0f, 0xf0, 0x41, 0x42, 0x43]; // padrões alternados + "ABC"
const TEST_TEXT = 'TESTE-OK 123';
const hex = (b) => b.toString(16).padStart(2, '0').toUpperCase();

/* ---------------- Quadro (frame) ---------------- */
function encodeFrame(text) {
  const clean = text.replace(/[\u0000-\u001f\u007f]/g, ' ').trim();
  const data = Array.from(enc.encode(clean));
  let x = 0;
  for (const b of data) x ^= b;
  return [0x02, ...data, 0x80 | (x & 0x7f), 0x03];
}

class FrameDecoder {
  constructor(onText, onBad = () => {}) { this.onText = onText; this.onBad = onBad; this.buf = null; }
  push(b) {
    if (b === 0x02) { this.buf = []; return; }
    if (!this.buf) return;
    if (b === 0x03) {
      const buf = this.buf; this.buf = null;
      if (buf.length < 2) return;
      const cs = buf.pop();
      let x = 0;
      for (const v of buf) x ^= v;
      if ((0x80 | (x & 0x7f)) === cs) {
        try { this.onText(dec.decode(new Uint8Array(buf))); } catch (_) { this.onBad('texto inválido'); }
      } else this.onBad('checksum errado');
      return;
    }
    this.buf.push(b);
    if (this.buf.length > 400) { this.buf = null; this.onBad('quadro longo demais'); }
  }
}

function bytesToBits(bytes) {
  const bits = [];
  for (let i = 0; i < 8; i++) bits.push(1, 0); // preâmbulo: calibra o receptor
  bits.push(0, 0, 0, 0);
  for (const byte of bytes) {
    bits.push(1);
    for (let i = 0; i < 8; i++) bits.push((byte >> i) & 1);
    bits.push(0, 0);
  }
  bits.push(0, 0, 0, 0);
  return bits;
}
const frameToBits = (frame) => bytesToBits(frame);
function toggleBits(ms, T) { // fase 1: 1010… por ~ms milissegundos
  const n = Math.max(8, Math.round(ms / T));
  const bits = [];
  for (let i = 0; i < n; i++) bits.push(i % 2 ? 0 : 1);
  bits.push(0);
  return bits;
}

/* Receptor UART genérico: recebe amostras (tempo, nível 0/1) e devolve bytes */
class Uart {
  constructor(T, onByte) { this.T = T; this.onByte = onByte; this.state = 'idle'; this.prev = 0; this.prevT = null; }
  push(t, l) {
    const T = this.T;
    if (this.state === 'idle') {
      if (l === 1 && this.prev === 0) {
        this.edge = this.prevT != null ? (this.prevT + t) / 2 : t;
        this.state = 'data'; this.bit = 0; this.byte = 0;
      }
    } else if (this.state === 'data') {
      while (this.bit < 8 && t >= this.edge + (this.bit + 1.5) * T) {
        this.byte |= l << this.bit; this.bit++;
      }
      if (this.bit === 8) this.state = 'stop';
    }
    if (this.state === 'stop' && t >= this.edge + 9.5 * T) {
      if (l === 0) this.onByte(this.byte);
      this.state = 'idle';
    }
    this.prev = l; this.prevT = t;
  }
}

/* Fase 1: detecta um sinal 1010… no ritmo T (trocas de nível espaçadas ~T) */
class ToggleDetector {
  constructor(T, onDetect) { this.T = T; this.onDetect = onDetect; this.prev = 0; this.run = 0; this.lastFlip = null; }
  push(t, l) {
    if (l !== this.prev) {
      if (this.lastFlip != null) {
        const dt = t - this.lastFlip;
        if (dt > this.T * 0.55 && dt < this.T * 1.6) this.run++; else this.run = 0;
        if (this.run >= 10) { this.onDetect(); this.run = 0; }
      }
      this.lastFlip = t;
    }
    this.prev = l;
  }
}

/* ---------------- Chat ---------------- */
const CH_NAME = { luz: 'Luz', som: 'Som', lora: 'LoRa' };
const seen = new Map(); // texto -> horário, para ignorar cópias que chegam por vários canais
let history = [];
try { history = JSON.parse(localStorage.getItem('nm-history') || '[]'); } catch (_) {}

function render() {
  const chat = $('chat');
  chat.querySelectorAll('.msg').forEach((n) => n.remove());
  $('empty').hidden = history.length > 0;
  for (const m of history) {
    const d = document.createElement('div');
    d.className = 'msg' + (m.me ? ' me' : '');
    d.textContent = m.text;
    const meta = document.createElement('span');
    meta.className = 'meta';
    meta.textContent = `${m.me ? 'você' : 'recebida'} · ${m.via.join(' + ')} · ${new Date(m.at).toLocaleTimeString('pt-BR', { hour: '2-digit', minute: '2-digit' })}`;
    d.appendChild(meta);
    chat.appendChild(d);
  }
  chat.scrollTop = chat.scrollHeight;
}
function save() {
  history = history.slice(-200);
  try { localStorage.setItem('nm-history', JSON.stringify(history)); } catch (_) {}
}
function addMessage(text, via, me) {
  if (!me) {
    const last = seen.get(text);
    if (last && Date.now() - last.at < 60000) {
      if (!last.msg.via.includes(via)) { last.msg.via.push(via); save(); render(); }
      return;
    }
    const msg = { text, via: [via], me: false, at: Date.now() };
    seen.set(text, { at: Date.now(), msg });
    history.push(msg);
    if (navigator.vibrate) navigator.vibrate(120);
  } else {
    history.push({ text, via, me: true, at: Date.now() });
  }
  save(); render();
}

/* ---------------- Registro ---------------- */
function log(ch, text, kind = '') {
  const box = $('log');
  const d = document.createElement('div');
  d.className = kind || ch || '';
  const t = new Date().toLocaleTimeString('pt-BR');
  d.textContent = `${t} ${ch ? '[' + CH_NAME[ch] + '] ' : ''}${text}`;
  box.appendChild(d);
  while (box.children.length > 200) box.firstChild.remove();
  box.scrollTop = box.scrollHeight;
}

/* ---------------- Fases ---------------- */
const PHASES = [
  { title: 'Sinal', desc: 'O emissor pisca ou apita 1-0-1-0 por 3 s. O receptor deve ver ondas regulares no gráfico.', btn: 'Enviar sinal de teste' },
  { title: 'Bits e bytes', desc: 'Envia 7 bytes fixos (55 AA 0F F0 41 42 43). O receptor confere byte a byte no registro.', btn: 'Enviar bytes de teste' },
  { title: 'Mensagem de teste', desc: `Envia o quadro "${TEST_TEXT}" com checksum. Confere se a mensagem inteira chega sem erro.`, btn: 'Enviar mensagem de teste' },
  { title: 'Conversa', desc: 'Escreva uma mensagem de verdade lá embaixo e envie. Passa quando chegar ao outro aparelho.', btn: null },
];
let phaseState;
function resetPhases() {
  phaseState = { luz: ['', '', '', ''], som: ['', '', '', ''], lora: ['', 'na', '', ''] };
  if (mesh) phaseState.lora[0] = 'ok';
  renderPhases();
}
function markPhase(ch, i) {
  if (phaseState[ch][i] === 'ok') return;
  phaseState[ch][i] = 'ok';
  log(ch, `Fase ${i + 1} (${PHASES[i].title}) passou ✓`, 'ok');
  if (navigator.vibrate) navigator.vibrate([60, 40, 60]);
  renderPhases();
}
function renderPhases() {
  const list = $('phaseList');
  list.textContent = '';
  PHASES.forEach((p, i) => {
    const done = ['luz', 'som', 'lora'].some((c) => phaseState[c][i] === 'ok');
    const el = document.createElement('div');
    el.className = 'phase' + (done ? ' done' : '');
    el.innerHTML = `<span class="num">${i + 1}</span><b></b><span class="d"></span><div class="chips"></div>`;
    el.querySelector('b').textContent = p.title;
    el.querySelector('.d').textContent = p.desc;
    const chips = el.querySelector('.chips');
    for (const c of ['luz', 'som', 'lora']) {
      const s = phaseState[c][i];
      const chip = document.createElement('span');
      chip.className = 'chip ' + s;
      chip.textContent = CH_NAME[c] + (s === 'ok' ? ' ✓' : s === 'na' ? '' : ' …');
      chips.appendChild(chip);
    }
    if (p.btn) {
      const b = document.createElement('button');
      b.type = 'button'; b.textContent = p.btn;
      b.onclick = () => runTest(i, b);
      chips.appendChild(b);
    }
    list.appendChild(el);
  });
}
$('phReset').onclick = resetPhases;

/* Cria a cadeia de recepção de um canal (luz ou som) */
function makeReceiver(ch, T) {
  let raw = [];
  let lineBytes = [], lineTimer = null;
  const flush = () => { if (lineBytes.length) log(ch, 'bytes: ' + lineBytes.map(hex).join(' ')); lineBytes = []; };
  const fd = new FrameDecoder(
    (txt) => {
      if (txt === TEST_TEXT) { log(ch, `mensagem de teste recebida: "${txt}"`, 'ok'); markPhase(ch, 2); }
      else { log(ch, `mensagem recebida: "${txt}"`, 'ok'); addMessage(txt, CH_NAME[ch], false); markPhase(ch, 3); }
    },
    (why) => log(ch, 'quadro descartado: ' + why, 'err'),
  );
  const uart = new Uart(T, (b) => {
    fd.push(b);
    raw.push(b); if (raw.length > 64) raw = raw.slice(-64);
    lineBytes.push(b); clearTimeout(lineTimer); lineTimer = setTimeout(flush, T * 30);
    const tail = raw.slice(-TEST_BYTES.length);
    if (tail.length === TEST_BYTES.length && tail.every((v, i) => v === TEST_BYTES[i])) markPhase(ch, 1);
  });
  const tog = new ToggleDetector(T, () => markPhase(ch, 0));
  return { push(t, l) { uart.push(t, l); tog.push(t, l); } };
}

/* Gráfico do sinal: linha = valor bruto, faixa = nível 0/1 decidido */
function makeScope(canvas) {
  const pts = [];
  const ctx = canvas.getContext('2d');
  let raf = 0;
  const draw = () => {
    raf = requestAnimationFrame(draw);
    const dpr = window.devicePixelRatio || 1;
    const w = canvas.clientWidth, h = canvas.clientHeight;
    if (canvas.width !== w * dpr) { canvas.width = w * dpr; canvas.height = h * dpr; }
    ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
    ctx.clearRect(0, 0, w, h);
    const now = performance.now(), span = 4000;
    while (pts.length && now - pts[0][0] > span) pts.shift();
    const cs = getComputedStyle(document.documentElement);
    ctx.fillStyle = cs.getPropertyValue('--ok').trim() + '33';
    for (let i = 0; i < pts.length - 1; i++) {
      if (pts[i][2]) { const x0 = w - (now - pts[i][0]) / span * w, x1 = w - (now - pts[i + 1][0]) / span * w; ctx.fillRect(x0, 0, x1 - x0 + 0.5, h); }
    }
    ctx.strokeStyle = cs.getPropertyValue('--fg').trim();
    ctx.lineWidth = 1.5; ctx.beginPath();
    pts.forEach(([t, v], i) => { const x = w - (now - t) / span * w, y = h - 4 - v * (h - 8); i ? ctx.lineTo(x, y) : ctx.moveTo(x, y); });
    ctx.stroke();
  };
  return {
    push(t, v, l) { pts.push([t, Math.max(0, Math.min(1, v)), l]); },
    start() { canvas.hidden = false; cancelAnimationFrame(raf); draw(); },
    stop() { cancelAnimationFrame(raf); canvas.hidden = true; pts.length = 0; },
  };
}
const luzScope = makeScope($('luzScope'));
const somScope = makeScope($('somScope'));

let transmitting = false; // não escuta a si mesmo enquanto envia

/* ---------------- LUZ ---------------- */
let flashAbort = false;
$('flashStop').onclick = () => { flashAbort = true; };

async function sendLightBits(bits) {
  const T = +$('luzT').value || 150;
  const mode = $('luzMode').value;
  let setLevel, cleanup;
  if (mode === 'torch') {
    const stream = await navigator.mediaDevices.getUserMedia({ video: { facingMode: 'environment' } });
    const track = stream.getVideoTracks()[0];
    const caps = track.getCapabilities ? track.getCapabilities() : {};
    if (!caps.torch) { stream.getTracks().forEach((t) => t.stop()); throw new Error('lanterna não controlável neste aparelho; use "Tela piscando"'); }
    setLevel = (v) => track.applyConstraints({ advanced: [{ torch: !!v }] }).catch(() => {});
    cleanup = () => { setLevel(0); stream.getTracks().forEach((t) => t.stop()); };
  } else {
    const f = $('flash');
    f.hidden = false; f.classList.remove('on');
    try { await f.requestFullscreen?.(); } catch (_) {}
    setLevel = (v) => f.classList.toggle('on', !!v);
    cleanup = () => { f.hidden = true; f.classList.remove('on'); if (document.fullscreenElement) document.exitFullscreen().catch(() => {}); };
  }
  let wake = null;
  try { wake = await navigator.wakeLock?.request('screen'); } catch (_) {}
  flashAbort = false;
  log('luz', `enviando ${bits.length} bits (${(bits.length * T / 1000).toFixed(1)} s)`);
  await sleep(500);
  const t0 = performance.now();
  for (let i = 0; i < bits.length && !flashAbort; i++) {
    const wait = t0 + i * T - performance.now();
    if (wait > 0) await sleep(wait);
    setLevel(bits[i]);
    $('flashProg').style.width = ((i + 1) / bits.length * 100) + '%';
  }
  await sleep(T);
  cleanup();
  try { wake?.release(); } catch (_) {}
  if (flashAbort) throw new Error('envio cancelado');
  log('luz', 'envio concluído');
}

let luzOn = false, luzStream = null;
$('luzRx').onclick = async () => {
  if (luzOn) { stopLuz(); return; }
  try {
    luzStream = await navigator.mediaDevices.getUserMedia({ video: { facingMode: 'environment', frameRate: { ideal: 60 }, width: { ideal: 320 } } });
  } catch (e) { $('luzStatus').textContent = 'câmera negada: ' + e.message; return; }
  const v = $('cam'); v.srcObject = luzStream; await v.play();
  $('camWrap').hidden = false; luzOn = true; $('luzDot').classList.add('on'); $('luzRx').textContent = 'Parar câmera';
  const T = +$('luzT').value || 150;
  log('luz', `escutando pela câmera (bit ${T} ms)`);
  const rx = makeReceiver('luz', T);
  luzScope.start();
  const c = document.createElement('canvas'); c.width = c.height = 24;
  const ctx = c.getContext('2d', { willReadFrequently: true });
  const hist = [];
  const tick = () => {
    if (!luzOn) return;
    const t = performance.now();
    const w = v.videoWidth, h = v.videoHeight;
    if (w) {
      ctx.drawImage(v, w * 0.3, h * 0.3, w * 0.4, h * 0.4, 0, 0, 24, 24);
      const d = ctx.getImageData(0, 0, 24, 24).data;
      let s = 0;
      for (let i = 0; i < d.length; i += 4) s += 0.299 * d[i] + 0.587 * d[i + 1] + 0.114 * d[i + 2];
      const y = s / (d.length / 4);
      hist.push([t, y]);
      while (hist.length && t - hist[0][0] > 3000) hist.shift();
      let mn = 255, mx = 0;
      for (const [, val] of hist) { if (val < mn) mn = val; if (val > mx) mx = val; }
      const contrast = mx - mn;
      const level = (contrast > 20 && y > (mn + mx) / 2) ? 1 : 0;
      $('luzMeter').style.width = (y / 255 * 100) + '%';
      $('luzMeter').classList.toggle('hi', level === 1);
      $('luzStatus').textContent = `brilho ${y.toFixed(0)} · contraste ${contrast.toFixed(0)}${contrast > 20 ? ' ✓' : ' (aproxime da luz)'}`;
      luzScope.push(t, contrast > 5 ? (y - mn) / contrast : 0.5, level);
      if (!transmitting) rx.push(t, level);
    }
    if (v.requestVideoFrameCallback) v.requestVideoFrameCallback(tick); else requestAnimationFrame(tick);
  };
  tick();
};
function stopLuz() {
  luzOn = false; luzStream?.getTracks().forEach((t) => t.stop());
  $('camWrap').hidden = true; $('luzDot').classList.remove('on'); $('luzRx').textContent = 'Ouvir pela câmera';
  $('luzStatus').textContent = 'parado'; $('luzMeter').style.width = '0';
  luzScope.stop(); log('luz', 'câmera parada');
}

/* ---------------- SOM ---------------- */
let actx = null;
function audio() { if (!actx) actx = new (window.AudioContext || window.webkitAudioContext)(); if (actx.state === 'suspended') actx.resume(); return actx; }
const freqs = () => $('somF').value.split(',').map(Number);

async function sendSoundBits(bits) {
  const ctx = audio();
  const T = (+$('somT').value || 80) / 1000;
  const [f0, f1] = freqs();
  const osc = ctx.createOscillator(); const g = ctx.createGain();
  osc.type = 'sine'; osc.connect(g); g.connect(ctx.destination);
  const start = ctx.currentTime + 0.15;
  g.gain.setValueAtTime(0, ctx.currentTime);
  g.gain.linearRampToValueAtTime(0.8, start);
  osc.frequency.setValueAtTime(f0, ctx.currentTime);
  const lead = 0.3; // tom "0" antes, para o receptor estabilizar
  bits.forEach((b, i) => osc.frequency.setValueAtTime(b ? f1 : f0, start + lead + i * T));
  const end = start + lead + bits.length * T;
  g.gain.setValueAtTime(0.8, end);
  g.gain.linearRampToValueAtTime(0, end + 0.05);
  log('som', `enviando ${bits.length} bits (${(end - ctx.currentTime).toFixed(1)} s)`);
  osc.start(); osc.stop(end + 0.1);
  await new Promise((r) => { osc.onended = r; });
  log('som', 'envio concluído');
}

let somOn = false, somStream = null, somTimer = null;
$('somRx').onclick = async () => {
  if (somOn) { stopSom(); return; }
  const ctx = audio();
  try {
    somStream = await navigator.mediaDevices.getUserMedia({ audio: { echoCancellation: false, noiseSuppression: false, autoGainControl: false } });
  } catch (e) { $('somStatus').textContent = 'microfone negado: ' + e.message; return; }
  const src = ctx.createMediaStreamSource(somStream);
  const an = ctx.createAnalyser(); an.fftSize = 1024; an.smoothingTimeConstant = 0;
  src.connect(an);
  const buf = new Float32Array(an.frequencyBinCount);
  const T = +$('somT').value || 80;
  const [f0, f1] = freqs();
  const binHz = ctx.sampleRate / an.fftSize;
  const peak = (f) => { const k = Math.round(f / binHz); return Math.max(buf[k - 1], buf[k], buf[k + 1]); };
  const rx = makeReceiver('som', T);
  somOn = true; $('somDot').classList.add('on'); $('somRx').textContent = 'Parar microfone';
  log('som', `escutando pelo microfone (bit ${T} ms, ${f0}/${f1} Hz)`);
  somScope.start();
  somTimer = setInterval(() => {
    an.getFloatFrequencyData(buf);
    const p0 = peak(f0), p1 = peak(f1);
    const strong = Math.max(p0, p1) > -75;
    const level = strong && p1 - p0 > 8 ? 1 : 0;
    const pct = Math.max(0, Math.min(100, (Math.max(p0, p1) + 100) * 1.5));
    $('somMeter').style.width = pct + '%';
    $('somMeter').classList.toggle('hi', level === 1);
    $('somStatus').textContent = `tom 0: ${p0.toFixed(0)} dB · tom 1: ${p1.toFixed(0)} dB${strong ? ' ✓' : ''}`;
    const t = performance.now();
    somScope.push(t, strong ? 0.5 + (p1 - p0) / 60 : 0.5, level);
    if (!transmitting) rx.push(t, level);
  }, 8);
};
function stopSom() {
  somOn = false; clearInterval(somTimer); somStream?.getTracks().forEach((t) => t.stop());
  $('somDot').classList.remove('on'); $('somRx').textContent = 'Ouvir pelo microfone';
  $('somStatus').textContent = 'parado'; $('somMeter').style.width = '0';
  somScope.stop(); log('som', 'microfone parado');
}

/* ---------------- LoRa / Meshtastic via Web Bluetooth ---------------- */
const MESH_SVC = '6ba1b218-15a8-461f-9fa8-5dcae273eafd';
const TO_RADIO = 'f75c76d2-129e-4dad-a1dd-7866124401e7';
const FROM_RADIO = '2c55e69e-4993-11ed-b878-0242ac120002';
const FROM_NUM = 'ed9da18c-a800-4f66-a670-aa7547e34453';
let mesh = null;

// Protobuf mínimo (só o necessário para mensagens de texto)
function varint(n) { const o = []; n = n >>> 0; while (n > 127) { o.push((n & 127) | 128); n >>>= 7; } o.push(n); return o; }
function lenField(field, bytes) { return [(field << 3) | 2, ...varint(bytes.length), ...bytes]; }
function fixed32(field, n) { return [(field << 3) | 5, n & 255, (n >>> 8) & 255, (n >>> 16) & 255, (n >>> 24) & 255]; }
function pbParse(u8) {
  const out = {}; let i = 0;
  const rv = () => { let r = 0, s = 0, b; do { b = u8[i++]; r += (b & 127) * 2 ** s; s += 7; } while (b & 128 && i < u8.length); return r; };
  while (i < u8.length) {
    const key = rv(); const f = Math.floor(key / 8), w = key & 7; let v;
    if (w === 0) v = rv();
    else if (w === 1) { v = u8.subarray(i, i + 8); i += 8; }
    else if (w === 2) { const l = rv(); v = u8.subarray(i, i + l); i += l; }
    else if (w === 5) { v = u8[i] | (u8[i + 1] << 8) | (u8[i + 2] << 16) | (u8[i + 3] << 24); i += 4; }
    else break;
    (out[f] = out[f] || []).push(v);
  }
  return out;
}

async function meshDrain() {
  for (let n = 0; n < 500; n++) {
    const dv = await mesh.from.readValue();
    if (!dv.byteLength) break;
    try {
      const fr = pbParse(new Uint8Array(dv.buffer, dv.byteOffset, dv.byteLength));
      if (!fr[2]) continue;                          // FromRadio.packet
      const pkt = pbParse(fr[2][0]);
      if (!pkt[4]) continue;                         // MeshPacket.decoded
      const data = pbParse(pkt[4][0]);
      if (data[1] && data[1][0] === 1 && data[2]) {  // TEXT_MESSAGE_APP
        const txt = dec.decode(data[2][0]);
        const from = pkt[1] ? '!' + (pkt[1][0] >>> 0).toString(16) : '?';
        if (txt === TEST_TEXT) { log('lora', `mensagem de teste de ${from}`, 'ok'); markPhase('lora', 2); }
        else { log('lora', `mensagem de ${from}: "${txt}"`, 'ok'); addMessage(txt, 'LoRa', false); markPhase('lora', 3); }
      }
    } catch (_) {}
  }
}

$('loraConn').onclick = async () => {
  if (mesh) { try { mesh.dev.gatt.disconnect(); } catch (_) {} return; }
  if (!navigator.bluetooth) { $('loraStatus').textContent = 'este navegador não tem Web Bluetooth. Use o Chrome no Android ou no computador.'; return; }
  try {
    $('loraStatus').textContent = 'procurando rádio…';
    const dev = await navigator.bluetooth.requestDevice({ filters: [{ services: [MESH_SVC] }] });
    const srv = await (await dev.gatt.connect()).getPrimaryService(MESH_SVC);
    mesh = { dev, to: await srv.getCharacteristic(TO_RADIO), from: await srv.getCharacteristic(FROM_RADIO), num: await srv.getCharacteristic(FROM_NUM), busy: false };
    dev.addEventListener('gattserverdisconnected', () => {
      mesh = null; $('loraDot').classList.remove('on'); $('loraConn').textContent = 'Conectar rádio'; $('loraStatus').textContent = 'desconectado';
      log('lora', 'rádio desconectado', 'err');
    });
    // inicia a sessão (want_config_id) e descarta a configuração enviada pelo rádio
    await mesh.to.writeValue(new Uint8Array([0x18, ...varint(Math.floor(Math.random() * 1e9))]));
    await meshDrain();
    await mesh.num.startNotifications();
    mesh.num.addEventListener('characteristicvaluechanged', async () => {
      if (!mesh || mesh.busy) return; mesh.busy = true;
      try { await meshDrain(); } finally { if (mesh) mesh.busy = false; }
    });
    $('loraDot').classList.add('on'); $('loraConn').textContent = 'Desconectar';
    $('loraStatus').textContent = 'conectado a ' + (dev.name || 'rádio Meshtastic');
    $('viaLora').checked = true;
    log('lora', 'conectado a ' + (dev.name || 'rádio Meshtastic'));
    markPhase('lora', 0);
  } catch (e) { mesh = null; $('loraStatus').textContent = 'falhou: ' + e.message; log('lora', 'falha ao conectar: ' + e.message, 'err'); }
};

async function sendLora(text) {
  if (!mesh) throw new Error('rádio LoRa não conectado');
  const payload = Array.from(enc.encode(text)).slice(0, 220);
  const data = [0x08, 0x01, ...lenField(2, payload)];                 // Data{portnum=1, payload}
  const id = (Math.random() * 0xffffffff) >>> 0;
  const pkt = [...fixed32(2, 0xffffffff), ...lenField(4, data), ...fixed32(6, id)]; // MeshPacket{to=broadcast, decoded, id}
  await mesh.to.writeValue(new Uint8Array(lenField(1, pkt)));        // ToRadio{packet}
  log('lora', `enviado ao rádio: "${text}"`);
}

/* ---------------- Envio por todos os canais ---------------- */
function selectedChannels() {
  const out = [];
  if ($('viaLuz').checked) out.push('luz');
  if ($('viaSom').checked) out.push('som');
  if ($('viaLora').checked) out.push('lora');
  return out;
}
// kind: 'toggle' (fase 1), 'bytes' (fase 2), 'text' (fases 3 e 4)
async function transmit(kind, text) {
  const chans = selectedChannels();
  if (!chans.length) { log(null, 'marque pelo menos um canal em "Enviar por"', 'err'); return []; }
  const jobs = chans.map((ch) => {
    if (ch === 'lora') {
      if (kind !== 'text') { log('lora', 'esta fase não se aplica ao rádio (ele já confere os dados)'); return Promise.resolve(); }
      return sendLora(text);
    }
    const T = ch === 'luz' ? (+$('luzT').value || 150) : (+$('somT').value || 80);
    const bits = kind === 'toggle' ? toggleBits(3000, T) : kind === 'bytes' ? bytesToBits(TEST_BYTES) : frameToBits(encodeFrame(text));
    return ch === 'luz' ? sendLightBits(bits) : sendSoundBits(bits);
  });
  transmitting = true;
  const results = await Promise.allSettled(jobs);
  transmitting = false;
  const ok = [];
  results.forEach((r, i) => {
    if (r.status === 'rejected') log(chans[i], 'erro ao enviar: ' + (r.reason?.message || r.reason), 'err');
    else ok.push(CH_NAME[chans[i]]);
  });
  return ok;
}

let busy = false;
async function runTest(i, btn) {
  if (busy) return;
  busy = true; btn.disabled = true; $('send').disabled = true;
  log(null, `▶ Fase ${i + 1}: ${PHASES[i].title}`);
  try { await transmit(['toggle', 'bytes', 'text'][i], TEST_TEXT); }
  finally { busy = false; btn.disabled = false; $('send').disabled = false; }
}

$('compose').addEventListener('submit', async (e) => {
  e.preventDefault();
  const text = $('text').value.trim();
  if (!text || busy) return;
  busy = true; $('send').disabled = true;
  try {
    const ok = await transmit('text', text);
    if (ok.length) { addMessage(text, ok, true); $('text').value = ''; }
  } finally { busy = false; $('send').disabled = false; }
});
$('text').addEventListener('keydown', (e) => {
  if (e.key === 'Enter' && !e.shiftKey) { e.preventDefault(); $('compose').requestSubmit(); }
});

resetPhases();
render();
log(null, 'pronto. Siga as fases acima.');
if ('serviceWorker' in navigator) navigator.serviceWorker.register('sw.js').catch(() => {});
