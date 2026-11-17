'use strict';
/*
 * Novo Messenger — interface.
 *
 * Todo o protocolo e o processamento de sinal ficam no núcleo Rust
 * (nm_core.wasm, carregado por nm.js). Aqui só ficam a tela, a câmera,
 * o microfone, o alto-falante e o Bluetooth.
 */

const $ = (id) => document.getElementById(id);
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
const hex = (b) => b.toString(16).padStart(2, '0').toUpperCase();
const CH_NAME = { luz: 'Luz', som: 'Som', lora: 'LoRa' };
let NM = null;

/* ---------------- Chat ---------------- */
const seen = new Map(); // texto -> {at, msg}: junta cópias que chegam por vários canais
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
  if (me) {
    history.push({ text, via, me: true, at: Date.now() });
  } else {
    const last = seen.get(text);
    if (last && Date.now() - last.at < 60000) {
      if (!last.msg.via.includes(via)) { last.msg.via.push(via); save(); render(); }
      return;
    }
    const msg = { text, via: [via], me: false, at: Date.now() };
    seen.set(text, { at: Date.now(), msg });
    history.push(msg);
    if (navigator.vibrate) navigator.vibrate(120);
  }
  save(); render();
}

/* ---------------- Registro ---------------- */
function log(ch, text, kind = '') {
  const box = $('log');
  const d = document.createElement('div');
  d.className = kind || ch || '';
  d.textContent = `${new Date().toLocaleTimeString('pt-BR')} ${ch ? '[' + CH_NAME[ch] + '] ' : ''}${text}`;
  box.appendChild(d);
  while (box.children.length > 200) box.firstChild.remove();
  box.scrollTop = box.scrollHeight;
}

/* ---------------- Fases ---------------- */
const PHASES = [
  { title: 'Sinal', desc: 'O emissor pisca ou apita 1-0-1-0 por 4 s. O receptor deve ver ondas regulares no gráfico.', btn: 'Enviar sinal de teste', kind: 'toggle' },
  { title: 'Bits e bytes', desc: 'Envia 7 bytes fixos (55 AA 0F F0 41 42 43). O receptor confere byte a byte no registro.', btn: 'Enviar bytes de teste', kind: 'bytes' },
  { title: 'Mensagem de teste', desc: 'Envia o quadro "TESTE-OK 123" com checksum. Confere se a mensagem inteira chega sem erro.', btn: 'Enviar mensagem de teste', kind: 'text' },
  { title: 'Conversa', desc: 'Escreva uma mensagem de verdade lá embaixo e envie. Passa quando chegar ao outro aparelho.', btn: null },
];
let phaseState;
function resetPhases() {
  phaseState = { luz: ['', '', '', ''], som: ['', '', '', ''], lora: [mesh ? 'ok' : '', 'na', '', ''] };
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
    const el = document.createElement('div');
    el.className = 'phase' + (['luz', 'som', 'lora'].some((c) => phaseState[c][i] === 'ok') ? ' done' : '');
    el.innerHTML = '<span class="num"></span><b></b><span class="d"></span><div class="chips"></div>';
    el.querySelector('.num').textContent = i + 1;
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
      b.type = 'button'; b.textContent = p.btn; b.disabled = !NM;
      b.onclick = () => runTest(i, b);
      chips.appendChild(b);
    }
    list.appendChild(el);
  });
}
$('phReset').onclick = resetPhases;

