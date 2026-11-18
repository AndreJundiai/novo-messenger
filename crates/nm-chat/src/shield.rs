//! Escudo contra golpes (docs/PESQUISA.md, seção 7.1).
//!
//! Tudo roda no aparelho, por regras simples e explicáveis; nada é enviado
//! para lugar nenhum e o texto analisado não é guardado.
//!
//! - [`money_request`]: a mensagem fala de Pix, transferência, código etc.
//! - [`similar_names`]: dois contatos com o mesmo nome (ou quase), como no
//!   golpe do "troquei de número".

/// Minúsculas, sem acento, só letras, dígitos e espaços simples.
pub fn normalize(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut space = true;
    for c in s.chars().flat_map(char::to_lowercase) {
        let c = match c {
            'á' | 'à' | 'â' | 'ã' | 'ä' => 'a',
            'é' | 'è' | 'ê' | 'ë' => 'e',
            'í' | 'ì' | 'î' | 'ï' => 'i',
            'ó' | 'ò' | 'ô' | 'õ' | 'ö' => 'o',
            'ú' | 'ù' | 'û' | 'ü' => 'u',
            'ç' => 'c',
            'ñ' => 'n',
            c => c,
        };
        if c.is_alphanumeric() || c == '$' {
            out.push(c);
            space = false;
        } else if !space {
            out.push(' ');
            space = true;
        }
    }
    if out.ends_with(' ') {
        out.pop();
    }
    out
}

/// Palavras que, sozinhas, indicam assunto de dinheiro (comparadas por inteiro).
const WORDS: &[&str] = &["pix", "boleto", "dinheiro", "grana", "r$", "reais", "deposito", "ted", "doc"];
/// Começos de palavra: "transfer" pega transferência, transfere, transferir…
const STEMS: &[&str] = &["transfer", "deposit", "emprest", "pagament", "empresta"];
/// Frases.
const PHRASES: &[&str] = &[
    "chave pix",
    "codigo de verificacao",
    "codigo que chegou",
    "codigo que vai chegar",
    "codigo de seis",
    "codigo de 6",
    "manda o codigo",
    "me passa o codigo",
    "conta bancaria",
    "dados do cartao",
    "numero do cartao",
    "senha do banco",
    "senha do cartao",
    "troquei de numero",
    "numero novo",
];

/// A mensagem parece pedir dinheiro, dados bancários ou código de acesso.
pub fn money_request(text: &str) -> bool {
    let n = normalize(text);
    if PHRASES.iter().any(|p| n.contains(p)) {
        return true;
    }
    let words: Vec<&str> = n.split(' ').collect();
    words.iter().any(|w| WORDS.contains(w) || STEMS.iter().any(|s| w.starts_with(s)))
        || words.windows(2).any(|w| w[0].chars().all(|c| c.is_ascii_digit()) && w[1] == "reais")
}

fn levenshtein(a: &[char], b: &[char]) -> usize {
    let mut prev: Vec<usize> = (0..=b.len()).collect();
    for (i, ca) in a.iter().enumerate() {
        let mut cur = vec![i + 1; b.len() + 1];
        for (j, cb) in b.iter().enumerate() {
            cur[j + 1] = (prev[j] + usize::from(ca != cb)).min(prev[j + 1] + 1).min(cur[j] + 1);
        }
        prev = cur;
    }
    prev[b.len()]
}

/// Mesmo nome ignorando acentos, maiúsculas e espaços, ou a uma letra de diferença.
pub fn similar_names(a: &str, b: &str) -> bool {
    let a: Vec<char> = normalize(a).chars().filter(|c| *c != ' ').collect();
    let b: Vec<char> = normalize(b).chars().filter(|c| *c != ' ').collect();
    if a.is_empty() || b.is_empty() {
        return false;
    }
    a == b || (a.len().min(b.len()) >= 3 && levenshtein(&a, &b) <= 1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_accents_case_and_punctuation() {
        assert_eq!(normalize("  Olá,  MÃE!! Transferência…  "), "ola mae transferencia");
        assert_eq!(normalize("R$ 500,00"), "r$ 500 00");
    }

    #[test]
    fn detects_money_requests() {
        for t in [
            "me manda um pix de 500",
            "Mãe, troquei de número, salva esse",
            "faz uma TRANSFERÊNCIA pra mim urgente",
            "consegue me emprestar 200 reais?",
            "qual sua chave pix",
            "me passa o código que chegou no seu celular",
            "paga esse boleto pra mim",
            "deposita na minha conta",
            "R$ 1.500 até amanhã",
        ] {
            assert!(money_request(t), "deveria alertar: {t}");
        }
    }

    #[test]
    fn ignores_normal_conversation() {
        for t in [
            "comprei pão",
            "o pixel do celular quebrou",
            "vamos cortar o cabelo às 15h?",
            "transmissão do jogo começou",
            "tudo bem com você?",
            "chegou o código do curso de inglês? brincadeira, chegou o livro",
        ] {
            assert!(!money_request(t), "não deveria alertar: {t}");
        }
    }

    #[test]
    fn similar_names_catch_impersonation() {
        assert!(similar_names("Mãe", "mae"));
        assert!(similar_names("Maria Silva", "mariasilva"));
        assert!(similar_names("Joana", "Joanna"));
        assert!(!similar_names("Ana", "Bruno"));
        assert!(!similar_names("Lu", "Li"), "nomes curtos demais só batem se iguais");
        assert!(!similar_names("", "Ana"));
    }
}
