//! Per-file evaluation of a compiled query. This is the semantic oracle: the
//! index never decides a result, it only skips files this function would
//! reject anyway.

use crate::code;
use crate::matcher::{for_each_token, CMatcher};
use crate::query::{Expr, Matcher, Query, Unit};
use std::collections::HashMap;
use std::path::Path;

pub enum CExpr {
    M(usize),
    And(Vec<CExpr>),
    Or(Vec<CExpr>),
    Not(Box<CExpr>),
    Near {
        a: usize,
        b: usize,
        dist: u32,
        ordered: bool,
    },
}

pub struct Compiled {
    pub matchers: Vec<CMatcher>,
    pub expr: CExpr,
    /// Matcher occurs outside any NOT (its matches are shown as hit lines).
    pub positive: Vec<bool>,
    pub unit: Unit,
    /// The expression is true for a unit with no matches at all (e.g. `NOT "x"`).
    pub empty_true: bool,
}

impl Compiled {
    pub fn new(q: &Query) -> Result<Self, String> {
        let mut ids = HashMap::new();
        let mut ms = Vec::new();
        let mut positive = Vec::new();
        let expr = lower(&q.expr, &mut ids, &mut ms, &mut positive, false);
        let matchers = ms
            .iter()
            .map(CMatcher::compile)
            .collect::<Result<Vec<_>, _>>()?;
        let empty_true = eval_empty(&expr);
        Ok(Compiled {
            matchers,
            expr,
            positive,
            unit: q.unit,
            empty_true,
        })
    }
}

fn intern(
    m: &Matcher,
    ids: &mut HashMap<Matcher, usize>,
    ms: &mut Vec<Matcher>,
    pos: &mut Vec<bool>,
    neg: bool,
) -> usize {
    if let Some(&i) = ids.get(m) {
        pos[i] |= !neg;
        return i;
    }
    let i = ms.len();
    ids.insert(m.clone(), i);
    ms.push(m.clone());
    pos.push(!neg);
    i
}

fn lower(
    e: &Expr,
    ids: &mut HashMap<Matcher, usize>,
    ms: &mut Vec<Matcher>,
    pos: &mut Vec<bool>,
    neg: bool,
) -> CExpr {
    match e {
        Expr::Match { m } => CExpr::M(intern(m, ids, ms, pos, neg)),
        Expr::And { args } => {
            CExpr::And(args.iter().map(|a| lower(a, ids, ms, pos, neg)).collect())
        }
        Expr::Or { args } => CExpr::Or(args.iter().map(|a| lower(a, ids, ms, pos, neg)).collect()),
        Expr::Not { arg } => CExpr::Not(Box::new(lower(arg, ids, ms, pos, !neg))),
        Expr::Near {
            a,
            b,
            dist,
            ordered,
        } => CExpr::Near {
            a: intern(a, ids, ms, pos, neg),
            b: intern(b, ids, ms, pos, neg),
            dist: *dist,
            ordered: *ordered,
        },
    }
}

fn eval_empty(e: &CExpr) -> bool {
    match e {
        CExpr::M(_) | CExpr::Near { .. } => false,
        CExpr::And(v) => v.iter().all(eval_empty),
        CExpr::Or(v) => v.iter().any(eval_empty),
        CExpr::Not(x) => !eval_empty(x),
    }
}

#[derive(Debug, Clone)]
pub struct UnitHit {
    pub start: usize,
    pub end: usize,
    pub start_line: u32,
    pub end_line: u32,
    /// 1-based line numbers containing matches of positive matchers.
    pub match_lines: Vec<u32>,
    pub symbol: Option<String>,
}

pub struct FileEval {
    pub hits: Vec<UnitHit>,
    /// Byte offset of each line start (only filled when there are hits).
    pub line_starts: Vec<usize>,
}

struct Ctx<'a> {
    c: &'a Compiled,
    path: &'a Path,
    hay: &'a [u8],
    matches: Vec<Option<Vec<(usize, usize)>>>,
    presence: Vec<Option<bool>>,
    tokens: Option<Vec<usize>>,
}

impl<'a> Ctx<'a> {
    fn new(c: &'a Compiled, path: &'a Path, hay: &'a [u8]) -> Self {
        let n = c.matchers.len();
        Ctx {
            c,
            path,
            hay,
            matches: vec![None; n],
            presence: vec![None; n],
            tokens: None,
        }
    }

