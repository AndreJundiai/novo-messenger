// node test.js — testa o protocolo de luz/som e o protobuf sem hardware
const fs = require('fs');
const src = fs.readFileSync(__dirname + '/app.js', 'utf8');
const core = src.split('/* ---------------- Chat')[0];
const mesh = src.slice(src.indexOf('function varint'), src.indexOf('async function meshDrain'));
const m = new Function('document', core + mesh + '; return { encodeFrame, FrameDecoder, frameToBits, bytesToBits, toggleBits, Uart, ToggleDetector, TEST_BYTES, pbParse, varint, lenField, fixed32 };')({ getElementById() {} });
let fails = 0;
const check = (name, ok) => { console.log((ok ? 'ok   ' : 'FALHA') + ' ' + name); if (!ok) fails++; };

// Simula transmissão com amostragem irregular (câmera ~30 fps com atrasos) e ruído de fase
function simulate(text, T, sampleMs, jitter, seed = 1) {
  let s = seed; const rnd = () => (s = (s * 16807) % 2147483647) / 2147483647;
  const bits = m.frameToBits(m.encodeFrame(text));
  const out = [];
  const fd = new m.FrameDecoder((t) => out.push(t));
  const u = new m.Uart(T, (b) => fd.push(b));
  const t0 = 500 + rnd() * 100;
  for (let t = 0; t < t0 + bits.length * T + 1000; t += sampleMs + (rnd() - 0.5) * jitter) {
    const i = Math.floor((t - t0) / T);
    u.push(t, i >= 0 && i < bits.length ? bits[i] : 0);
  }
  return out;
}
const msg = 'Olá, tudo bem? Corte às 15h ✂️';
check('luz 150ms @30fps', simulate(msg, 150, 33.3, 15)[0] === msg);
check('luz 100ms @30fps', simulate(msg, 100, 33.3, 10, 7)[0] === msg);
check('som 80ms @8ms', simulate(msg, 80, 8, 4)[0] === msg);
check('som 40ms @8ms', simulate(msg, 40, 8, 4, 3)[0] === msg);

// Bits corrompidos não podem virar mensagem
{
  const out = []; const fd = new m.FrameDecoder((t) => out.push(t));
  const f = m.encodeFrame('teste'); f[3] ^= 1; f.forEach((b) => fd.push(b));
  check('checksum rejeita erro', out.length === 0);
}

// Protobuf Meshtastic: monta um FromRadio com texto e lê de volta
{
  const payload = [...Buffer.from('oi pelo LoRa')];
  const data = [0x08, 0x01, ...m.lenField(2, payload)];
  const pkt = [...m.fixed32(1, 0x12345678), ...m.fixed32(2, 0xffffffff), ...m.lenField(4, data)];
  const fromRadio = new Uint8Array([0x08, 0x05, ...m.lenField(2, pkt)]);
  const fr = m.pbParse(fromRadio);
  const p = m.pbParse(fr[2][0]);
  const d = m.pbParse(p[4][0]);
  check('protobuf texto', d[1][0] === 1 && Buffer.from(d[2][0]).toString() === 'oi pelo LoRa' && (p[2][0] >>> 0) === 0xffffffff);
  check('varint 300', JSON.stringify(m.varint(300)) === '[172,2]');
}
// Fase 1: detector de 1010… no ritmo certo; não dispara com ritmo errado
function feed(bits, T, Trx, sampleMs) {
  let hit = false; const d = new m.ToggleDetector(Trx, () => { hit = true; });
  for (let t = 0; t < bits.length * T + 500; t += sampleMs) { const i = Math.floor(t / T); d.push(t, i < bits.length ? bits[i] : 0); }
  return hit;
}
check('fase 1 detecta sinal', feed(m.toggleBits(3000, 150), 150, 150, 33.3));
check('fase 1 ignora ritmo errado', !feed(m.toggleBits(3000, 400), 400, 150, 33.3));
// Fase 2: bytes crus chegam iguais
{
  const got = []; const u = new m.Uart(150, (b) => got.push(b)); const bits = m.bytesToBits(m.TEST_BYTES);
  for (let t = 0; t < bits.length * 150 + 500; t += 33.3) { const i = Math.floor((t - 200) / 150); u.push(t, i >= 0 && i < bits.length ? bits[i] : 0); }
  check('fase 2 bytes', JSON.stringify(got.slice(-7)) === JSON.stringify(m.TEST_BYTES));
}
process.exit(fails ? 1 : 0);
