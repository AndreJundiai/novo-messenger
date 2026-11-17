'use strict';
/*
 * Novo Messenger — interface.
 *
 * Todo o protocolo, a correção de erros, o QR Code e o histórico (CRDT)
 * ficam no núcleo Rust (nm_core.wasm, carregado por nm.js). Aqui só ficam a
 * tela, a câmera, o microfone, o alto-falante e o Bluetooth.
 */

const $ = (id) => document.getElementById(id);
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
const hex = (b) => b.toString(16).padStart(2, '0').toUpperCase();
const randId = () => (crypto.getRandomValues(new Uint32Array(1))[0] || 1) >>> 0;
const CHANNELS = ['luz', 'som', 'lora', 'qr'];
const CH_NAME = { luz: 'Luz', som: 'Som', lora: 'LoRa', qr: 'QR' };
// Bits de "por onde chegou", iguais aos de nm-core::store::via
const VIA = { me: 1, luz: 2, som: 4, lora: 8, qr: 16, file: 32 };
const VIA_NAME = [[VIA.me, 'você'], [VIA.luz, 'Luz'], [VIA.som, 'Som'], [VIA.lora, 'LoRa'], [VIA.qr, 'QR'], [VIA.file, 'arquivo']];
const TTL = 3; // quantas pontes uma mensagem nova pode atravessar
let NM = null;

/* ---------------- Conversa (histórico no núcleo, CRDT) ---------------- */
function persist() {
  try { localStorage.setItem('nm-store', NM.storeSave()); } catch (_) {}
}
function render() {
  const chat = $('chat');
  chat.querySelectorAll('.msg').forEach((n) => n.remove());
  const list = NM ? NM.storeList() : [];
  $('empty').hidden = list.length > 0;
  for (const m of list) {
    const d = document.createElement('div');
    d.className = 'msg' + (m.via & VIA.me ? ' me' : '');
    d.textContent = m.text;
    const meta = document.createElement('span');
    meta.className = 'meta';
    const via = VIA_NAME.filter(([bit]) => m.via & bit && bit !== VIA.me).map(([, n]) => n);
    const when = new Date(m.at).toLocaleTimeString('pt-BR', { hour: '2-digit', minute: '2-digit' });
    meta.textContent = `${m.via & VIA.me ? 'você' : 'recebida'}${via.length ? ' · ' + via.join(' + ') : ''} · ${when}`;
    d.appendChild(meta);
    chat.appendChild(d);
  }
  chat.scrollTop = chat.scrollHeight;
}

/* Mensagem com cabeçalho chegou por luz, som ou QR */
function onMessage(ch, m) {
  const isNew = NM.storeReceive(m, VIA[ch], Date.now());
  persist(); render();
  if (!isNew) { log(ch, `cópia de mensagem já recebida: "${m.text}"`); return; }
  log(ch, `mensagem recebida: "${m.text}"`, 'ok');
  if (navigator.vibrate) navigator.vibrate(120);
  bridge(ch, m);
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
  { title: 'Mensagem de teste', desc: 'Envia "TESTE-OK 123" com correção de erros. Confere se chega inteira (bytes consertados aparecem no registro).', btn: 'Enviar mensagem de teste', kind: 'test' },
  { title: 'Conversa', desc: 'Escreva uma mensagem de verdade lá embaixo e envie. Passa quando chegar ao outro aparelho.', btn: null },
];
let phaseState;
function resetPhases() {
  phaseState = { luz: ['', '', '', ''], som: ['', '', '', ''], lora: [mesh ? 'ok' : '', 'na', '', ''], qr: ['na', 'na', '', ''] };
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
    el.className = 'phase' + (CHANNELS.some((c) => phaseState[c][i] === 'ok') ? ' done' : '');
    el.innerHTML = '<span class="num"></span><b></b><span class="d"></span><div class="chips"></div>';
    el.querySelector('.num').textContent = i + 1;
    el.querySelector('b').textContent = p.title;
    el.querySelector('.d').textContent = p.desc;
    const chips = el.querySelector('.chips');
    for (const c of CHANNELS) {
      const s = phaseState[c][i];
      const chip = document.createElement('span');
      chip.className = 'chip ' + s;
      chip.textContent = CH_NAME[c] + (s === 'ok' ? ' ✓' : s === 'na' ? '' : ' …');
      chips.appendChild(chip);
    }
    if (p.btn) {
      const b = document.createElement('button');
      b.type = 'button'; b.textContent = p.btn; b.disabled = !NM;
      b.onclick = () => runTest(i);
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

/* Aplica o que o núcleo devolveu: gráfico, bytes, correções, fases, mensagens */
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
      } else if (e.k === 'fixed') {
        flush(); log(ch, `correção de erros: ${e.v} byte(s) consertado(s)`, 'ok');
      } else if (e.k === 'phase') {
        flush();
        if (e.v === 3) log(ch, `mensagem de teste recebida: "${NM.TEST_TEXT}"`, 'ok');
        markPhase(ch, e.v - 1);
      } else if (e.k === 'msg') {
        flush(); onMessage(ch, e.v);
      } else if (e.k === 'bad') {
        flush(); log(ch, 'quadro descartado: ' + e.v, 'err');
      }
    }
    return d.st;
  };
}

