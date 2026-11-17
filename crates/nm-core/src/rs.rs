//! Reed-Solomon sobre GF(2⁸) (polinômio 0x11D, raízes α⁰…α^(nsym-1)).
//!
//! Com `nsym` bytes de paridade, corrige até `nsym / 2` bytes errados em
//! qualquer posição. É o mesmo código usado no QR Code ([`crate::qr`]).
//! Polinômios são guardados do maior grau para o menor.

const fn tables() -> ([u8; 512], [u8; 256]) {
    let mut exp = [0u8; 512];
    let mut log = [0u8; 256];
    let mut x: u16 = 1;
    let mut i = 0;
    while i < 255 {
        exp[i] = x as u8;
        log[x as usize] = i as u8;
        x <<= 1;
        if x & 0x100 != 0 {
            x ^= 0x11D;
        }
        i += 1;
    }
    while i < 512 {
        exp[i] = exp[i - 255];
        i += 1;
    }
    (exp, log)
}

const EXP: [u8; 512] = tables().0;
const LOG: [u8; 256] = tables().1;

#[inline]
pub fn mul(a: u8, b: u8) -> u8 {
    if a == 0 || b == 0 {
        0
    } else {
        EXP[LOG[a as usize] as usize + LOG[b as usize] as usize]
    }
}

#[inline]
fn inv(a: u8) -> u8 {
    debug_assert!(a != 0);
    EXP[255 - LOG[a as usize] as usize]
}

#[inline]
fn div(a: u8, b: u8) -> u8 {
    if a == 0 {
        0
    } else {
        EXP[(LOG[a as usize] as usize + 255 - LOG[b as usize] as usize) % 255]
    }
}

/// α^e, com `e` qualquer inteiro.
#[inline]
fn alpha(e: i64) -> u8 {
    EXP[e.rem_euclid(255) as usize]
}

fn poly_eval(p: &[u8], x: u8) -> u8 {
    p.iter().fold(0, |acc, &c| mul(acc, x) ^ c)
}

fn poly_mul(p: &[u8], q: &[u8]) -> Vec<u8> {
    let mut r = vec![0u8; p.len() + q.len() - 1];
    for (i, &a) in p.iter().enumerate() {
        for (j, &b) in q.iter().enumerate() {
            r[i + j] ^= mul(a, b);
        }
    }
    r
}

fn poly_scale(p: &[u8], k: u8) -> Vec<u8> {
    p.iter().map(|&c| mul(c, k)).collect()
}

fn poly_add(p: &[u8], q: &[u8]) -> Vec<u8> {
    let n = p.len().max(q.len());
    let mut r = vec![0u8; n];
    for (i, &c) in p.iter().enumerate() {
        r[i + n - p.len()] = c;
    }
    for (i, &c) in q.iter().enumerate() {
        r[i + n - q.len()] ^= c;
    }
    r
}

/// Polinômio gerador ∏ (x − αⁱ), i = 0..nsym.
pub fn generator(nsym: usize) -> Vec<u8> {
    (0..nsym).fold(vec![1], |g, i| poly_mul(&g, &[1, alpha(i as i64)]))
}

/// Calcula os `nsym` bytes de paridade de `data`.
pub fn parity(data: &[u8], nsym: usize) -> Vec<u8> {
    let gen = generator(nsym);
    let mut rem = vec![0u8; nsym];
    for &d in data {
        let factor = d ^ rem[0];
        rem.rotate_left(1);
        rem[nsym - 1] = 0;
        if factor != 0 {
            for (r, &g) in rem.iter_mut().zip(&gen[1..]) {
                *r ^= mul(g, factor);
            }
        }
    }
    rem
}

fn syndromes(cw: &[u8], nsym: usize) -> Vec<u8> {
    (0..nsym).map(|i| poly_eval(cw, alpha(i as i64))).collect()
}

