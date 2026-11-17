//! Escrita mínima de JSON (sem dependências) para entregar eventos ao navegador.

use std::fmt::Write;

pub fn string(out: &mut String, s: &str) {
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => {
                let _ = write!(out, "\\u{:04x}", c as u32);
            }
            c => out.push(c),
        }
    }
    out.push('"');
}

/// Número finito com até 3 casas; NaN/infinito viram 0.
pub fn number(out: &mut String, x: f64) {
    if x.is_finite() {
        let _ = write!(out, "{}", (x * 1000.0).round() / 1000.0);
    } else {
        out.push('0');
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn escapes() {
        let mut s = String::new();
        super::string(&mut s, "a\"b\\c\n\u{1}é");
        assert_eq!(s, r#""a\"b\\c\n\u0001é""#);
    }
}
