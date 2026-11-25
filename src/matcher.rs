//! Compiled matchers: produce sorted `(start, end)` byte ranges in a file.

use crate::code;
use crate::fold::{fold_char, fold_str, is_ascii_only_orbit};
use crate::query::Matcher;
use regex::bytes::{Regex, RegexBuilder};
use std::path::Path;

pub enum CMatcher {
    AsciiLit {
        needle: Vec<u8>,
        anchor_idx: usize,
        anchor_lo: u8,
        anchor_hi: u8,
    },
    Re {
        re: Regex,
        /// Sound single-byte SIMD prefilter `(lo, hi)` for `is_match` when a
        /// literal contains `'k'`/`'s'` alongside an ASCII-orbit byte.
        prefilter: Option<(u8, u8)>,
    },
    Fuzzy {
        target: Vec<char>,
        k: usize,
    },
    Symbol {
        target: String,
        prefilter: Regex,
    },
}

/// Rank bytes by ascending rarity in English/code (lower = rarer = better SIMD anchor).
fn byte_rarity(b: u8) -> u8 {
    match b.to_ascii_lowercase() {
        b'z' | b'q' | b'x' | b'j' => 0,
        b'v' | b'k' | b'w' | b'y' | b'b' => 1,
        b'g' | b'p' | b'f' | b'm' | b'u' => 2,
        b'0'..=b'9' => 2,
        b'c' | b'd' | b'l' | b'h' => 3,
        b'r' | b'n' | b'i' | b'o' | b's' => 4,
        b'a' | b't' | b'e' => 5,
        b'_' | b'-' | b'.' | b':' | b'/' => 2,
        b' ' => 6,
        _ => 1,
    }
}

fn best_ascii_anchor(bytes: &[u8]) -> Option<(usize, u8, u8)> {
    let (idx, &b) = bytes
        .iter()
        .enumerate()
        .filter(|&(_, &b)| is_ascii_only_orbit(b))
        .min_by_key(|&(_, &b)| byte_rarity(b))?;
    Some((idx, b.to_ascii_lowercase(), b.to_ascii_uppercase()))
}

impl CMatcher {
    pub fn compile(m: &Matcher) -> Result<Self, String> {
        match m {
            Matcher::Lit { text } => {
                if text.is_ascii() && !text.is_empty() {
                    let b = text.as_bytes();
                    if b.iter().all(|&x| is_ascii_only_orbit(x)) {
                        let (anchor_idx, anchor_lo, anchor_hi) = best_ascii_anchor(b).unwrap();
                        return Ok(CMatcher::AsciiLit {
                            needle: b.to_vec(),
                            anchor_idx,
                            anchor_lo,
                            anchor_hi,
                        });
                    }
                    let prefilter = best_ascii_anchor(b).map(|(_, lo, hi)| (lo, hi));
                    let re = RegexBuilder::new(&regex::escape(text))
                        .case_insensitive(true)
                        .build()
                        .map_err(|e| e.to_string())?;
                    return Ok(CMatcher::Re { re, prefilter });
                }
                RegexBuilder::new(&regex::escape(text))
                    .case_insensitive(true)
                    .build()
                    .map(|re| CMatcher::Re {
                        re,
                        prefilter: None,
                    })
                    .map_err(|e| e.to_string())
            }
            Matcher::Regex { pattern } => RegexBuilder::new(pattern)
                .multi_line(true)
                .size_limit(64 << 20)
                .build()
                .map(|re| CMatcher::Re {
                    re,
                    prefilter: None,
                })
                .map_err(|e| format!("invalid regex: {e}")),
            Matcher::Fuzzy { text, k } => Ok(CMatcher::Fuzzy {
                target: text.chars().map(fold_char).collect(),
                k: *k as usize,
            }),
            Matcher::Symbol { name } => {
                let prefilter = RegexBuilder::new(&regex::escape(name))
                    .case_insensitive(true)
                    .build()
                    .map_err(|e| e.to_string())?;
                Ok(CMatcher::Symbol {
                    target: fold_str(name),
                    prefilter,
                })
            }
        }
    }