/// Corrige `codeword` (dados + paridade) no lugar. Devolve quantos bytes
/// foram corrigidos, ou `None` se houver erros demais.
pub fn correct(codeword: &mut [u8], nsym: usize) -> Option<usize> {
    let n = codeword.len();
    if nsym == 0 || n > 255 || n <= nsym {
        return None;
    }
    let synd = syndromes(codeword, nsym);
    if synd.iter().all(|&s| s == 0) {
        return Some(0);
    }

    // Berlekamp–Massey: localizador de erros.
    let mut err_loc = vec![1u8];
    let mut old_loc = vec![1u8];
    for k in 0..nsym {
        let mut delta = synd[k];
        for j in 1..err_loc.len() {
            delta ^= mul(err_loc[err_loc.len() - 1 - j], synd[k - j]);
        }
        old_loc.push(0);
        if delta != 0 {
            if old_loc.len() > err_loc.len() {
                let new_loc = poly_scale(&old_loc, delta);
                old_loc = poly_scale(&err_loc, inv(delta));
                err_loc = new_loc;
            }
            err_loc = poly_add(&err_loc, &poly_scale(&old_loc, delta));
        }
    }
    while err_loc.len() > 1 && err_loc[0] == 0 {
        err_loc.remove(0);
    }
    let errs = err_loc.len() - 1;
    if errs * 2 > nsym {
        return None;
    }

    // Chien: posições onde o localizador zera.
    let rev: Vec<u8> = err_loc.iter().rev().copied().collect();
    let pos: Vec<usize> = (0..n).filter(|&i| poly_eval(&rev, alpha(i as i64)) == 0).map(|i| n - 1 - i).collect();
    if pos.len() != errs {
        return None;
    }

    // Forney: magnitude de cada erro.
    let coef: Vec<usize> = pos.iter().map(|&p| n - 1 - p).collect();
    let loc = coef.iter().fold(vec![1u8], |l, &c| poly_mul(&l, &[alpha(c as i64), 1]));
    let mut padded = vec![0u8];
    padded.extend_from_slice(&synd);
    padded.reverse();
    let prod = poly_mul(&padded, &loc);
    let eval_rev: Vec<u8> = prod[prod.len() - loc.len()..].to_vec();
    let x: Vec<u8> = coef.iter().map(|&c| alpha(c as i64)).collect();
    for (i, &xi) in x.iter().enumerate() {
        let xi_inv = inv(xi);
        let denom = x.iter().enumerate().filter(|&(j, _)| j != i).fold(1u8, |acc, (_, &xj)| mul(acc, 1 ^ mul(xi_inv, xj)));
        if denom == 0 {
            return None;
        }
        let y = mul(xi, poly_eval(&eval_rev, xi_inv));
        codeword[pos[i]] ^= div(y, denom);
    }
    syndromes(codeword, nsym).iter().all(|&s| s == 0).then_some(errs)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::uart::tests::Lcg;

    fn codeword(data: &[u8], nsym: usize) -> Vec<u8> {
        let mut cw = data.to_vec();
        cw.extend(parity(data, nsym));
        cw
    }

    #[test]
    fn known_vector_matches_qr_spec() {
        // Exemplo clássico do QR "HELLO WORLD" 1-M: 16 bytes de dados, 10 de correção.
        let data = [32, 91, 11, 120, 209, 114, 220, 77, 67, 64, 236, 17, 236, 17, 236, 17];
        assert_eq!(parity(&data, 10), [196, 35, 39, 119, 235, 215, 231, 226, 93, 23]);
    }

    #[test]
    fn corrects_up_to_half_parity_random() {
        let mut rng = Lcg(42);
        for round in 0..400 {
            let len = 5 + (rng.next() * 150.0) as usize;
            let nsym = 8 + 2 * (rng.next() * 16.0) as usize;
            let data: Vec<u8> = (0..len).map(|_| (rng.next() * 256.0) as u8).collect();
            let clean = codeword(&data, nsym);
            let mut cw = clean.clone();
            let nerr = (rng.next() * (nsym / 2 + 1) as f64) as usize;
            let mut hit = std::collections::HashSet::new();
            while hit.len() < nerr {
                hit.insert((rng.next() * cw.len() as f64) as usize);
            }
            for &p in &hit {
                cw[p] ^= 1 + (rng.next() * 255.0) as u8;
            }
            assert_eq!(correct(&mut cw, nsym), Some(nerr), "rodada {round}");
            assert_eq!(cw, clean, "rodada {round}");
        }
    }

    #[test]
    fn too_many_errors_is_detected_not_miscorrected_usually() {
        let mut rng = Lcg(7);
        let data: Vec<u8> = (0..100).map(|i| i as u8).collect();
        let clean = codeword(&data, 16);
        let mut wrong = 0;
        for _ in 0..200 {
            let mut cw = clean.clone();
            for _ in 0..20 {
                let p = (rng.next() * cw.len() as f64) as usize;
                cw[p] ^= 1 + (rng.next() * 255.0) as u8;
            }
            if correct(&mut cw, 16).is_some() && cw != clean {
                wrong += 1;
            }
        }
        // Com 16 de paridade, aceitar uma palavra errada é raríssimo.
        assert!(wrong <= 2, "{wrong} correções erradas");
    }
}