/* ---------------- Gráfico do sinal ---------------- */
function makeScope(canvas) {
  const pts = [];
  const ctx = canvas.getContext('2d');
  let raf = 0;
  const draw = () => {
    raf = requestAnimationFrame(draw);
    const dpr = window.devicePixelRatio || 1;
    const w = canvas.clientWidth, h = canvas.clientHeight;
    if (canvas.width !== Math.round(w * dpr)) { canvas.width = Math.round(w * dpr); canvas.height = Math.round(h * dpr); }
    ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
    ctx.clearRect(0, 0, w, h);
    if (!pts.length) return;
    const now = pts[pts.length - 1][0], span = 4000; // tempo do próprio sinal (relógio de amostras no som)
    while (pts.length && now - pts[0][0] > span) pts.shift();
    const cs = getComputedStyle(document.documentElement);
    ctx.fillStyle = cs.getPropertyValue('--ok').trim() + '33';
    for (let i = 0; i < pts.length - 1; i++) {
      if (pts[i][2]) {
        const x0 = w - (now - pts[i][0]) / span * w, x1 = w - (now - pts[i + 1][0]) / span * w;
        ctx.fillRect(x0, 0, x1 - x0 + 0.5, h);
      }
    }
    ctx.strokeStyle = cs.getPropertyValue('--fg').trim();
    ctx.lineWidth = 1.5; ctx.beginPath();
    pts.forEach(([t, v], i) => { const x = w - (now - t) / span * w, y = h - 4 - v * (h - 8); i ? ctx.lineTo(x, y) : ctx.moveTo(x, y); });
    ctx.stroke();
  };
  return {
    add(points) { for (const p of points) pts.push(p); },
    start() { canvas.hidden = false; cancelAnimationFrame(raf); draw(); },
    stop() { cancelAnimationFrame(raf); canvas.hidden = true; pts.length = 0; },
  };
}
const scopes = { luz: makeScope($('luzScope')), som: makeScope($('somScope')) };

/* Aplica o que o núcleo devolveu: gráfico, bytes, fases, mensagens */
function makeSink(ch, byteMs) {
  let line = [], timer = null;
  const flush = () => { if (line.length) log(ch, 'bytes: ' + line.map(hex).join(' ')); line = []; };
  return (d) => {
    scopes[ch].add(d.s);
    for (const e of d.e) {
      if (e.k === 'byte') {
        line.push(e.v);
        clearTimeout(timer); timer = setTimeout(flush, byteMs * 3);
        if (line.length >= 16) flush();
      } else if (e.k === 'phase') {
        flush();
        if (e.v === 3) log(ch, `mensagem de teste recebida: "${NM.TEST_TEXT}"`, 'ok');
        markPhase(ch, e.v - 1);
      } else if (e.k === 'msg') {
        flush();
        log(ch, `mensagem recebida: "${e.v}"`, 'ok');
        addMessage(e.v, CH_NAME[ch], false);
      } else if (e.k === 'bad') {
        flush();
        log(ch, 'quadro descartado: ' + e.v, 'err');
      }
    }
    return d.st;
  };
}

const receivers = new Set(); // receptores ativos, silenciados enquanto este aparelho transmite
const muteAll = (m) => receivers.forEach((r) => r.setMuted(m));

/* ---------------- LUZ ---------------- */
let flashAbort = false;
$('flashStop').onclick = () => { flashAbort = true; };
const luzT = () => +$('luzT').value || 150;

