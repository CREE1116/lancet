//! Canonical case folding shared by the indexer and the query planner.
//!
//! Literal matching uses `regex` with `(?i)`, i.e. Unicode *simple* case
//! folding: a text char `d` matches a pattern char `c` iff both are in the same
//! simple-fold orbit. We map every char to a canonical representative of its
//! orbit, so `fold(c) == fold(d)` exactly when they match. Trigrams computed on
//! folded text are therefore a sound filter for case-insensitive literals.

use regex_syntax::hir::{ClassUnicode, ClassUnicodeRange};
use std::cell::RefCell;
use std::collections::HashMap;

thread_local! {
    static CACHE: RefCell<HashMap<char, char>> = RefCell::new(HashMap::new());
}

fn fold_slow(c: char) -> char {
    let mut cls = ClassUnicode::new([ClassUnicodeRange::new(c, c)]);
    cls.case_fold_simple();
    let min = cls.ranges().first().map(|r| r.start()).unwrap_or(c);
    if min.is_ascii() {
        min.to_ascii_lowercase()
    } else {
        min
    }
}

/// True iff `b` is an ASCII byte whose Unicode simple-case-folding orbit
/// contains only ASCII characters (e.g. false for `'k'`/`'s'` because of
/// `\u{212A}` and `\u{017F}`).
pub fn is_ascii_only_orbit(b: u8) -> bool {
    if !b.is_ascii_alphabetic() {
        return true;
    }
    let c = b as char;
    let mut cls = ClassUnicode::new([ClassUnicodeRange::new(c, c)]);
    cls.case_fold_simple();
    cls.ranges().iter().all(|r| r.end().is_ascii())
}

#[inline]
pub fn fold_char(c: char) -> char {
    if c.is_ascii() {
        return c.to_ascii_lowercase();
    }
    let u = c as u32;
    // Caseless scripts that dominate non-ASCII text we care about.
    if (0xAC00..=0xD7A3).contains(&u)
        || (0x3040..=0x30FF).contains(&u)
        || (0x4E00..=0x9FFF).contains(&u)
    {
        return c;
    }
    CACHE.with(|m| *m.borrow_mut().entry(c).or_insert_with(|| fold_slow(c)))
}

/// Fold UTF-8 text (invalid byte sequences are copied unchanged).
pub fn fold_bytes(input: &[u8], out: &mut Vec<u8>) {
    out.reserve(input.len());
    for chunk in input.utf8_chunks() {
        let mut s = chunk.valid();
        while !s.is_empty() {
            let n = s.bytes().position(|b| b >= 0x80).unwrap_or(s.len());
            out.extend(s.as_bytes()[..n].iter().map(u8::to_ascii_lowercase));
            s = &s[n..];
            let mut end = s.len();
            for (i, ch) in s.char_indices() {
                if ch.is_ascii() {
                    end = i;
                    break;
                }
                let mut buf = [0u8; 4];
                out.extend_from_slice(fold_char(ch).encode_utf8(&mut buf).as_bytes());
            }
            s = &s[end..];
        }
        out.extend_from_slice(chunk.invalid());
    }
}

pub fn fold_str(s: &str) -> String {
    s.chars().map(fold_char).collect()
}

/// Sorted, deduplicated trigram keys (3 bytes packed into a `u32`).
pub fn trigrams(folded: &[u8]) -> Vec<u32> {
    let mut v: Vec<u32> = folded
        .windows(3)
        .map(|w| ((w[0] as u32) << 16) | ((w[1] as u32) << 8) | w[2] as u32)
        .collect();
    v.sort_unstable();
    v.dedup();
    v
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn orbits_share_representative() {
        assert_eq!(fold_char('K'), fold_char('\u{212A}')); // Kelvin sign
        assert_eq!(fold_char('s'), fold_char('\u{017F}')); // long s
        assert_eq!(fold_char('É'), fold_char('é'));
        assert_eq!(fold_char('Σ'), fold_char('σ'));
        assert_eq!(fold_char('Σ'), fold_char('ς'));
        assert_eq!(fold_char('학'), '학');
        assert!(!is_ascii_only_orbit(b'k'));
        assert!(!is_ascii_only_orbit(b's'));
        assert!(is_ascii_only_orbit(b'a'));
        assert!(is_ascii_only_orbit(b'_'));
    }
}
