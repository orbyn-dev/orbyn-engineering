//! The two typed code formats: the invite `XXXX-XXXX-XXXX-XXXX` and the emailed
//! `XXXX-XXXX`. The emailed code uses a 30-symbol alphabet with no `0`, `O`, `1`, `I`, `L` or `U`; the invite uses the Engine's 32 symbols, which keep `0` and `1`.
//! Input ignores case, hyphens and spaces, and anything else is refused.

/// The emailed code: digits 2 to 9 and the letters without I, L, O and U, 8 + 22 = 30 symbols (the Engine's `email_code`).
pub const ALPHABET: &str = "23456789ABCDEFGHJKMNPQRSTVWXYZ";

/// The invite: Crockford base32, the digits 0 to 9 and the letters without I, L, O and U, 32 symbols. The Engine draws invites from
/// this alphabet (`accounts/invites.rs`), so an invite it issued can hold a 0 or a 1; refusing them here made about two real invites
/// in three fail as `invalid_invite` before the Engine saw them.
pub const INVITE_ALPHABET: &str = "0123456789ABCDEFGHJKMNPQRSTVWXYZ";

pub const INVITE_LEN: usize = 16;
pub const EMAIL_CODE_LEN: usize = 8;

fn normalize(input: &str, len: usize, alphabet: &str) -> Option<String> {
    let code: String = input
        .chars()
        .filter(|c| !c.is_whitespace() && *c != '-')
        .flat_map(char::to_uppercase)
        .collect();
    (code.chars().count() == len && code.chars().all(|c| alphabet.contains(c))).then_some(code)
}

/// The invite without hyphens, in capitals, or `None` when it cannot be an invite.
pub fn normalize_invite(input: &str) -> Option<String> {
    normalize(input, INVITE_LEN, INVITE_ALPHABET)
}

pub fn normalize_email_code(input: &str) -> Option<String> {
    normalize(input, EMAIL_CODE_LEN, ALPHABET)
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
    fn the_invite_alphabet_is_the_engines_thirty_two_symbols() {
        assert_eq!(INVITE_ALPHABET.chars().count(), 32);
        assert!(!INVITE_ALPHABET.contains(['O', 'I', 'L', 'U']));
        assert_eq!(
            normalize_invite("0123-4567-89AB-CDEF").as_deref(),
            Some("0123456789ABCDEF")
        );
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
        for bad in ["I", "L", "O", "U"] {
            let invite = format!("{bad}{}", "ABCD".repeat(4).get(1..).unwrap());
            assert_eq!(normalize_invite(&invite), None, "{bad}");
        }
        for bad in ["0", "1", "I", "L", "O", "U"] {
            assert_eq!(
                normalize_email_code(&format!("{bad}ABCDEFG")),
                None,
                "{bad}"
            );
        }
        assert_eq!(normalize_invite("ABCD-EFGH-JKMN"), None);
        assert_eq!(normalize_invite("ABCD-EFGH-JKMN-PQRST"), None);
        assert_eq!(normalize_email_code("ABCD-EFG"), None);
    }
}
