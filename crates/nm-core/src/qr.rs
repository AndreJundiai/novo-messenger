//! Gerador de QR Code (ISO/IEC 18004), modo byte, correção nível M,
//! versões 1 a 40, com escolha automática da máscara.
//!
//! Usado para mandar a mensagem inteira de uma vez pela câmera: a tela mostra
//! o QR e o outro aparelho lê. A correção de erros é o mesmo Reed-Solomon de
//! [`crate::rs`].

use crate::rs;

/// Bytes de correção por bloco, nível M, por versão (índice 0 não usado).
const ECC_M: [usize; 41] = [
    0, 10, 16, 26, 18, 24, 16, 18, 22, 22, 26, 30, 22, 22, 24, 24, 28, 28, 26, 26, 26, 26, 28, 28, 28, 28, 28, 28, 28, 28, 28, 28, 28, 28, 28, 28,
    28, 28, 28, 28, 28,
];
/// Número de blocos, nível M, por versão.
const BLOCKS_M: [usize; 41] = [
    0, 1, 1, 1, 2, 2, 4, 4, 4, 5, 5, 5, 8, 9, 9, 10, 10, 11, 13, 14, 16, 17, 17, 18, 20, 21, 23, 25, 26, 28, 29, 31, 33, 35, 37, 38, 40, 43, 45,
    47, 49,
];
/// Bits de formato do nível M.
const ECL_M_BITS: u32 = 0;

pub struct QrCode {
    pub version: usize,
    pub size: usize,
    modules: Vec<bool>,
}

impl QrCode {
    /// `true` = módulo escuro. Coordenadas a partir do canto superior esquerdo.
    pub fn get(&self, x: usize, y: usize) -> bool {
        self.modules[y * self.size + x]
    }
}

fn raw_modules(ver: usize) -> usize {
    let mut r = (16 * ver + 128) * ver + 64;
    if ver >= 2 {
        let na = ver / 7 + 2;
        r -= (25 * na - 10) * na - 55;
        if ver >= 7 {
            r -= 36;
        }
    }
    r
}

fn data_codewords(ver: usize) -> usize {
    raw_modules(ver) / 8 - ECC_M[ver] * BLOCKS_M[ver]
}

fn alignment_positions(ver: usize) -> Vec<usize> {
    if ver == 1 {
        return Vec::new();
    }
    let size = ver * 4 + 17;
    let na = ver / 7 + 2;
    let step = (ver * 8 + na * 3 + 5) / (na * 4 - 4) * 2;
    let mut v: Vec<usize> = (0..na - 1).map(|i| size - 7 - i * step).collect();
    v.push(6);
    v.reverse();
    v
}

/// Maior quantidade de bytes que cabe na versão 40-M.
pub fn max_bytes() -> usize {
    (data_codewords(40) * 8 - 4 - 16) / 8
}

/// Gera o menor QR Code (nível M) que comporta `data`. `None` se não couber.
pub fn encode(data: &[u8]) -> Option<QrCode> {
    let ver = (1..=40).find(|&v| {
        let count_bits = if v < 10 { 8 } else { 16 };
        4 + count_bits + data.len() * 8 <= data_codewords(v) * 8
    })?;
    let cap = data_codewords(ver) * 8;

    let mut bits: Vec<bool> = Vec::with_capacity(cap);
    let put = |val: u32, n: usize, bits: &mut Vec<bool>| (0..n).rev().for_each(|i| bits.push((val >> i) & 1 != 0));
    put(0b0100, 4, &mut bits);
    put(data.len() as u32, if ver < 10 { 8 } else { 16 }, &mut bits);
    for &b in data {
        put(b as u32, 8, &mut bits);
    }
    let term = (cap - bits.len()).min(4);
    put(0, term, &mut bits);
    put(0, (8 - bits.len() % 8) % 8, &mut bits);
    let mut codewords: Vec<u8> = bits.chunks(8).map(|c| c.iter().fold(0u8, |a, &b| (a << 1) | b as u8)).collect();
    for pad in [0xEC, 0x11].into_iter().cycle() {
        if codewords.len() >= data_codewords(ver) {
            break;
        }
        codewords.push(pad);
    }

    let mut b = Builder::new(ver);
    b.draw_function_patterns();
    b.draw_codewords(&add_ecc_and_interleave(&codewords, ver));
    let mask = (0..8)
        .min_by_key(|&m| {
            b.apply_mask(m);
            b.draw_format(m);
            let p = b.penalty();
            b.apply_mask(m);
            p
        })
        .unwrap_or(0);
    b.apply_mask(mask);
    b.draw_format(mask);
    Some(QrCode { version: ver, size: b.size, modules: b.modules })
}

