//! `nm`: Novo Messenger no terminal (Linux, macOS, Windows).
//!
//! Fala e ouve pelo mesmo protocolo de som do app, usando o núcleo Rust.
//! O áudio entra e sai como arquivo WAV ou como PCM bruto (f32, mono) pela
//! entrada/saída padrão, para ligar no microfone e no alto-falante com as
//! ferramentas do sistema (arecord/aplay, sox, ffmpeg).

use nm_core::fsk::{synth, FskConfig};
use nm_core::session::{Event, Receiver};
use nm_core::{tx_bits, Payload, TEST_TEXT};
use std::io::{self, BufWriter, Read, Write};
use std::process::ExitCode;

const HELP: &str = "\
nm — Novo Messenger por som, sem internet

USO
  nm falar \"texto\"  [-o saida.wav]   gera o som da mensagem (sem -o: PCM f32 na saída padrão)
  nm fases           [-o saida.wav]   gera as 4 fases de teste em sequência
  nm ouvir entrada.wav                decodifica um arquivo WAV
  nm ouvir -                          decodifica PCM f32 mono da entrada padrão (ao vivo)

OPÇÕES
  --bit MS        duração de cada bit (padrão 80)
  --taxa HZ       taxa de amostragem (padrão 48000)
  --ultra         usa 17,5/18,5 kHz (quase inaudível) em vez de 1,5/2,5 kHz
  --volume V      0..1 (padrão 0,8)

AO VIVO
  Linux:  nm falar \"oi\" | aplay -f FLOAT_LE -r 48000 -c 1
          arecord -f FLOAT_LE -r 48000 -c 1 -t raw | nm ouvir -
  macOS/Windows (sox):
          nm falar \"oi\" | sox -t f32 -r 48000 -c 1 - -d
          sox -d -t f32 -r 48000 -c 1 - | nm ouvir -
";

struct Opts {
    bit_ms: f64,
    rate: u32,
    ultra: bool,
    volume: f32,
    out: Option<String>,
    args: Vec<String>,
}

fn parse(mut it: impl Iterator<Item = String>) -> Result<Opts, String> {
    let mut o = Opts { bit_ms: 80.0, rate: 48_000, ultra: false, volume: 0.8, out: None, args: Vec::new() };
    while let Some(a) = it.next() {
        let mut val = |name: &str| it.next().ok_or(format!("{name} precisa de um valor"));
        match a.as_str() {
            "--bit" => o.bit_ms = val("--bit")?.parse().map_err(|_| "--bit inválido")?,
            "--taxa" => o.rate = val("--taxa")?.parse().map_err(|_| "--taxa inválida")?,
            "--volume" => o.volume = val("--volume")?.replace(',', ".").parse().map_err(|_| "--volume inválido")?,
            "--ultra" => o.ultra = true,
            "-o" => o.out = Some(val("-o")?),
            _ => o.args.push(a),
        }
    }
    if !(20.0..=1000.0).contains(&o.bit_ms) {
        return Err("--bit deve ficar entre 20 e 1000 ms".into());
    }
    if !(8_000..=192_000).contains(&o.rate) {
        return Err("--taxa deve ficar entre 8000 e 192000 Hz".into());
    }
    Ok(o)
}

impl Opts {
    fn cfg(&self, rate: u32) -> FskConfig {
        let (f0, f1) = if self.ultra { FskConfig::NEAR_ULTRASONIC } else { FskConfig::AUDIBLE };
        FskConfig::new(rate as f32, f0, f1, self.bit_ms)
    }
}

// ---------- WAV ----------

