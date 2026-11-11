//! The two typed code formats: the invite `XXXX-XXXX-XXXX-XXXX` and the emailed
//! `XXXX-XXXX`. Both use one 30-symbol alphabet with no `0`, `O`, `1`, `I`, `L` or `U`.
//! Input ignores case, hyphens and spaces, and anything else is refused.

/// Digits 2 to 9 and the letters without I, L, O and U: 8 + 22 = 30 symbols.
pub const ALPHABET: &str = "23456789ABCDEFGHJKMNPQRSTVWXYZ";

pub const INVITE_LEN: usize = 16;
pub const EMAIL_CODE_LEN: usize = 8;

fn normalize(input: &str, len: usize) -> Option<String> {
    let code: String = input
        .chars()
        .filter(|c| !c.is_whitespace() && *c != '-')
        .flat_map(char::to_uppercase)
        .collect();
    (code.chars().count() == len && code.chars().all(|c| ALPHABET.contains(c))).then_some(code)
}

/// The invite without hyphens, in capitals, or `None` when it cannot be an invite.
pub fn normalize_invite(input: &str) -> Option<String> {
    normalize(input, INVITE_LEN)
}

pub fn normalize_email_code(input: &str) -> Option<String> {
    normalize(input, EMAIL_CODE_LEN)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_alphabet_has_thirty_symbols_and_none_that_look_alike() {
        assert_eq!(ALPHABET.chars().count(), 30);
        assert!(!ALPHABET.contains(['0', 'O', '1', 'I', 'L', 'U']));
    }

    #[test]
    fn input_ignores_case_hyphens_and_spaces() {
        let want = Some("K7QM2XWH9QXA3C2D".to_owned());
        for input in [
            "K7QM-2XWH-9QXA-3C2D",
            "k7qm-2xwh-9qxa-3c2d",
            "k7qm 2xwh 9qxa 3c2d",
            " K7QM2XWH9QXA3C2D ",
            "k7qm2xwh-9qxa 3c2d",
        ] {
            assert_eq!(normalize_invite(input), want, "{input}");
        }
        assert_eq!(normalize_email_code("abcd-efgh"), Some("ABCDEFGH".into()));
    }

    #[test]
    fn letters_that_are_not_in_the_alphabet_are_refused() {
        for bad in ["I", "L", "O", "U", "0", "1"] {
            let invite = format!("{bad}{}", "ABCD".repeat(4).get(1..).unwrap());
            assert_eq!(normalize_invite(&invite), None, "{bad}");
        }
        assert_eq!(normalize_invite("ABCD-EFGH-JKMN"), None);
        assert_eq!(normalize_invite("ABCD-EFGH-JKMN-PQRST"), None);
        assert_eq!(normalize_email_code("ABCD-EFG"), None);
    }
}