fn add_ecc_and_interleave(data: &[u8], ver: usize) -> Vec<u8> {
    let nblocks = BLOCKS_M[ver];
    let ecc_len = ECC_M[ver];
    let raw = raw_modules(ver) / 8;
    let nshort = nblocks - raw % nblocks;
    let short_len = raw / nblocks;
    let mut blocks: Vec<Vec<u8>> = Vec::with_capacity(nblocks);
    let mut k = 0;
    for i in 0..nblocks {
        let dlen = short_len - ecc_len + usize::from(i >= nshort);
        let dat = &data[k..k + dlen];
        k += dlen;
        let mut block = dat.to_vec();
        if i < nshort {
            block.push(0); // espaço para igualar aos blocos longos
        }
        block.extend(rs::parity(dat, ecc_len));
        blocks.push(block);
    }
    let mut out = Vec::with_capacity(raw);
    for i in 0..blocks[0].len() {
        for (j, block) in blocks.iter().enumerate() {
            if i != short_len - ecc_len || j >= nshort {
                out.push(block[i]);
            }
        }
    }
    out
}

struct Builder {
    ver: usize,
    size: usize,
    modules: Vec<bool>,
    func: Vec<bool>,
}

impl Builder {
    fn new(ver: usize) -> Self {
        let size = ver * 4 + 17;
        Self { ver, size, modules: vec![false; size * size], func: vec![false; size * size] }
    }

    fn set(&mut self, x: usize, y: usize, dark: bool) {
        let i = y * self.size + x;
        self.modules[i] = dark;
        self.func[i] = true;
    }

    fn draw_function_patterns(&mut self) {
        let s = self.size;
        for i in 0..s {
            self.set(6, i, i % 2 == 0);
            self.set(i, 6, i % 2 == 0);
        }
        for (cx, cy) in [(3, 3), (s - 4, 3), (3, s - 4)] {
            for dy in -4i32..=4 {
                for dx in -4i32..=4 {
                    let (x, y) = (cx as i32 + dx, cy as i32 + dy);
                    if (0..s as i32).contains(&x) && (0..s as i32).contains(&y) {
                        let d = dx.abs().max(dy.abs());
                        self.set(x as usize, y as usize, d != 2 && d != 4);
                    }
                }
            }
        }
        let pos = alignment_positions(self.ver);
        let n = pos.len();
        for i in 0..n {
            for j in 0..n {
                if (i == 0 && j == 0) || (i == 0 && j == n - 1) || (i == n - 1 && j == 0) {
                    continue;
                }
                for dy in -2i32..=2 {
                    for dx in -2i32..=2 {
                        let (x, y) = ((pos[i] as i32 + dx) as usize, (pos[j] as i32 + dy) as usize);
                        self.set(x, y, dx.abs().max(dy.abs()) != 1);
                    }
                }
            }
        }
        self.draw_format(0);
        if self.ver >= 7 {
            let mut rem = self.ver as u32;
            for _ in 0..12 {
                rem = (rem << 1) ^ ((rem >> 11) * 0x1F25);
            }
            let bits = (self.ver as u32) << 12 | rem;
            for i in 0..18 {
                let bit = (bits >> i) & 1 != 0;
                let (a, b) = (s - 11 + i % 3, i / 3);
                self.set(a, b, bit);
                self.set(b, a, bit);
            }
        }
    }

    fn draw_format(&mut self, mask: u32) {
        let data = ECL_M_BITS << 3 | mask;
        let mut rem = data;
        for _ in 0..10 {
            rem = (rem << 1) ^ ((rem >> 9) * 0x537);
        }
        let bits = (data << 10 | rem) ^ 0x5412;
        let bit = |i: usize| (bits >> i) & 1 != 0;
        let s = self.size;
        for i in 0..=5 {
            self.set(8, i, bit(i));
        }
        self.set(8, 7, bit(6));
        self.set(8, 8, bit(7));
        self.set(7, 8, bit(8));
        for i in 9..15 {
            self.set(14 - i, 8, bit(i));
        }
        for i in 0..8 {
            self.set(s - 1 - i, 8, bit(i));
        }
        for i in 8..15 {
            self.set(8, s - 15 + i, bit(i));
        }
        self.set(8, s - 8, true);
    }