    fn ensure(&mut self, i: usize) {
        if self.matches[i].is_none() {
            let mut v = Vec::new();
            self.c.matchers[i].find_all(self.path, self.hay, &mut v);
            self.matches[i] = Some(v);
        }
    }

    fn m(&self, i: usize) -> &[(usize, usize)] {
        self.matches[i].as_deref().unwrap_or(&[])
    }

    fn present_file(&mut self, i: usize) -> bool {
        if let Some(m) = &self.matches[i] {
            return !m.is_empty();
        }
        if let Some(p) = self.presence[i] {
            return p;
        }
        let p = self.c.matchers[i].is_match(self.path, self.hay);
        self.presence[i] = Some(p);
        p
    }

    /// Over-approximation: can any unit of this file satisfy `e`?
    fn possible(&mut self, e: &CExpr) -> bool {
        match e {
            CExpr::M(i) => self.present_file(*i),
            CExpr::And(v) => v.iter().all(|x| self.possible(x)),
            CExpr::Or(v) => v.iter().any(|x| self.possible(x)),
            CExpr::Not(_) => true,
            CExpr::Near { a, b, .. } => self.present_file(*a) && self.present_file(*b),
        }
    }

    /// Exact evaluation over the whole file.
    fn eval_file(&mut self, e: &CExpr) -> bool {
        match e {
            CExpr::M(i) => self.present_file(*i),
            CExpr::And(v) => v.iter().all(|x| self.eval_file(x)),
            CExpr::Or(v) => v.iter().any(|x| self.eval_file(x)),
            CExpr::Not(x) => !self.eval_file(x),
            CExpr::Near {
                a,
                b,
                dist,
                ordered,
            } => {
                if !(self.present_file(*a) && self.present_file(*b)) {
                    return false;
                }
                self.ensure(*a);
                self.ensure(*b);
                self.near_in(*a, *b, *dist, *ordered, 0, self.hay.len())
            }
        }
    }

    /// Exact evaluation over `[s, e)`; all matchers must be `ensure`d.
    fn eval_range(&mut self, x: &CExpr, s: usize, e: usize) -> bool {
        match x {
            CExpr::M(i) => {
                let m = self.m(*i);
                let k = m.partition_point(|p| p.0 < s);
                k < m.len() && m[k].0 < e && m[k].1 <= e
            }
            CExpr::And(v) => v.iter().all(|y| self.eval_range(y, s, e)),
            CExpr::Or(v) => v.iter().any(|y| self.eval_range(y, s, e)),
            CExpr::Not(y) => !self.eval_range(y, s, e),
            CExpr::Near {
                a,
                b,
                dist,
                ordered,
            } => self.near_in(*a, *b, *dist, *ordered, s, e),
        }
    }

    fn near_in(
        &mut self,
        a: usize,
        b: usize,
        dist: u32,
        ordered: bool,
        s: usize,
        e: usize,
    ) -> bool {
        if self.tokens.is_none() {
            let mut t = Vec::new();
            for_each_token(self.hay, &mut |ts, _| {
                t.push(ts);
                true
            });
            self.tokens = Some(t);
        }
        let toks = self.tokens.as_deref().unwrap();
        let tok = |off: usize| toks.partition_point(|&t| t <= off).saturating_sub(1) as i64;
        let slice = |m: &'_ [(usize, usize)]| -> Vec<usize> {
            let lo = m.partition_point(|p| p.0 < s);
            let hi = m.partition_point(|p| p.0 < e);
            m[lo..hi].iter().filter(|p| p.1 <= e).map(|p| p.0).collect()
        };
        let (va, vb) = (slice(self.m(a)), slice(self.m(b)));
        if va.is_empty() || vb.is_empty() {
            return false;
        }
        let dist = dist as i64;
        if ordered {
            for &pa in &va {
                let k = vb.partition_point(|&pb| pb <= pa);
                if k < vb.len() && tok(vb[k]) - tok(pa) <= dist {
                    return true;
                }
            }
            false
        } else {
            let (mut i, mut j) = (0, 0);
            while i < va.len() && j < vb.len() {
                if (tok(va[i]) - tok(vb[j])).abs() <= dist {
                    return true;
                }
                if va[i] < vb[j] {
                    i += 1;
                } else {
                    j += 1;
                }
            }
            false
        }
    }

    fn collect_lines(&mut self, s: usize, e: usize, ls: &[usize]) -> Vec<u32> {
        let mut out = Vec::new();
        for i in 0..self.c.matchers.len() {
            if !self.c.positive[i] {
                continue;
            }
            self.ensure(i);
            let m = self.m(i);
            let lo = m.partition_point(|p| p.0 < s);
            for p in &m[lo..] {
                if p.0 >= e {
                    break;
                }
                if p.1 > e {
                    continue;
                }
                out.push(ls.partition_point(|&x| x <= p.0) as u32);
            }
        }
        out.sort_unstable();
        out.dedup();
        out
    }
}