async function sendLight(kind, text) {
  const T = luzT();
  const bits = NM.txBits(kind, text, T);
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

let luz = null;
$('luzRx').onclick = async () => {
  if (luz) { stopLuz(); return; }
  let stream;
  try {
    stream = await navigator.mediaDevices.getUserMedia({ video: { facingMode: 'environment', frameRate: { ideal: 60 }, width: { ideal: 320 } } });
  } catch (e) { $('luzStatus').textContent = 'câmera negada: ' + e.message; return; }
  const v = $('cam'); v.srcObject = stream; await v.play();
  const T = luzT();
  const rx = NM.lightReceiver(T);
  receivers.add(rx);
  luz = { stream, rx };
  $('camWrap').hidden = false; $('luzDot').classList.add('on'); $('luzRx').textContent = 'Parar câmera';
  log('luz', `escutando pela câmera (bit ${T} ms)`);
  const sink = makeSink('luz', T * 11);
  scopes.luz.start();
  const c = document.createElement('canvas'); c.width = c.height = 24;
  const ctx = c.getContext('2d', { willReadFrequently: true });
  const tick = () => {
    if (!luz || luz.rx !== rx) return;
    const w = v.videoWidth, h = v.videoHeight;
    if (w) {
      ctx.drawImage(v, w * 0.3, h * 0.3, w * 0.4, h * 0.4, 0, 0, 24, 24);
      const d = ctx.getImageData(0, 0, 24, 24).data;
      let s = 0;
      for (let i = 0; i < d.length; i += 4) s += 0.299 * d[i] + 0.587 * d[i + 1] + 0.114 * d[i + 2];
      const y = s / (d.length / 4);
      rx.pushLuma(performance.now(), y);
      const st = sink(rx.drain());
      $('luzMeter').style.width = (y / 255 * 100) + '%';
      $('luzMeter').classList.toggle('hi', st.l === 1);
      $('luzStatus').textContent = `brilho ${st.a.toFixed(0)} · contraste ${st.b.toFixed(0)}${st.sig ? ' ✓' : ' (aproxime da luz)'}`;
    }
    if (v.requestVideoFrameCallback) v.requestVideoFrameCallback(tick); else requestAnimationFrame(tick);
  };
  tick();
};
function stopLuz() {
  luz.stream.getTracks().forEach((t) => t.stop());
  receivers.delete(luz.rx); luz.rx.free(); luz = null;
  $('camWrap').hidden = true; $('luzDot').classList.remove('on'); $('luzRx').textContent = 'Ouvir pela câmera';
  $('luzStatus').textContent = 'parado'; $('luzMeter').style.width = '0';
  scopes.luz.stop(); log('luz', 'câmera parada');
}

/* ---------------- SOM ---------------- */
let actx = null;
function audio() {
  if (!actx) actx = new (window.AudioContext || window.webkitAudioContext)();
  if (actx.state === 'suspended') actx.resume();
  return actx;
}
const freqs = () => $('somF').value.split(',').map(Number);
const somT = () => +$('somT').value || 80;

async function sendSound(kind, text) {
  const ctx = audio();
  const [f0, f1] = freqs();
  const pcm = NM.txSynth(kind, text, ctx.sampleRate, f0, f1, somT(), 0.8);
  const buf = ctx.createBuffer(1, pcm.length, ctx.sampleRate);
  buf.copyToChannel(pcm, 0);
  const src = ctx.createBufferSource();
  src.buffer = buf; src.connect(ctx.destination);
  log('som', `enviando (${buf.duration.toFixed(1)} s)`);
  const done = new Promise((r) => { src.onended = r; });
  src.start();
  await done;
  log('som', 'envio concluído');
}

let som = null;
$('somRx').onclick = async () => {
  if (som) { stopSom(); return; }
  const ctx = audio();
  let stream;
  try {
    stream = await navigator.mediaDevices.getUserMedia({ audio: { echoCancellation: false, noiseSuppression: false, autoGainControl: false } });
    await ctx.audioWorklet.addModule('mic-worklet.js');
  } catch (e) { $('somStatus').textContent = 'microfone indisponível: ' + e.message; stream?.getTracks().forEach((t) => t.stop()); return; }
  const T = somT();
  const [f0, f1] = freqs();
  const rx = NM.soundReceiver(ctx.sampleRate, f0, f1, T);
  receivers.add(rx);
  const src = ctx.createMediaStreamSource(stream);
  const tap = new AudioWorkletNode(ctx, 'nm-tap');
  const mute = ctx.createGain(); mute.gain.value = 0;
  src.connect(tap); tap.connect(mute); mute.connect(ctx.destination);
  som = { stream, rx, src, tap };
  $('somDot').classList.add('on'); $('somRx').textContent = 'Parar microfone';
  log('som', `escutando pelo microfone (bit ${T} ms, ${f0}/${f1} Hz, ${ctx.sampleRate} Hz)`);
  const sink = makeSink('som', T * 11);
  scopes.som.start();
  tap.port.onmessage = (e) => {
    if (!som || som.rx !== rx) return;
    rx.pushPcm(e.data);
    const st = sink(rx.drain());
    $('somMeter').style.width = Math.max(0, Math.min(100, (Math.max(st.a, st.b) + 80) * 1.25)) + '%';
    $('somMeter').classList.toggle('hi', st.l === 1);
    $('somStatus').textContent = `tom 0: ${st.a.toFixed(0)} dB · tom 1: ${st.b.toFixed(0)} dB${st.sig ? ' ✓' : ''}`;
  };
};
function stopSom() {
  som.tap.port.onmessage = null; som.src.disconnect(); som.tap.disconnect();
  som.stream.getTracks().forEach((t) => t.stop());
  receivers.delete(som.rx); som.rx.free(); som = null;
  $('somDot').classList.remove('on'); $('somRx').textContent = 'Ouvir pelo microfone';
  $('somStatus').textContent = 'parado'; $('somMeter').style.width = '0';
  scopes.som.stop(); log('som', 'microfone parado');
}

/* ---------------- LoRa / Meshtastic via Web Bluetooth ---------------- */
let mesh = null;

async function meshDrain() {
  for (let n = 0; n < 500 && mesh; n++) {
    const dv = await mesh.from.readValue();
    if (!dv.byteLength) break;
    const m = NM.meshDecode(new Uint8Array(dv.buffer, dv.byteOffset, dv.byteLength));
    if (!m) continue;
    const from = '!' + (m.from >>> 0).toString(16);
    if (m.text === NM.TEST_TEXT) { log('lora', `mensagem de teste de ${from}`, 'ok'); markPhase('lora', 2); }
    else { log('lora', `mensagem de ${from}: "${m.text}"`, 'ok'); addMessage(m.text, 'LoRa', false); markPhase('lora', 3); }
  }
}

$('loraConn').onclick = async () => {
  if (mesh) { try { mesh.dev.gatt.disconnect(); } catch (_) {} return; }
  if (!navigator.bluetooth) { $('loraStatus').textContent = 'este navegador não tem Web Bluetooth. Use o Chrome no Android ou no computador.'; return; }
  const U = NM.MESH;
  try {
    $('loraStatus').textContent = 'procurando rádio…';
    const dev = await navigator.bluetooth.requestDevice({ filters: [{ services: [U.service] }] });
    const srv = await (await dev.gatt.connect()).getPrimaryService(U.service);
    mesh = { dev, to: await srv.getCharacteristic(U.to), from: await srv.getCharacteristic(U.from), num: await srv.getCharacteristic(U.num), busy: false };
    dev.addEventListener('gattserverdisconnected', () => {
      mesh = null; $('loraDot').classList.remove('on'); $('loraConn').textContent = 'Conectar rádio'; $('loraStatus').textContent = 'desconectado';
      log('lora', 'rádio desconectado', 'err');
    });
    // abre a sessão e descarta a configuração que o rádio envia em seguida
    await mesh.to.writeValue(NM.meshWantConfig((Math.random() * 1e9) >>> 0));
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
  await mesh.to.writeValue(NM.meshText(text, (Math.random() * 0xffffffff) >>> 0));
  log('lora', `enviado ao rádio: "${text}"`);
}

/* ---------------- Envio por todos os canais ---------------- */
function selectedChannels() {
  return ['luz', 'som', 'lora'].filter((c) => $('via' + CH_NAME[c]).checked);
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
    return ch === 'luz' ? sendLight(kind, text) : sendSound(kind, text);
  });
  muteAll(true);
  const results = await Promise.allSettled(jobs);
  muteAll(false);
  const ok = [];
  results.forEach((r, i) => {
    if (r.status === 'rejected') log(chans[i], 'erro ao enviar: ' + (r.reason?.message || r.reason), 'err');
    else ok.push(CH_NAME[chans[i]]);
  });
  return ok;
}

