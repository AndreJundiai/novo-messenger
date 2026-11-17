//! Gera QR Codes em PBM (imagem P1) para conferir com um leitor externo.
//! Uso: cargo run -p nm-core --example qr_dump -- <pasta>
use nm_core::message::Message;
use std::fmt::Write;

fn main() {
    let dir = std::env::args().nth(1).expect("pasta de saída");
    let mut cases: Vec<String> = vec!["ola".into(), "HELLO WORLD".into(), "x".repeat(84), "y".repeat(213)];
    for (i, len) in [10usize, 60, 120, 200].into_iter().enumerate() {
        let text: String = "Corte às 15h ✂️ ".chars().cycle().take(len).collect();
        cases.push(nm_core::qr_text(&Message::new(1000 + i as u32, 7, 3, &text)));
    }
    cases.push("z".repeat(1200));
    for (i, payload) in cases.iter().enumerate() {
        let q = nm_core::qr::encode(payload.as_bytes()).expect("cabe");
        let border = 4;
        let n = q.size + 2 * border;
        let scale = 4;
        let mut s = format!("P1\n{} {}\n", n * scale, n * scale);
        for y in 0..n * scale {
            for x in 0..n * scale {
                let (mx, my) = ((x / scale) as isize - border as isize, (y / scale) as isize - border as isize);
                let dark = mx >= 0 && my >= 0 && (mx as usize) < q.size && (my as usize) < q.size && q.get(mx as usize, my as usize);
                s.push(if dark { '1' } else { '0' });
            }
            s.push('\n');
        }
        std::fs::write(format!("{dir}/qr{i}.pbm"), s).unwrap();
        let mut meta = String::new();
        let _ = write!(meta, "{}", payload);
        std::fs::write(format!("{dir}/qr{i}.txt"), meta).unwrap();
        eprintln!("qr{i}: versão {} ({} bytes)", q.version, payload.len());
    }
}