pub fn line_starts(hay: &[u8]) -> Vec<usize> {
    let mut v = vec![0];
    for p in memchr::memchr_iter(b'\n', hay) {
        if p + 1 < hay.len() {
            v.push(p + 1);
        }
    }
    v
}

fn line_end(ls: &[usize], i: usize, len: usize) -> usize {
    ls.get(i + 1).copied().unwrap_or(len)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LineMode {
    /// Caller does not need line numbers or snippets (`| count`, `| files`, `| terms`).
    None,
    /// Line hits only: avoid building a full-file `line_starts` table for `Unit::Line`.
    HitsOnly,
    /// Full `line_starts` table needed (e.g., `Unit::File`, `Unit::Block`, `Unit::Function`, `group file`).
    Full,
}

/// Evaluate `c` on one file's bytes.
pub fn eval_file(c: &Compiled, path: &Path, hay: &[u8], line_mode: LineMode) -> FileEval {
    let mut ctx = Ctx::new(c, path, hay);
    let empty = FileEval {
        hits: vec![],
        line_starts: vec![],
    };
    let need_lines = line_mode != LineMode::None;
    match c.unit {
        Unit::Function => {
            if hay.is_empty() || (!c.empty_true && !ctx.possible(&c.expr)) {
                return empty;
            }
            let Some(spans) = code::extract_functions(path, hay) else {
                return empty;
            };
            if spans.is_empty() {
                return empty;
            }
            for i in 0..c.matchers.len() {
                ctx.ensure(i);
            }
            let ls = if need_lines { line_starts(hay) } else { vec![] };
            let mut hits = Vec::new();
            for sp in spans {
                if !c.empty_true {
                    let any = (0..c.matchers.len()).any(|i| {
                        let m = ctx.m(i);
                        let k = m.partition_point(|p| p.0 < sp.start);
                        k < m.len() && m[k].0 < sp.end && m[k].1 <= sp.end
                    });
                    if !any {
                        continue;
                    }
                }
                if ctx.eval_range(&c.expr, sp.start, sp.end) {
                    let (start_line, end_line, match_lines) = if need_lines {
                        let sl = (ls.partition_point(|&x| x <= sp.start) as u32).max(1);
                        let last_byte = sp.end.saturating_sub(1).max(sp.start);
                        let el = (ls.partition_point(|&x| x <= last_byte) as u32).max(sl);
                        (sl, el, ctx.collect_lines(sp.start, sp.end, &ls))
                    } else {
                        (0, 0, vec![])
                    };
                    hits.push(UnitHit {
                        start: sp.start,
                        end: sp.end,
                        start_line,
                        end_line,
                        match_lines,
                        symbol: sp.name,
                    });
                }
            }
            FileEval {
                hits,
                line_starts: ls,
            }
        }
        Unit::File => {
            if !ctx.eval_file(&c.expr) {
                return empty;
            }
            if !need_lines {
                let hit = UnitHit {
                    start: 0,
                    end: hay.len(),
                    start_line: 0,
                    end_line: 0,
                    match_lines: vec![],
                    symbol: None,
                };
                return FileEval {
                    hits: vec![hit],
                    line_starts: vec![],
                };
            }
            let ls = line_starts(hay);
            let match_lines = ctx.collect_lines(0, hay.len(), &ls);
            let hit = UnitHit {
                start: 0,
                end: hay.len(),
                start_line: 1,
                end_line: ls.len() as u32,
                match_lines,
                symbol: None,
            };
            FileEval {
                hits: vec![hit],
                line_starts: ls,
            }
        }
        Unit::Line if !c.empty_true && line_mode != LineMode::Full => {
            if hay.is_empty() || !ctx.possible(&c.expr) {
                return empty;
            }
            for i in 0..c.matchers.len() {
                ctx.ensure(i);
            }
            let mut cand_lines: Vec<(usize, usize)> = Vec::new();
            for i in 0..c.matchers.len() {
                for &(p0, _) in ctx.m(i) {
                    if p0 >= hay.len() {
                        continue;
                    }
                    let s = memchr::memrchr(b'\n', &hay[..p0])
                        .map(|p| p + 1)
                        .unwrap_or(0);
                    let e = memchr::memchr(b'\n', &hay[p0..])
                        .map(|p| p0 + p + 1)
                        .unwrap_or(hay.len());
                    cand_lines.push((s, e));
                }
            }
            cand_lines.sort_unstable();
            cand_lines.dedup();

            let mut hits = Vec::new();
            if line_mode == LineMode::None {
                for (s, e) in cand_lines {
                    if ctx.eval_range(&c.expr, s, e) {
                        hits.push(UnitHit {
                            start: s,
                            end: e,
                            start_line: 0,
                            end_line: 0,
                            match_lines: vec![],
                            symbol: None,
                        });
                    }
                }
                return FileEval {
                    hits,
                    line_starts: vec![],
                };
            }

            // LineMode::HitsOnly: count newlines incrementally without allocating `line_starts`.
            let mut cur_off = 0usize;
            let mut cur_line = 1u32;
            for (s, e) in cand_lines {
                if !ctx.eval_range(&c.expr, s, e) {
                    continue;
                }
                cur_line += memchr::memchr_iter(b'\n', &hay[cur_off..s]).count() as u32;
                cur_off = s;
                let has_pos = (0..c.matchers.len()).any(|i| {
                    if !c.positive[i] {
                        return false;
                    }
                    let m = ctx.m(i);
                    let k = m.partition_point(|p| p.0 < s);
                    k < m.len() && m[k].0 < e && m[k].1 <= e
                });
                hits.push(UnitHit {
                    start: s,
                    end: e,
                    start_line: cur_line,
                    end_line: cur_line,
                    match_lines: if has_pos { vec![cur_line] } else { vec![] },
                    symbol: None,
                });
            }
            FileEval {
                hits,
                line_starts: vec![],
            }
        }
        Unit::Line | Unit::Block => {
            if hay.is_empty() {
                return empty;
            }
            if !c.empty_true && !ctx.possible(&c.expr) {
                return empty;
            }
            for i in 0..c.matchers.len() {
                ctx.ensure(i);
            }
            let ls = line_starts(hay);
            // Units as inclusive line-index ranges.
            let (units, line_unit): (Vec<(usize, usize)>, Vec<u32>) = if c.unit == Unit::Line {
                (
                    (0..ls.len()).map(|i| (i, i)).collect(),
                    (0..ls.len() as u32).collect(),
                )
            } else {
                let mut units = Vec::new();
                let mut lu = vec![u32::MAX; ls.len()];
                let mut cur: Option<usize> = None;
                for i in 0..ls.len() {
                    let line = &hay[ls[i]..line_end(&ls, i, hay.len())];
                    let blank = line.iter().all(|b| b.is_ascii_whitespace());
                    if blank {
                        if let Some(st) = cur.take() {
                            units.push((st, i - 1));
                        }
                    } else {
                        if cur.is_none() {
                            cur = Some(i);
                        }
                        lu[i] = units.len() as u32;
                    }
                }
                if let Some(st) = cur {
                    units.push((st, ls.len() - 1));
                }
                (units, lu)
            };
            let cands: Vec<usize> = if c.empty_true {
                (0..units.len()).collect()
            } else {
                let mut v: Vec<usize> = Vec::new();
                for i in 0..c.matchers.len() {
                    for p in ctx.m(i) {
                        let li = ls.partition_point(|&x| x <= p.0) - 1;
                        let u = line_unit[li];
                        if u != u32::MAX {
                            v.push(u as usize);
                        }
                    }
                }
                v.sort_unstable();
                v.dedup();
                v
            };
            let mut hits = Vec::new();
            for u in cands {
                let (a, b) = units[u];
                let (s, e) = (ls[a], line_end(&ls, b, hay.len()));
                if ctx.eval_range(&c.expr, s, e) {
                    let match_lines = if need_lines {
                        ctx.collect_lines(s, e, &ls)
                    } else {
                        vec![]
                    };
                    hits.push(UnitHit {
                        start: s,
                        end: e,
                        start_line: a as u32 + 1,
                        end_line: b as u32 + 1,
                        match_lines,
                        symbol: None,
                    });
                }
            }
            FileEval {
                hits,
                line_starts: ls,
            }
        }
    }
}
