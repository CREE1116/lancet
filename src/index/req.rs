//! Trigram requirements: a boolean formula over trigram sets that every
//! matching file must satisfy. Derivation is conservative — whenever we are
//! unsure we return `All` (no constraint), so the index can never drop a match.

use super::segment::Segment;
use crate::fold::{fold_char, fold_str, trigrams};
use crate::query::{Expr, Matcher};
use regex_syntax::hir::{Class, Hir, HirKind};
use serde::Serialize;
use std::fmt;

/// Intersect up to this many rarest trigrams per literal; intersecting a dozen
/// posting lists in L2 cache takes microseconds and eliminates false positives.
const MAX_GRAMS_PER_LIT: usize = 12;

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct EvalTrace {
    pub grams_available: usize,
    pub grams_decoded: usize,
    pub index_skipped: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Req {
    All,
    Grams(Vec<u32>),
    /// Sound k-of-n threshold (used for fuzzy terms via the q-gram lemma).
    AtLeast {
        min: usize,
        grams: Vec<u32>,
    },
    And(Vec<Req>),
    Or(Vec<Req>),
}

impl fmt::Display for Req {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Req::All => f.write_str("All"),
            Req::Grams(g) => write!(f, "Grams({})", g.len()),
            Req::AtLeast { min, grams } => write!(f, "AtLeast({min}/{})", grams.len()),
            Req::And(v) => {
                f.write_str("AND(")?;
                for (i, r) in v.iter().enumerate() {
                    if i > 0 {
                        f.write_str(", ")?;
                    }
                    write!(f, "{r}")?;
                }
                f.write_str(")")
            }
            Req::Or(v) => {
                f.write_str("OR(")?;
                for (i, r) in v.iter().enumerate() {
                    if i > 0 {
                        f.write_str(", ")?;
                    }
                    write!(f, "{r}")?;
                }
                f.write_str(")")
            }
        }
    }
}

impl Req {
    pub fn from_expr(e: &Expr) -> Req {
        let r = match e {
            Expr::Match { m } => from_matcher(m),
            Expr::And { args } => Req::And(args.iter().map(Req::from_expr).collect()),
            Expr::Or { args } => Req::Or(args.iter().map(Req::from_expr).collect()),
            Expr::Not { .. } => Req::All,
            Expr::Near { a, b, .. } => Req::And(vec![from_matcher(a), from_matcher(b)]),
        };
        r.simplify()
    }

    pub fn is_all(&self) -> bool {
        matches!(self, Req::All)
    }

    fn simplify(self) -> Req {
        match self {
            Req::Grams(g) if g.is_empty() => Req::All,
            Req::AtLeast { min: 0, .. } => Req::All,
            Req::AtLeast { grams, .. } if grams.is_empty() => Req::All,
            Req::AtLeast { min, grams } if min >= grams.len() => Req::Grams(grams),
            Req::And(v) => {
                let mut out = Vec::new();
                for r in v.into_iter().map(Req::simplify) {
                    match r {
                        Req::All => {}
                        Req::And(inner) => out.extend(inner),
                        r => out.push(r),
                    }
                }
                // Narrowest constraints first so `Grams` can short-circuit `AtLeast`/`Or`.
                out.sort_by_key(|r| match r {
                    Req::Grams(_) => 0,
                    Req::And(_) => 1,
                    Req::AtLeast { .. } => 2,
                    Req::Or(_) => 3,
                    Req::All => 4,
                });
                match out.len() {
                    0 => Req::All,
                    1 => out.pop().unwrap(),
                    _ => Req::And(out),
                }
            }
            Req::Or(v) => {
                let mut out = Vec::new();
                for r in v.into_iter().map(Req::simplify) {
                    match r {
                        Req::All => return Req::All,
                        Req::Or(inner) => out.extend(inner),
                        r => out.push(r),
                    }
                }
                match out.len() {
                    0 => Req::All,
                    1 => out.pop().unwrap(),
                    _ => Req::Or(out),
                }
            }
            r => r,
        }
    }

    /// Candidate local doc ids in `seg` (sorted); `None` means "all docs".
    pub fn eval(&self, seg: &Segment) -> Option<Vec<u32>> {
        let mut trace = EvalTrace::default();
        self.eval_with_trace(seg, &mut trace)
    }

