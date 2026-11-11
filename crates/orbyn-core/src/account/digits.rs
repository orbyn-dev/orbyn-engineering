//! The six digits that finish a new device's sign-in: a trusted device shows them after it
//! allows the request, and the new device types them. The rule lives here, so the stand-in
//! that plays the account server and any other holder of digits apply the same one:
//!
//! - **Single use.** Once the right digits are accepted they are spent; nothing accepts them
//!   again.
//! - **Bounded tries.** [`TRIES`] wrong entries close the request. A wrong entry never says
//!   which digit was wrong.
//! - **Constant-time compare.** The typed digits are compared with every digit looked at, so
//!   the time taken does not say how many leading digits were right.
//! - **Ten minutes.** The digits die with the request ([`LIFETIME`]).
//!
//! Neither the issued nor the typed digits are logged, formatted or returned: [`Issued`]
//! prints as `[redacted]`.

use std::fmt;
use std::time::{Duration, Instant};

use zeroize::Zeroize;

/// How many wrong entries close the request. After the first wrong one, two tries are left.
pub const TRIES: u8 = 3;
/// How long a request, and so its digits, lasts.
pub const LIFETIME: Duration = Duration::from_secs(10 * 60);
pub const LEN: usize = 6;

/// What checking one entry found.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Check {
    /// Right, and now spent.
    Accepted,
    /// Wrong; this many tries are left.
    Wrong { tries_left: u8 },
    /// The tries ran out (now, or before), or the digits were already used.
    Locked,
    /// The ten minutes are over.
    Expired,
}

/// Six digits issued for one request, with the tries left and when they die.
pub struct Issued {
    digits: String,
    tries_left: u8,
    spent: bool,
    expires: Instant,
}

impl Issued {
    /// Issues `digits` (six ASCII digits) for `life` from now.
    pub fn new(digits: String, life: Duration) -> Self {
        Self {
            digits,
            tries_left: TRIES,
            spent: false,
            expires: Instant::now() + life,
        }
    }

    /// Six random digits, from the operating system's generator. Each digit is drawn without
    /// modulo bias.
    pub fn random_digits() -> String {
        let mut digits = String::with_capacity(LEN);
        let mut byte = [0_u8; 1];
        while digits.len() < LEN {
            // 250 is the largest multiple of ten that fits in a byte: bytes above it are redrawn.
            if getrandom::fill(&mut byte).is_err() {
                // No randomness means no digits: a fixed run would be guessable, so fail closed
                // with a value no six-digit entry can equal.
                digits.zeroize();
                return String::new();
            }
            if byte[0] < 250 {
                digits.push(char::from(b'0' + byte[0] % 10));
            }
        }
        digits
    }

    /// Checks one typed entry. It never says which digit was wrong, and it uses a try only
    /// when the entry is wrong.
    pub fn check(&mut self, typed: &str) -> Check {
        if self.spent || self.tries_left == 0 || self.digits.is_empty() {
            return Check::Locked;
        }
        if Instant::now() >= self.expires {
            return Check::Expired;
        }
        if constant_time_eq(self.digits.as_bytes(), typed.as_bytes()) {
            self.spent = true;
            self.digits.zeroize();
            return Check::Accepted;
        }
        self.tries_left -= 1;
        if self.tries_left == 0 {
            self.digits.zeroize();
            Check::Locked
        } else {
            Check::Wrong {
                tries_left: self.tries_left,
            }
        }
    }

    /// The digits for the trusted device to show; `None` once they are spent or locked.
    pub fn shown(&self) -> Option<&str> {
        (!self.spent && self.tries_left > 0).then_some(self.digits.as_str())
    }

    pub fn tries_left(&self) -> u8 {
        self.tries_left
    }
}

impl Drop for Issued {
    fn drop(&mut self) {
        self.digits.zeroize();
    }
}

impl fmt::Debug for Issued {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Issued([redacted])")
    }
}

/// Six ASCII digits, nothing else: what a typed entry has to be before it is checked or counts.
pub fn is_well_formed(typed: &str) -> bool {
    typed.len() == LEN && typed.bytes().all(|byte| byte.is_ascii_digit())
}