let busy = false;
async function runTest(i, btn) {
  if (busy || !NM) return;
  busy = true; btn.disabled = true; $('send').disabled = true;
  log(null, `▶ Fase ${i + 1}: ${PHASES[i].title}`);
  try { await transmit(PHASES[i].kind, NM.TEST_TEXT); }
  finally { busy = false; btn.disabled = false; $('send').disabled = false; }
}

$('compose').addEventListener('submit', async (e) => {
  e.preventDefault();
  const text = $('text').value.trim();
  if (!text || busy || !NM) return;
  busy = true; $('send').disabled = true;
  try {
    const ok = await transmit('text', text);
    if (ok.length) { addMessage(text, ok, true); $('text').value = ''; }
  } finally { busy = false; $('send').disabled = false; }
});
$('text').addEventListener('keydown', (e) => {
  if (e.key === 'Enter' && !e.shiftKey) { e.preventDefault(); $('compose').requestSubmit(); }
});

/* ---------------- Início ---------------- */
const controls = ['luzRx', 'somRx', 'loraConn', 'send'];
controls.forEach((id) => { $(id).disabled = true; });
resetPhases();
render();
loadNM().then((m) => {
  NM = m;
  controls.forEach((id) => { $(id).disabled = false; });
  renderPhases();
  log(null, 'núcleo Rust carregado. Siga as fases acima.');
}).catch((e) => log(null, 'não foi possível carregar o núcleo (nm_core.wasm): ' + e.message, 'err'));
if ('serviceWorker' in navigator) navigator.serviceWorker.register('sw.js').catch(() => {});