    pub fn eval_with_trace(&self, seg: &Segment, trace: &mut EvalTrace) -> Option<Vec<u32>> {
        match self {
            Req::All => None,
            Req::Grams(keys) => {
                let mut ranges = Vec::with_capacity(keys.len());
                for &k in keys {
                    match seg.posting_range(k) {
                        Some(r) => ranges.push((r, seg.peek_count(r))),
                        None => return Some(vec![]),
                    }
                }
                if ranges.is_empty() {
                    return None;
                }
                ranges.sort_by_key(|&(r, count)| (count, r.1));
                trace.grams_available += ranges.len();

                let total = seg.ndocs().max(1);
                if ranges.len() == 1 && ranges[0].1 * 100 > total * 85 {
                    trace.index_skipped = true;
                    return None;
                }

                ranges.truncate(MAX_GRAMS_PER_LIT);
                let mut acc = seg.decode(ranges[0].0);
                trace.grams_decoded += 1;

                for &(range, _) in &ranges[1..] {
                    if acc.is_empty() {
                        break;
                    }
                    acc = intersect(&acc, &seg.decode(range));
                    trace.grams_decoded += 1;
                }
                Some(acc)
            }
            Req::AtLeast { min, grams } => {
                trace.grams_available += grams.len();
                let mut ranges = Vec::with_capacity(grams.len());
                for &k in grams {
                    if let Some(r) = seg.posting_range(k) {
                        ranges.push(r);
                    }
                }
                if ranges.len() < *min {
                    return Some(vec![]);
                }
                if *min == 1 {
                    let mut acc = Vec::new();
                    for r in ranges {
                        acc = union(&acc, &seg.decode(r));
                        trace.grams_decoded += 1;
                    }
                    return Some(acc);
                }
                let mut counts = vec![0u16; seg.ndocs()];
                for r in ranges {
                    for id in seg.decode(r) {
                        if let Some(c) = counts.get_mut(id as usize) {
                            *c = c.saturating_add(1);
                        }
                    }
                    trace.grams_decoded += 1;
                }
                let target = *min as u16;
                let out = (0..seg.ndocs() as u32)
                    .filter(|&id| counts[id as usize] >= target)
                    .collect();
                Some(out)
            }
            Req::And(v) => {
                let mut acc: Option<Vec<u32>> = None;
                for r in v {
                    if let Some(ids) = r.eval_with_trace(seg, trace) {
                        acc = Some(match acc {
                            None => ids,
                            Some(a) => intersect(&a, &ids),
                        });
                        if acc.as_ref().is_some_and(|a| a.is_empty()) {
                            break;
                        }
                    }
                }
                acc
            }
            Req::Or(v) => {
                let mut acc = Vec::new();
                for r in v {
                    acc = union(&acc, &r.eval_with_trace(seg, trace)?);
                }
                Some(acc)
            }
        }
    }
}

fn intersect(a: &[u32], b: &[u32]) -> Vec<u32> {
    let mut out = Vec::with_capacity(a.len().min(b.len()));
    let (mut i, mut j) = (0, 0);
    while i < a.len() && j < b.len() {
        match a[i].cmp(&b[j]) {
            std::cmp::Ordering::Less => i += 1,
            std::cmp::Ordering::Greater => j += 1,
            std::cmp::Ordering::Equal => {
                out.push(a[i]);
                i += 1;
                j += 1;
            }
        }
    }
    out
}

fn union(a: &[u32], b: &[u32]) -> Vec<u32> {
    let mut out = Vec::with_capacity(a.len() + b.len());
    let (mut i, mut j) = (0, 0);
    while i < a.len() || j < b.len() {
        if j >= b.len() || (i < a.len() && a[i] < b[j]) {
            out.push(a[i]);
            i += 1;
        } else if i >= a.len() || b[j] < a[i] {
            out.push(b[j]);
            j += 1;
        } else {
            out.push(a[i]);
            i += 1;
            j += 1;
        }
    }
    out
}

fn grams_of_folded(s: &str) -> Req {
    Req::Grams(trigrams(s.as_bytes()))
}