fn write_wav(w: &mut impl Write, pcm: &[f32], rate: u32) -> io::Result<()> {
    let data_len = (pcm.len() * 2) as u32;
    w.write_all(b"RIFF")?;
    w.write_all(&(36 + data_len).to_le_bytes())?;
    w.write_all(b"WAVEfmt ")?;
    w.write_all(&16u32.to_le_bytes())?;
    w.write_all(&1u16.to_le_bytes())?; // PCM
    w.write_all(&1u16.to_le_bytes())?; // mono
    w.write_all(&rate.to_le_bytes())?;
    w.write_all(&(rate * 2).to_le_bytes())?;
    w.write_all(&2u16.to_le_bytes())?;
    w.write_all(&16u16.to_le_bytes())?;
    w.write_all(b"data")?;
    w.write_all(&data_len.to_le_bytes())?;
    for &x in pcm {
        w.write_all(&((x.clamp(-1.0, 1.0) * 32767.0) as i16).to_le_bytes())?;
    }
    Ok(())
}

/// Lê WAV PCM 16 bits ou float 32 bits; usa só o primeiro canal.
fn read_wav(buf: &[u8]) -> Result<(Vec<f32>, u32), String> {
    if buf.len() < 12 || &buf[0..4] != b"RIFF" || &buf[8..12] != b"WAVE" {
        return Err("não é um arquivo WAV".into());
    }
    let u16_at = |i: usize| u16::from_le_bytes([buf[i], buf[i + 1]]);
    let u32_at = |i: usize| u32::from_le_bytes([buf[i], buf[i + 1], buf[i + 2], buf[i + 3]]);
    let (mut fmt, mut pos) = (None, 12);
    while pos + 8 <= buf.len() {
        let id = &buf[pos..pos + 4];
        let size = u32_at(pos + 4) as usize;
        let body = pos + 8;
        let end = body.saturating_add(size).min(buf.len());
        if id == b"fmt " && size >= 16 {
            fmt = Some((u16_at(body), u16_at(body + 2) as usize, u32_at(body + 4), u16_at(body + 14)));
        } else if id == b"data" {
            let (format, channels, rate, bits) = fmt.ok_or("WAV sem bloco fmt")?;
            let data = &buf[body..end];
            let pcm = match (format, bits) {
                (1, 16) => data.chunks_exact(2 * channels).map(|c| i16::from_le_bytes([c[0], c[1]]) as f32 / 32768.0).collect(),
                (3, 32) => data.chunks_exact(4 * channels).map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]])).collect(),
                _ => return Err(format!("formato WAV não suportado (formato {format}, {bits} bits)")),
            };
            return Ok((pcm, rate));
        }
        pos = body + size + (size & 1);
    }
    Err("WAV sem bloco de dados".into())
}

// ---------- comandos ----------