// Receptor ativo por canal. Ao transmitir por luz ou som, só o receptor do
// mesmo canal é silenciado (eco do próprio aparelho); os outros seguem ouvindo.
const receivers = { luz: null, som: null };
const muteFor = (chans, m) => chans.forEach((c) => receivers[c]?.setMuted(m));

/* ---------------- LUZ (e leitura de QR, pela mesma câmera) ---------------- */
let flashAbort = false;
$('flashStop').onclick = () => { flashAbort = true; };
const luzT = () => +$('luzT').value || 150;

async function sendLight(kind, msg) {
  const T = luzT();
  const bits = NM.txBits(kind, msg, T);
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

/* Leitor de QR: nativo (BarcodeDetector) quando existe, senão jsQR */
function makeQrReader() {
  let detector = null;
  try { if ('BarcodeDetector' in window) detector = new BarcodeDetector({ formats: ['qr_code'] }); } catch (_) {}
  const c = document.createElement('canvas');
  const ctx = c.getContext('2d', { willReadFrequently: true });
  let busy = false, last = '', lastAt = 0;
  const handle = (raw) => {
    if (!raw || (raw === last && Date.now() - lastAt < 10000)) return;
    last = raw; lastAt = Date.now();
    const m = NM.qrParse(raw);
    if (!m) { log('qr', 'QR lido, mas não é do Novo Messenger'); return; }
    if (m.fixed) log('qr', `correção de erros: ${m.fixed} byte(s) consertado(s)`, 'ok');
    if (m.test) { log('qr', `mensagem de teste recebida: "${m.text}"`, 'ok'); markPhase('qr', 2); return; }
    onMessage('qr', m);
    markPhase('qr', 3);
  };
  return async (video) => {
    if (busy || !video.videoWidth) return;
    busy = true;
    try {
      if (detector) {
        const codes = await detector.detect(video);
        for (const code of codes) handle(code.rawValue);
      } else if (window.jsQR) {
        const scale = Math.min(1, 480 / video.videoWidth);
        c.width = Math.round(video.videoWidth * scale); c.height = Math.round(video.videoHeight * scale);
        ctx.drawImage(video, 0, 0, c.width, c.height);
        const img = ctx.getImageData(0, 0, c.width, c.height);
        const code = window.jsQR(img.data, c.width, c.height, { inversionAttempts: 'dontInvert' });
        if (code) handle(code.data);
      }
    } catch (_) { /* quadro ruim: tenta no próximo */ } finally { busy = false; }
  };
}

let luz = null;
$('luzRx').onclick = async () => {
  if (luz) { stopLuz(); return; }
  let stream;
  try {
    stream = await navigator.mediaDevices.getUserMedia({ video: { facingMode: 'environment', frameRate: { ideal: 60 }, width: { ideal: 640 } } });
  } catch (e) { $('luzStatus').textContent = 'câmera negada: ' + e.message; return; }
  const v = $('cam'); v.srcObject = stream; await v.play();
  const T = luzT();
  const rx = NM.lightReceiver(T);
  receivers.luz = rx;
  luz = { stream, rx };
  $('camWrap').hidden = false; $('luzDot').classList.add('on'); $('qrDot').classList.add('on');
  $('luzRx').textContent = 'Parar câmera';
  log('luz', `escutando pela câmera (bit ${T} ms). QR Codes também são lidos.`);
  const sink = makeSink('luz', T * 11);
  const readQr = makeQrReader();
  let lastQr = 0;
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
      if (performance.now() - lastQr > 250) { lastQr = performance.now(); readQr(v); }
    }
    if (v.requestVideoFrameCallback) v.requestVideoFrameCallback(tick); else requestAnimationFrame(tick);
  };
  tick();
};
function stopLuz() {
  luz.stream.getTracks().forEach((t) => t.stop());
  receivers.luz = null; luz.rx.free(); luz = null;
  $('camWrap').hidden = true; $('luzDot').classList.remove('on'); $('qrDot').classList.remove('on');
  $('luzRx').textContent = 'Ouvir pela câmera';
  $('luzStatus').textContent = 'parado'; $('luzMeter').style.width = '0';
  scopes.luz.stop(); log('luz', 'câmera parada');
}