    pub fn is_match(&self, path: &Path, hay: &[u8]) -> bool {
        match self {
            CMatcher::AsciiLit { .. } => {
                let mut found = false;
                self.ascii_scan(hay, &mut |_, _| {
                    found = true;
                    false
                });
                found
            }
            CMatcher::Re { re, prefilter } => {
                if let Some((lo, hi)) = *prefilter {
                    let has = if lo == hi {
                        memchr::memchr(lo, hay).is_some()
                    } else {
                        memchr::memchr2(lo, hi, hay).is_some()
                    };
                    if !has {
                        return false;
                    }
                }
                re.is_match(hay)
            }
            CMatcher::Fuzzy { .. } => {
                let mut found = false;
                self.fuzzy_scan(hay, &mut |_, _| {
                    found = true;
                    false
                });
                found
            }
            CMatcher::Symbol { target, prefilter } => {
                if !prefilter.is_match(hay) {
                    return false;
                }
                code::extract_symbols(path, hay).is_some_and(|spans| {
                    spans
                        .iter()
                        .any(|sp| sp.name.as_deref().is_some_and(|n| fold_str(n) == *target))
                })
            }
        }
    }

    pub fn find_all(&self, path: &Path, hay: &[u8], out: &mut Vec<(usize, usize)>) {
        match self {
            CMatcher::AsciiLit { .. } => self.ascii_scan(hay, &mut |s, e| {
                out.push((s, e));
                true
            }),
            CMatcher::Re { re, prefilter } => {
                if let Some((lo, hi)) = *prefilter {
                    let has = if lo == hi {
                        memchr::memchr(lo, hay).is_some()
                    } else {
                        memchr::memchr2(lo, hi, hay).is_some()
                    };
                    if !has {
                        return;
                    }
                }
                out.extend(re.find_iter(hay).map(|m| (m.start(), m.end())));
            }
            CMatcher::Fuzzy { .. } => self.fuzzy_scan(hay, &mut |s, e| {
                out.push((s, e));
                true
            }),
            CMatcher::Symbol { target, prefilter } => {
                if !prefilter.is_match(hay) {
                    return;
                }
                if let Some(spans) = code::extract_symbols(path, hay) {
                    for sp in spans {
                        if sp.name.as_deref().is_some_and(|n| fold_str(n) == *target) {
                            out.push((sp.name_start, sp.name_end));
                        }
                    }
                }
            }
        }
    }

    fn ascii_scan(&self, hay: &[u8], f: &mut dyn FnMut(usize, usize) -> bool) {
        let CMatcher::AsciiLit {
            needle,
            anchor_idx,
            anchor_lo,
            anchor_hi,
        } = self
        else {
            return;
        };
        let nlen = needle.len();
        let tail = nlen - 1 - *anchor_idx;
        let single = *anchor_lo == *anchor_hi;
        let mut pos = 0usize;
        while pos + nlen <= hay.len() {
            let slice = &hay[pos + *anchor_idx..hay.len() - tail];
            let found = if single {
                memchr::memchr(*anchor_lo, slice)
            } else {
                memchr::memchr2(*anchor_lo, *anchor_hi, slice)
            };
            let Some(rel) = found else { break };
            let start = pos + rel;
            if hay[start..start + nlen].eq_ignore_ascii_case(needle) {
                if !f(start, start + nlen) {
                    return;
                }
                pos = start + nlen;
            } else {
                pos = start + 1;
            }
        }
    }