/// Spaces and hyphens ignored, as pasted from "418 207".
pub fn clean(typed: &str) -> String {
    typed
        .chars()
        .filter(|c| !c.is_whitespace() && *c != '-')
        .collect()
}

/// Compares two byte strings looking at every byte of both, whatever the first difference.
fn constant_time_eq(left: &[u8], right: &[u8]) -> bool {
    let mut difference = left.len() ^ right.len();
    for index in 0..left.len().max(right.len()) {
        let a = left.get(index).copied().unwrap_or(0);
        let b = right.get(index).copied().unwrap_or(0);
        difference |= usize::from(a ^ b);
    }
    difference == 0
}

#[cfg(test)]
mod tests {
    use super::*;

    fn issued() -> (Issued, String) {
        let digits = Issued::random_digits();
        (Issued::new(digits.clone(), LIFETIME), digits)
    }

    /// Every digit changed, so it can never equal `right`.
    fn wrong_for(right: &str) -> String {
        right
            .bytes()
            .map(|byte| char::from(b'0' + (byte - b'0' + 1) % 10))
            .collect()
    }

    #[test]
    fn random_digits_are_six_digits_and_differ_between_calls() {
        let first = Issued::random_digits();
        assert!(is_well_formed(&first));
        // Two draws are equal one time in a million; twenty equal draws never happen.
        assert!((0..20).any(|_| Issued::random_digits() != first));
    }

    #[test]
    fn the_right_digits_are_accepted_once() {
        let (mut issued, right) = issued();
        assert_eq!(issued.check(&right), Check::Accepted);
        assert_eq!(issued.check(&right), Check::Locked);
        assert!(issued.shown().is_none());
    }

    #[test]
    fn three_wrong_entries_lock_it_and_the_right_ones_no_longer_work() {
        let (mut issued, right) = issued();
        let wrong = wrong_for(&right);
        assert_eq!(issued.check(&wrong), Check::Wrong { tries_left: 2 });
        assert_eq!(issued.check(&wrong), Check::Wrong { tries_left: 1 });
        assert_eq!(issued.check(&wrong), Check::Locked);
        assert_eq!(issued.check(&right), Check::Locked);
        assert!(issued.shown().is_none());
    }

    #[test]
    fn a_wrong_entry_before_the_right_one_still_lets_it_in() {
        let (mut issued, right) = issued();
        assert_eq!(
            issued.check(&wrong_for(&right)),
            Check::Wrong { tries_left: 2 }
        );
        assert_eq!(issued.check(&right), Check::Accepted);
    }

    #[test]
    fn digits_die_with_the_request() {
        let right = Issued::random_digits();
        let mut issued = Issued::new(right.clone(), Duration::ZERO);
        assert_eq!(issued.check(&right), Check::Expired);
    }

    #[test]
    fn an_entry_of_the_wrong_length_is_wrong_and_costs_a_try() {
        let (mut issued, right) = issued();
        assert_eq!(issued.check(&right[..5]), Check::Wrong { tries_left: 2 });
        assert_eq!(
            issued.check(&format!("{right}0")),
            Check::Wrong { tries_left: 1 }
        );
    }

    #[test]
    fn the_compare_looks_at_every_byte() {
        assert!(constant_time_eq(b"123456", b"123456"));
        assert!(!constant_time_eq(b"123456", b"123457"));
        assert!(!constant_time_eq(b"123456", b"023456"));
        assert!(!constant_time_eq(b"123456", b"12345"));
        assert!(!constant_time_eq(b"", b"1"));
    }

    #[test]
    fn nothing_prints_the_digits() {
        let (issued, right) = issued();
        assert!(!format!("{issued:?}").contains(&right));
    }

    #[test]
    fn pasted_digits_lose_spaces_and_hyphens() {
        assert_eq!(clean(" 12 34-56 "), "123456");
        assert!(is_well_formed("123456"));
        assert!(!is_well_formed("12345a"));
        assert!(!is_well_formed("1234567"));
    }
}