/* ---------------- QR (envio) ---------------- */
let qrClose = null;
$('qrClose').onclick = () => qrClose?.();

async function sendQr(kind, msg) {
  const q = NM.qr(kind, msg);
  if (!q) throw new Error('mensagem grande demais para um QR Code');
  const quiet = 4, n = q.size + quiet * 2, scale = Math.max(4, Math.floor(1024 / n));
  const cv = $('qrCanvas');
  cv.width = cv.height = n * scale;
  const ctx = cv.getContext('2d');
  ctx.fillStyle = '#ffffff'; ctx.fillRect(0, 0, cv.width, cv.height);
  ctx.fillStyle = '#000000';
  for (let y = 0; y < q.size; y++) {
    for (let x = 0; x < q.size; x++) {
      if (q.modules[y * q.size + x]) ctx.fillRect((x + quiet) * scale, (y + quiet) * scale, scale, scale);
    }
  }
  $('qrCaption').textContent = (kind === 'test' ? 'Mensagem de teste' : `"${msg.text}"`) + ' · aponte a câmera do outro aparelho. Fecha sozinho em 20 s.';
  $('qrView').hidden = false;
  log('qr', `mostrando QR (${q.size}×${q.size} módulos)`);
  await new Promise((resolve) => {
    const timer = setTimeout(resolve, 20000);
    qrClose = () => { clearTimeout(timer); resolve(); };
  });
  qrClose = null;
  $('qrView').hidden = true;
  log('qr', 'QR fechado');
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

async function sendSound(kind, msg) {
  const ctx = audio();
  const [f0, f1] = freqs();
  const pcm = NM.txSynth(kind, msg, ctx.sampleRate, f0, f1, somT(), 0.8);
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
  receivers.som = rx;
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
  receivers.som = null; som.rx.free(); som = null;
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
    const r = NM.meshDecode(new Uint8Array(dv.buffer, dv.byteOffset, dv.byteLength));
    if (!r) continue;
    const from = '!' + (r.from >>> 0).toString(16);
    if (r.text === NM.TEST_TEXT) { log('lora', `mensagem de teste de ${from}`, 'ok'); markPhase('lora', 2); continue; }
    // O rádio leva texto puro (compatível com outros apps Meshtastic): o núcleo cria a mensagem.
    const m = NM.storeReceiveText(r.text, VIA.lora, randId(), Date.now());
    persist(); render();
    if (!m) { log('lora', `cópia de mensagem já recebida: "${r.text}"`); continue; }
    log('lora', `mensagem de ${from}: "${r.text}"`, 'ok');
    if (navigator.vibrate) navigator.vibrate(120);
    markPhase('lora', 3);
    bridge('lora', m);
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
    await mesh.to.writeValue(NM.meshWantConfig(randId()));
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
  await mesh.to.writeValue(NM.meshText(text, randId()));
  log('lora', `enviado ao rádio: "${text}"`);
}

/* ---------------- Transmissão (uma por vez, em fila) ---------------- */
const VIA_BOX = { luz: 'viaLuz', som: 'viaSom', lora: 'viaLora', qr: 'viaQr' };
const selectedChannels = () => CHANNELS.filter((c) => $(VIA_BOX[c]).checked);
let queue = Promise.resolve();
let pending = 0;
function setSending(on) {
  $('send').disabled = on || !NM;
  document.querySelectorAll('#phaseList button').forEach((b) => { b.disabled = on || !NM; });
}

// kind: 'toggle' (fase 1), 'bytes' (fase 2), 'test' (fase 3), 'msg' (mensagem)
function transmit(kind, msg, chans) {
  pending++; setSending(true);
  const run = async () => {
    if (!chans.length) { log(null, 'marque pelo menos um canal em "Enviar por"', 'err'); return []; }
    const jobs = chans.map((ch) => {
      if (ch === 'lora') {
        if (kind === 'toggle' || kind === 'bytes') { log('lora', 'esta fase não se aplica ao rádio (ele já confere os dados)'); return Promise.resolve(); }
        return sendLora(kind === 'test' ? NM.TEST_TEXT : msg.text);
      }
      if (ch === 'qr') {
        if (kind === 'toggle' || kind === 'bytes') { log('qr', 'esta fase não se aplica ao QR (ele já confere os dados)'); return Promise.resolve(); }
        return sendQr(kind, msg);
      }
      return ch === 'luz' ? sendLight(kind, msg) : sendSound(kind, msg);
    });
    muteFor(chans, true);
    const results = await Promise.allSettled(jobs);
    muteFor(chans, false);
    const ok = [];
    results.forEach((r, i) => {
      if (r.status === 'rejected') log(chans[i], 'erro ao enviar: ' + (r.reason?.message || r.reason), 'err');
      else ok.push(chans[i]);
    });
    return ok;
  };
  const p = queue.then(run, run);
  queue = p.catch(() => {}).finally(() => { if (--pending === 0) setSending(false); });
  return p;
}

/* Modo ponte: repassa para os outros canais marcados o que chegou por um canal */
function bridge(fromCh, m) {
  if (!$('bridge').checked) return;
  const targets = selectedChannels().filter((c) => c !== fromCh);
  if (!targets.length) return;
  // Mensagem vinda do LoRa (texto puro) sai com ttl 0: não volta a ser repassada.
  // As outras gastam um repasse.
  const out = fromCh === 'lora' ? m : (m.ttl > 0 ? { ...m, ttl: m.ttl - 1 } : null);
  if (!out) { log(fromCh, 'ponte: limite de repasses atingido, não repassa'); return; }
  log(fromCh, `ponte: repassando para ${targets.map((c) => CH_NAME[c]).join(', ')}`, 'ok');
  // Espera um pouco (aleatório) para não falar ao mesmo tempo que outra ponte.
  sleep(500 + Math.random() * 1500).then(() => transmit('msg', out, targets));
}

function runTest(i) {
  if (!NM) return;
  log(null, `▶ Fase ${i + 1}: ${PHASES[i].title}`);
  transmit(PHASES[i].kind, null, selectedChannels());
}

$('compose').addEventListener('submit', async (e) => {
  e.preventDefault();
  const text = $('text').value.trim();
  if (!text || !NM) return;
  const chans = selectedChannels();
  if (!chans.length) { log(null, 'marque pelo menos um canal em "Enviar por"', 'err'); return; }
  const msg = NM.storeCompose(text, TTL, randId(), Date.now());
  persist(); render();
  $('text').value = '';
  const ok = await transmit('msg', msg, chans);
  if (!ok.length) log(null, 'a mensagem ficou salva aqui, mas não saiu por nenhum canal', 'err');
});
$('text').addEventListener('keydown', (e) => {
  if (e.key === 'Enter' && !e.shiftKey) { e.preventDefault(); $('compose').requestSubmit(); }
});

/* ---------------- Exportar, importar, apagar ---------------- */
$('exportBtn').onclick = () => {
  const data = NM.storeExport();
  const a = document.createElement('a');
  a.href = URL.createObjectURL(new Blob([data], { type: 'application/octet-stream' }));
  a.download = `conversa-${new Date().toISOString().slice(0, 10)}.nmlog`;
  document.body.appendChild(a); a.click(); a.remove();
  setTimeout(() => URL.revokeObjectURL(a.href), 5000);
  $('toolsStatus').textContent = `arquivo com ${NM.storeList().length} mensagens. Envie por Bluetooth ou cabo para o outro aparelho.`;
};
$('importFile').onchange = async (e) => {
  const f = e.target.files[0];
  e.target.value = '';
  if (!f) return;
  const n = NM.storeImport(new Uint8Array(await f.arrayBuffer()), Date.now());
  if (n < 0) { $('toolsStatus').textContent = 'esse arquivo não é uma conversa do Novo Messenger.'; return; }
  persist(); render();
  $('toolsStatus').textContent = n ? `${n} mensagem(ns) nova(s) juntada(s) à conversa.` : 'nada novo: você já tinha todas essas mensagens.';
  log(null, `importado: ${n} mensagem(ns) nova(s)`, 'ok');
};
let clearArmed = 0;
$('clearBtn').onclick = () => {
  if (Date.now() - clearArmed > 4000) {
    clearArmed = Date.now();
    $('toolsStatus').textContent = 'toque em "Apagar conversa" de novo para confirmar.';
    return;
  }
  clearArmed = 0;
  NM.storeClear(); persist(); render();
  $('toolsStatus').textContent = 'conversa apagada neste aparelho.';
};

/* ---------------- Início ---------------- */
const controls = ['luzRx', 'somRx', 'loraConn', 'send', 'exportBtn', 'clearBtn', 'importFile'];
controls.forEach((id) => { $(id).disabled = true; });
resetPhases();
loadNM().then((m) => {
  NM = m;
  try {
    const saved = localStorage.getItem('nm-store');
    if (saved && !NM.storeLoad(saved)) log(null, 'histórico salvo estava corrompido e foi ignorado', 'err');
  } catch (_) {}
  controls.forEach((id) => { $(id).disabled = false; });
  renderPhases(); render();
  log(null, 'núcleo Rust carregado. Siga as fases acima.');
}).catch((e) => log(null, 'não foi possível carregar o núcleo (nm_core.wasm): ' + e.message, 'err'));
if ('serviceWorker' in navigator) navigator.serviceWorker.register('sw.js').catch(() => {});