    /// Calls `f` for each matching token; stops when `f` returns false.
    fn fuzzy_scan(&self, hay: &[u8], f: &mut dyn FnMut(usize, usize) -> bool) {
        let CMatcher::Fuzzy { target, k } = self else {
            return;
        };
        let (tlen, k) = (target.len(), *k);
        let mut buf: Vec<char> = Vec::with_capacity(64);
        for_each_token(hay, &mut |s, e| {
            let tok = &hay[s..e];
            // Cheap length prefilter (byte length >= char length).
            if tok.len() + k < tlen {
                return true;
            }
            buf.clear();
            buf.extend(String::from_utf8_lossy(tok).chars().map(fold_char));
            if buf.len().abs_diff(tlen) > k {
                return true;
            }
            if levenshtein_within(&buf, target, k) {
                return f(s, e);
            }
            true
        });
    }
}

#[inline]
fn is_word_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

/// Visit byte ranges of word tokens (`[\p{Alphanumeric}_]+`). Stops when `f`
/// returns false.
pub fn for_each_token(hay: &[u8], f: &mut dyn FnMut(usize, usize) -> bool) {
    let base = hay.as_ptr() as usize;
    let mut start: Option<usize> = None;
    for chunk in hay.utf8_chunks() {
        let v = chunk.valid();
        let off = v.as_ptr() as usize - base;
        for (i, c) in v.char_indices() {
            let w = if c.is_ascii() {
                c.is_ascii_alphanumeric() || c == '_'
            } else {
                is_word_char(c)
            };
            match (w, start) {
                (true, None) => start = Some(off + i),
                (false, Some(s)) => {
                    start = None;
                    if !f(s, off + i) {
                        return;
                    }
                }
                _ => {}
            }
        }
        if !chunk.invalid().is_empty() {
            if let Some(s) = start.take() {
                if !f(s, off + v.len()) {
                    return;
                }
            }
        }
    }
    if let Some(s) = start {
        f(s, hay.len());
    }
}

fn levenshtein_within(a: &[char], b: &[char], k: usize) -> bool {
    let mut prev: Vec<usize> = (0..=b.len()).collect();
    let mut cur = vec![0usize; b.len() + 1];
    for (i, &ca) in a.iter().enumerate() {
        cur[0] = i + 1;
        let mut row_min = cur[0];
        for (j, &cb) in b.iter().enumerate() {
            let cost = usize::from(ca != cb);
            cur[j + 1] = (prev[j + 1] + 1).min(cur[j] + 1).min(prev[j] + cost);
            row_min = row_min.min(cur[j + 1]);
        }
        if row_min > k {
            return false;
        }
        std::mem::swap(&mut prev, &mut cur);
    }
    prev[b.len()] <= k
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ascii_lit_matches_regex_identically() {
        let hays: &[&[u8]] = &[
            b"",
            b"a",
            b"ababa",
            b"ABABA",
            b"xxAbAxAbAx",
            b"definitely_not_in_any_crate_xyz",
            b"prefix DEFINITELY_NOT_IN_ANY_CRATE_XYZ suffix",
            b"z\x00z\xffaba",
        ];
        for pat in [
            "a",
            "aba",
            "definitely_not_in_any_crate_xyz",
            "crate_xyz",
            "z",
        ] {
            let cm = CMatcher::compile(&Matcher::Lit { text: pat.into() }).unwrap();
            assert!(
                matches!(cm, CMatcher::AsciiLit { .. }),
                "expected AsciiLit for {pat}"
            );
            let re = RegexBuilder::new(&regex::escape(pat))
                .case_insensitive(true)
                .build()
                .unwrap();
            for hay in hays {
                assert_eq!(
                    cm.is_match(Path::new("x.txt"), hay),
                    re.is_match(hay),
                    "pat={pat}"
                );
                let mut got = Vec::new();
                cm.find_all(Path::new("x.txt"), hay, &mut got);
                let exp: Vec<(usize, usize)> =
                    re.find_iter(hay).map(|m| (m.start(), m.end())).collect();
                assert_eq!(got, exp, "pat={pat}");
            }
        }
    }
}