    fn draw_codewords(&mut self, data: &[u8]) {
        let s = self.size;
        let total = data.len() * 8;
        let mut i = 0;
        let mut right = s as i32 - 1;
        while right >= 1 {
            if right == 6 {
                right = 5;
            }
            for vert in 0..s {
                for j in 0..2 {
                    let x = (right - j) as usize;
                    let upward = (right + 1) & 2 == 0;
                    let y = if upward { s - 1 - vert } else { vert };
                    let idx = y * s + x;
                    if !self.func[idx] && i < total {
                        self.modules[idx] = (data[i >> 3] >> (7 - (i & 7))) & 1 != 0;
                        i += 1;
                    }
                }
            }
            right -= 2;
        }
    }

    fn apply_mask(&mut self, mask: u32) {
        let s = self.size;
        for y in 0..s {
            for x in 0..s {
                let flip = match mask {
                    0 => (x + y) % 2 == 0,
                    1 => y % 2 == 0,
                    2 => x % 3 == 0,
                    3 => (x + y) % 3 == 0,
                    4 => (x / 3 + y / 2) % 2 == 0,
                    5 => x * y % 2 + x * y % 3 == 0,
                    6 => (x * y % 2 + x * y % 3) % 2 == 0,
                    _ => ((x + y) % 2 + x * y % 3) % 2 == 0,
                };
                let i = y * s + x;
                if flip && !self.func[i] {
                    self.modules[i] = !self.modules[i];
                }
            }
        }
    }

    /// Penalidades 1 (sequências), 2 (blocos 2×2), 3 (padrões parecidos com
    /// os localizadores) e 4 (equilíbrio claro/escuro) da norma.
    fn penalty(&self) -> usize {
        let s = self.size;
        let m = |x: usize, y: usize| self.modules[y * s + x];
        let mut p = 0;
        for horizontal in [true, false] {
            for a in 0..s {
                let mut run = 0;
                let mut prev = None;
                let mut line = Vec::with_capacity(s);
                for b in 0..s {
                    let v = if horizontal { m(b, a) } else { m(a, b) };
                    line.push(v);
                    if Some(v) == prev {
                        run += 1;
                    } else {
                        if run >= 5 {
                            p += 3 + run - 5;
                        }
                        run = 1;
                        prev = Some(v);
                    }
                }
                if run >= 5 {
                    p += 3 + run - 5;
                }
                const F: [bool; 11] = [true, false, true, true, true, false, true, false, false, false, false];
                for w in line.windows(11) {
                    if w == F || w.iter().rev().eq(F.iter()) {
                        p += 40;
                    }
                }
            }
        }
        for y in 0..s - 1 {
            for x in 0..s - 1 {
                let c = m(x, y);
                if c == m(x + 1, y) && c == m(x, y + 1) && c == m(x + 1, y + 1) {
                    p += 3;
                }
            }
        }
        let dark = self.modules.iter().filter(|&&d| d).count();
        let total = s * s;
        let k = (dark * 20).abs_diff(total * 10).div_ceil(total) - 1;
        p + k * 10
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn capacities_match_the_standard() {
        // Capacidade em modo byte, nível M (tabela 7 da norma).
        let cap = |v: usize| (data_codewords(v) * 8 - 4 - if v < 10 { 8 } else { 16 }) / 8;
        assert_eq!([cap(1), cap(2), cap(5), cap(10), cap(20), cap(40)], [14, 26, 84, 213, 666, 2331]);
        assert_eq!(max_bytes(), 2331);
    }

    #[test]
    fn alignment_positions_match_the_standard() {
        assert_eq!(alignment_positions(2), vec![6, 18]);
        assert_eq!(alignment_positions(7), vec![6, 22, 38]);
        assert_eq!(alignment_positions(32), vec![6, 34, 60, 86, 112, 138]);
        assert_eq!(alignment_positions(40), vec![6, 30, 58, 86, 114, 142, 170]);
    }

    #[test]
    fn picks_smallest_version_and_has_finders() {
        let q = encode(b"ola").unwrap();
        assert_eq!((q.version, q.size), (1, 21));
        // Canto do localizador: escuro; anel interno: claro.
        assert!(q.get(0, 0) && !q.get(1, 1) && q.get(2, 2));
        assert!(q.get(8, q.size - 8), "módulo escuro fixo");
        assert!(encode(&vec![b'x'; 2332]).is_none());
        assert_eq!(encode(&vec![b'x'; 350]).unwrap().version, 14);
    }
}