fn from_matcher(m: &Matcher) -> Req {
    match m {
        Matcher::Lit { text } | Matcher::Symbol { name: text } => grams_of_folded(&fold_str(text)),
        Matcher::Regex { pattern } => match regex_syntax::ParserBuilder::new()
            .multi_line(true)
            .build()
            .parse(pattern)
        {
            Ok(h) => hir_req(&h),
            Err(_) => Req::All,
        },
        Matcher::Fuzzy { text, k } => {
            let folded = fold_str(text);
            let k = *k as usize;
            if k == 0 {
                return grams_of_folded(&folded);
            }
            if folded.is_ascii() {
                let g = trigrams(folded.as_bytes());
                let min = g.len().saturating_sub(3 * k);
                if min == 0 {
                    Req::All
                } else {
                    Req::AtLeast { min, grams: g }
                }
            } else if !folded.is_empty() && folded.chars().all(|c| c.len_utf8() == 3) {
                let mut g: Vec<u32> = folded
                    .chars()
                    .map(|c| {
                        let mut buf = [0u8; 4];
                        let b = c.encode_utf8(&mut buf).as_bytes();
                        ((b[0] as u32) << 16) | ((b[1] as u32) << 8) | b[2] as u32
                    })
                    .collect();
                g.sort_unstable();
                g.dedup();
                let min = g.len().saturating_sub(k);
                if min == 0 {
                    Req::All
                } else {
                    Req::AtLeast { min, grams: g }
                }
            } else {
                Req::All
            }
        }
    }
}

fn hir_req(h: &Hir) -> Req {
    match h.kind() {
        HirKind::Concat(subs) => {
            let mut parts = Vec::new();
            let mut run = String::new();
            for s in subs {
                if let Some(chars) = as_chars(s) {
                    run.push_str(&chars);
                } else {
                    if !run.is_empty() {
                        parts.push(grams_of_folded(&std::mem::take(&mut run)));
                    }
                    parts.push(hir_req(s));
                }
            }
            if !run.is_empty() {
                parts.push(grams_of_folded(&run));
            }
            Req::And(parts)
        }
        HirKind::Literal(_) => as_chars(h).map(|s| grams_of_folded(&s)).unwrap_or(Req::All),
        HirKind::Capture(c) => hir_req(&c.sub),
        HirKind::Repetition(r) if r.min >= 1 => hir_req(&r.sub),
        HirKind::Alternation(alts) => Req::Or(alts.iter().map(hir_req).collect()),
        _ => Req::All,
    }
}

/// If `h` always matches exactly one fixed string up to case folding, return
/// that string folded.
fn as_chars(h: &Hir) -> Option<String> {
    match h.kind() {
        HirKind::Literal(lit) => std::str::from_utf8(&lit.0).ok().map(fold_str),
        HirKind::Class(Class::Unicode(cu)) => {
            let mut rep: Option<char> = None;
            let mut n = 0u32;
            for r in cu.ranges() {
                n += r.end() as u32 - r.start() as u32 + 1;
                if n > 4 {
                    return None;
                }
                for c in r.start()..=r.end() {
                    let f = fold_char(c);
                    match rep {
                        None => rep = Some(f),
                        Some(x) if x == f => {}
                        Some(_) => return None,
                    }
                }
            }
            rep.map(|c| c.to_string())
        }
        HirKind::Capture(c) => as_chars(&c.sub),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rx(p: &str) -> Req {
        from_matcher(&Matcher::Regex { pattern: p.into() }).simplify()
    }

    #[test]
    fn alternation_is_or_not_dropped() {
        match rx("autophagy|mitochondria") {
            Req::Or(v) => assert_eq!(v.len(), 2),
            r => panic!("{r:?}"),
        }
        assert!(rx("a|bcd").is_all());
        assert!(rx("(foo)?bar").ne(&Req::All));
        assert!(rx("x*").is_all());
    }

    #[test]
    fn case_insensitive_regex_still_uses_grams() {
        assert_eq!(rx("(?i)hello"), Req::Grams(trigrams(b"hello")));
    }

    #[test]
    fn fuzzy_trigram_threshold_lemma() {
        match from_matcher(&Matcher::Fuzzy {
            text: "transmte".into(),
            k: 1,
        })
        .simplify()
        {
            Req::AtLeast { min, grams } => {
                assert_eq!(grams.len(), 6);
                assert_eq!(min, 3);
            }
            other => panic!("unexpected req: {other:?}"),
        }
        assert!(from_matcher(&Matcher::Fuzzy {
            text: "banana".into(),
            k: 1
        })
        .simplify()
        .is_all());
        match from_matcher(&Matcher::Fuzzy {
            text: "학습율".into(),
            k: 1,
        })
        .simplify()
        {
            Req::AtLeast { min, grams } => {
                assert_eq!(grams.len(), 3);
                assert_eq!(min, 2);
            }
            other => panic!("unexpected req: {other:?}"),
        }
    }
}