fn emit(pcm: &[f32], o: &Opts) -> Result<(), String> {
    match &o.out {
        Some(path) => {
            let mut f = BufWriter::new(std::fs::File::create(path).map_err(|e| format!("{path}: {e}"))?);
            write_wav(&mut f, pcm, o.rate).and_then(|_| f.flush()).map_err(|e| e.to_string())?;
            eprintln!("gravado {path} ({:.1} s)", pcm.len() as f64 / o.rate as f64);
        }
        None => {
            let mut w = BufWriter::new(io::stdout().lock());
            for x in pcm {
                w.write_all(&x.to_le_bytes()).map_err(|e| e.to_string())?;
            }
            w.flush().map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

fn phases_bits(bit_ms: f64) -> Vec<u8> {
    let gap = vec![0u8; (1000.0 / bit_ms) as usize];
    let mut bits = gap.clone();
    for (p, t) in [(Payload::Toggle, ""), (Payload::TestBytes, ""), (Payload::Text, TEST_TEXT), (Payload::Text, "Olá pelo Novo Messenger ✂️")] {
        bits.extend(tx_bits(p, t, bit_ms));
        bits.extend(&gap);
    }
    bits
}

struct Printer {
    bytes: Vec<u8>,
}

impl Printer {
    fn flush_bytes(&mut self) {
        if !self.bytes.is_empty() {
            let hex: Vec<String> = self.bytes.iter().map(|b| format!("{b:02X}")).collect();
            eprintln!("  bytes: {}", hex.join(" "));
            self.bytes.clear();
        }
    }
    fn handle(&mut self, events: Vec<Event>) -> io::Result<()> {
        for e in events {
            match e {
                Event::Byte(b) => {
                    self.bytes.push(b);
                    if self.bytes.len() == 16 {
                        self.flush_bytes();
                    }
                }
                Event::Phase(p) => {
                    self.flush_bytes();
                    let name = ["sinal", "bits e bytes", "mensagem de teste", "conversa"][(p as usize - 1).min(3)];
                    eprintln!("✓ fase {p} ({name}) passou");
                }
                Event::Message(t) => {
                    self.flush_bytes();
                    let mut out = io::stdout().lock();
                    writeln!(out, "{t}")?;
                    out.flush()?;
                }
                Event::Bad(b) => {
                    self.flush_bytes();
                    eprintln!("✗ quadro descartado: {}", b.describe());
                }
            }
        }
        Ok(())
    }
}

fn listen(o: &Opts) -> Result<(), String> {
    let src = o.args.get(1).ok_or("diga o arquivo WAV, ou - para ouvir ao vivo")?;
    let mut printer = Printer { bytes: Vec::new() };
    if src == "-" {
        let mut rx = Receiver::sound(o.cfg(o.rate));
        eprintln!("ouvindo PCM f32 mono a {} Hz… (Ctrl+C para sair)", o.rate);
        let mut stdin = io::stdin().lock();
        let mut buf = vec![0u8; 4096];
        let mut carry = Vec::<u8>::new();
        loop {
            let n = stdin.read(&mut buf).map_err(|e| e.to_string())?;
            if n == 0 {
                break;
            }
            carry.extend_from_slice(&buf[..n]);
            let whole = carry.len() / 4 * 4;
            let pcm: Vec<f32> = carry[..whole].chunks_exact(4).map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]])).collect();
            carry.drain(..whole);
            rx.push_pcm(&pcm);
            printer.handle(rx.take_events()).map_err(|e| e.to_string())?;
        }
    } else {
        let data = std::fs::read(src).map_err(|e| format!("{src}: {e}"))?;
        let (pcm, rate) = read_wav(&data)?;
        let mut rx = Receiver::sound(o.cfg(rate));
        for c in pcm.chunks(1024) {
            rx.push_pcm(c);
            printer.handle(rx.take_events()).map_err(|e| e.to_string())?;
        }
    }
    printer.flush_bytes();
    Ok(())
}

fn run() -> Result<(), String> {
    let o = parse(std::env::args().skip(1))?;
    match o.args.first().map(String::as_str) {
        Some("falar") => {
            let text = o.args.get(1).ok_or("diga o texto: nm falar \"oi\"")?;
            emit(&synth(&tx_bits(Payload::Text, text, o.bit_ms), &o.cfg(o.rate), 300.0, o.volume), &o)
        }
        Some("fases") => emit(&synth(&phases_bits(o.bit_ms), &o.cfg(o.rate), 300.0, o.volume), &o),
        Some("ouvir") => listen(&o),
        _ => {
            print!("{HELP}");
            Ok(())
        }
    }
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("erro: {e}");
            ExitCode::FAILURE
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wav_roundtrip_decodes_all_phases() {
        let o = parse(["--bit", "60"].map(String::from).into_iter()).unwrap();
        let pcm = synth(&phases_bits(60.0), &o.cfg(48_000), 300.0, 0.5);
        let mut wav = Vec::new();
        write_wav(&mut wav, &pcm, 48_000).unwrap();
        let (back, rate) = read_wav(&wav).unwrap();
        assert_eq!(rate, 48_000);
        let mut rx = Receiver::sound(o.cfg(rate));
        rx.push_pcm(&back);
        let ev = rx.take_events();
        let phases: Vec<u8> = ev.iter().filter_map(|e| if let Event::Phase(p) = e { Some(*p) } else { None }).collect();
        assert_eq!(phases, vec![1, 2, 3, 4]);
        assert!(ev.contains(&Event::Message("Olá pelo Novo Messenger ✂️".into())));
    }

    #[test]
    fn rejects_bad_options() {
        assert!(parse(["--bit", "5"].map(String::from).into_iter()).is_err());
        assert!(read_wav(b"nada").is_err());
    }
}
