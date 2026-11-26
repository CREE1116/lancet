//! Query executor: plans scan vs index, evaluates files in parallel in path
//! order (deterministic output), stops early once `limit` is satisfied.

use crate::code;
use crate::content;
use crate::eval::{eval_file, Compiled, LineMode, UnitHit};
use crate::fold::fold_str;
use crate::index::{self, EvalTrace, Index, Req, FLAG_BINARY, FLAG_ERROR, FLAG_OVERSIZE};
use crate::matcher::for_each_token;
use crate::output::{BudgetInfo, LineOut, Response, ResultItem, Stats, Status, TermStat};
use crate::query::{parse, Expr, Matcher, Query, SortOrder, Unit};
use crate::walk::{walk, FileEntry, FileFilter, WalkOpts};
use rayon::prelude::*;
use serde::Serialize;
use std::cmp::Reverse;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::time::Instant;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IndexMode {
    /// Use the index if present; files changed since indexing are scanned.
    Auto,
    /// Never use the index.
    Off,
    /// Use the index without checking the filesystem for changes (fastest;
    /// new/changed files are missed until the next `lancet index`).
    Trust,
}

#[derive(Clone, Debug)]
pub struct SearchOptions {
    pub root: PathBuf,
    pub index_dir: Option<PathBuf>,
    pub walk: WalkOpts,
    pub index: IndexMode,
    pub ocr: bool,
    pub max_lines: usize,
    pub max_cols: usize,
    pub max_unit_bytes: usize,
}

impl SearchOptions {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        SearchOptions {
            root: root.into(),
            index_dir: None,
            walk: WalkOpts::default(),
            index: IndexMode::Auto,
            ocr: false,
            max_lines: 20,
            max_cols: 240,
            max_unit_bytes: 8192,
        }
    }
    fn index_dir(&self) -> PathBuf {
        self.index_dir
            .clone()
            .unwrap_or_else(|| index::default_dir(&self.root))
    }
}

pub(crate) struct Target {
    pub path: PathBuf,
    pub rel: String,
}

#[derive(Default)]
struct Outcome {
    searched: bool,
    binary: bool,
    error: bool,
    matched_units: usize,
    items: Vec<(ResultItem, usize)>,
    file_terms: HashMap<String, usize>,
}

fn ms(t0: Instant) -> f64 {
    t0.elapsed().as_secs_f64() * 1000.0
}

pub fn search(src: &str, opts: &SearchOptions) -> Response {
    let t0 = Instant::now();
    match parse(src) {
        Ok(q) => search_query(&q, opts, t0),
        Err(e) => Response::failure(Status::InvalidQuery, None, e.message, Some(e.pos), ms(t0)),
    }
}

pub fn search_query(q: &Query, opts: &SearchOptions, t0: Instant) -> Response {
    let ix = open_index(opts);
    execute_one(
        q,
        opts,
        ix.as_ref().map(|o| o.as_ref()).map_err(|s| s.as_str()),
        None,
        t0,
    )
}

#[derive(Serialize, Debug, Clone)]
pub struct BatchItem {
    pub name: String,
    pub response: Response,
}

#[derive(Serialize, Debug, Clone)]
pub struct BatchResponse {
    pub results: Vec<BatchItem>,
    pub elapsed_ms: f64,
}

/// Run multiple named queries sharing a single index open and directory walk.
pub fn search_batch(queries: &[(String, String)], opts: &SearchOptions) -> BatchResponse {
    let t0 = Instant::now();
    let ix = open_index(opts);
    let mut walk_cache: Option<Vec<FileEntry>> = None;
    let need_walk =
        ix.as_ref().ok().and_then(|o| o.as_ref()).is_none() || opts.index != IndexMode::Trust;
    if need_walk {
        let need_meta = matches!((&ix, opts.index), (Ok(Some(_)), IndexMode::Auto));
        walk_cache = Some(walk(&opts.root, &opts.walk, &FileFilter::all(), need_meta));
    }
    let ix_ref = ix.as_ref().map(|o| o.as_ref()).map_err(|s| s.as_str());
    let mut results = Vec::with_capacity(queries.len());
    for (name, src) in queries {
        let qt0 = Instant::now();
        let response = match parse(src) {
            Ok(q) => execute_one(&q, opts, ix_ref, walk_cache.as_deref(), qt0),
            Err(e) => {
                Response::failure(Status::InvalidQuery, None, e.message, Some(e.pos), ms(qt0))
            }
        };
        results.push(BatchItem {
            name: name.clone(),
            response,
        });
    }
    BatchResponse {
        results,
        elapsed_ms: ms(t0),
    }
}

#[derive(Serialize, Debug, Clone)]
pub struct ExplainResponse {
    pub status: Status,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub canonical: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ir: Option<Query>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub trigram_req: Option<String>,
    pub index_exists: bool,
    pub files_considered: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub index_candidates: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stale_files: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub selectivity_pct: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub grams_available: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub grams_decoded: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub index_skipped: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<crate::output::ErrorInfo>,
    pub elapsed_ms: f64,
}

impl ExplainResponse {
    fn err(
        canonical: Option<String>,
        ir: Option<Query>,
        message: String,
        pos: Option<usize>,
        elapsed_ms: f64,
    ) -> Self {
        ExplainResponse {
            status: Status::InvalidQuery,
            canonical,
            ir,
            trigram_req: None,
            index_exists: false,
            files_considered: 0,
            index_candidates: None,
            stale_files: None,
            selectivity_pct: None,
            grams_available: None,
            grams_decoded: None,
            index_skipped: None,
            error: Some(crate::output::ErrorInfo { message, pos }),
            elapsed_ms,
        }
    }
}

pub fn explain(src: &str, opts: &SearchOptions) -> ExplainResponse {
    let t0 = Instant::now();
    let q = match parse(src) {
        Ok(q) => q,
        Err(e) => return ExplainResponse::err(None, None, e.message, Some(e.pos), ms(t0)),
    };
    let canonical = q.to_string();
    if let Err(e) = Compiled::new(&q) {
        return ExplainResponse::err(Some(canonical), Some(q), e, None, ms(t0));
    }
    let filter = match FileFilter::new(&q.filters) {
        Ok(f) => f,
        Err(e) => return ExplainResponse::err(Some(canonical), Some(q), e, None, ms(t0)),
    };

    let req = Req::from_expr(&q.expr);
    let mut stats = Stats::default();
    let mut index_exists = false;
    let mut trace = EvalTrace::default();
    if opts.index != IndexMode::Off {
        if let Ok(Some(ix)) = Index::open(&opts.index_dir()) {
            index_exists = true;
            let _ = plan_with_index(&ix, &q, &filter, opts, None, &mut stats, Some(&mut trace));
        }
    }
    if !index_exists {
        stats.files_considered = walk(&opts.root, &opts.walk, &filter, false).len();
    }
    let selectivity_pct = stats.index_candidates.map(|c| {
        if stats.files_considered == 0 {
            0.0
        } else {
            (c as f64 / stats.files_considered as f64) * 100.0
        }
    });
    ExplainResponse {
        status: Status::Complete,
        canonical: Some(canonical),
        ir: Some(q),
        trigram_req: Some(req.to_string()),
        index_exists,
        files_considered: stats.files_considered,
        index_candidates: stats.index_candidates,
        stale_files: stats.stale_files,
        selectivity_pct,
        grams_available: index_exists.then_some(trace.grams_available),
        grams_decoded: index_exists.then_some(trace.grams_decoded),
        index_skipped: index_exists.then_some(trace.index_skipped),
        error: None,
        elapsed_ms: ms(t0),
    }
}

fn open_index(opts: &SearchOptions) -> Result<Option<Index>, String> {
    if opts.index == IndexMode::Off {
        return Ok(None);
    }
    Index::open(&opts.index_dir()).map_err(|e| format!("{e:#}"))
}

fn expr_has_symbol(e: &Expr) -> bool {
    match e {
        Expr::Match { m } => matches!(m, Matcher::Symbol { .. }),
        Expr::And { args } | Expr::Or { args } => args.iter().any(expr_has_symbol),
        Expr::Not { arg } => expr_has_symbol(arg),
        Expr::Near { a, b, .. } => {
            matches!(a, Matcher::Symbol { .. }) || matches!(b, Matcher::Symbol { .. })
        }
    }
}

fn execute_one(
    q: &Query,
    opts: &SearchOptions,
    ix_res: Result<Option<&Index>, &str>,
    walk_cache: Option<&[FileEntry]>,
    t0: Instant,
) -> Response {
    let canonical = q.to_string();
    if !code::ENABLED && (q.unit == Unit::Function || expr_has_symbol(&q.expr)) {
        return Response::failure(
            Status::Unsupported,
            Some(canonical),
            "AST features ('in function', 'symbol(...)') require building with `--features ast`"
                .into(),
            None,
            ms(t0),
        );
    }
    let compiled = match Compiled::new(q) {
        Ok(c) => c,
        Err(e) => return Response::failure(Status::InvalidQuery, Some(canonical), e, None, ms(t0)),
    };
    let filter = match FileFilter::new(&q.filters) {
        Ok(f) => f,
        Err(e) => return Response::failure(Status::InvalidQuery, Some(canonical), e, None, ms(t0)),
    };

    let mut stats = Stats::default();
    let mut warnings = Vec::new();
    let mut mode = "scan";
    let mut targets: Option<Vec<Target>> = None;
    match ix_res {
        Ok(Some(ix)) => {
            mode = if opts.index == IndexMode::Trust {
                "index_trusted"
            } else {
                "index"
            };
            targets = Some(plan_with_index(
                ix, q, &filter, opts, walk_cache, &mut stats, None,
            ));
        }
        Ok(None) => {
            if opts.index == IndexMode::Trust {
                warnings
                    .push("no index found; fell back to a full scan (run `lancet index`)".into());
            }
        }
        Err(e) => warnings.push(format!("index unusable ({e}); fell back to a full scan")),
    }
    let targets = targets.unwrap_or_else(|| {
        walk_cache
            .map(|w| {
                w.iter()
                    .filter(|e| filter.accepts(&e.rel))
                    .map(|e| Target {
                        path: e.path.clone(),
                        rel: e.rel.clone(),
                    })
                    .collect()
            })
            .unwrap_or_else(|| {
                walk(&opts.root, &opts.walk, &filter, false)
                    .into_iter()
                    .map(|e| Target {
                        path: e.path,
                        rel: e.rel,
                    })
                    .collect()
            })
    });
    if stats.files_considered == 0 {
        stats.files_considered = targets.len();
    }

    let can_early_stop = !q.count && q.terms_top.is_none() && q.sort == SortOrder::Path;
    let early_limit = if can_early_stop { q.limit } else { None };
    let query_terms = if q.terms_top.is_some() {
        collect_query_terms(&q.expr)
    } else {
        HashSet::new()
    };

    let mut pairs: Vec<(ResultItem, usize)> = Vec::new();
    let mut agg_terms: HashMap<String, (usize, usize)> = HashMap::new();
    let mut matched_units_total = 0usize;
    let mut stopped_early = false;

    if let Some(l) = early_limit {
        let mut idx = 0;
        let mut chunk = 256;
        while idx < targets.len() {
            let end = (idx + chunk).min(targets.len());
            let part: Vec<Outcome> = targets[idx..end]
                .par_iter()
                .map(|t| process(t, q, &compiled, opts, &query_terms))
                .collect();
            for o in part {
                accumulate_outcome(
                    o,
                    &mut stats,
                    &mut matched_units_total,
                    &mut pairs,
                    &mut agg_terms,
                );
            }
            idx = end;
            chunk = (chunk * 2).min(4096);
            if pairs.len() >= l && idx < targets.len() {
                stopped_early = true;
                break;
            }
        }
    } else {
        let part: Vec<Outcome> = targets
            .par_iter()
            .map(|t| process(t, q, &compiled, opts, &query_terms))
            .collect();
        for o in part {
            accumulate_outcome(
                o,
                &mut stats,
                &mut matched_units_total,
                &mut pairs,
                &mut agg_terms,
            );
        }
    }

    if let Some(top) = q.terms_top {
        let mut terms: Vec<TermStat> = agg_terms
            .into_iter()
            .map(|(term, (count, files))| TermStat { term, count, files })
            .collect();
        terms.sort_by(|a, b| {
            let sa = (a.files as f64) * (1.0 + (1.0 + a.count as f64).ln());
            let sb = (b.files as f64) * (1.0 + (1.0 + b.count as f64).ln());
            sb.total_cmp(&sa).then_with(|| a.term.cmp(&b.term))
        });
        terms.truncate(top);
        let returned = terms.len();
        stats.elapsed_ms = ms(t0);
        return Response {
            status: if matched_units_total == 0 {
                Status::Empty
            } else {
                Status::Complete
            },
            query: Some(canonical),
            mode: mode.into(),
            matched: matched_units_total,
            matched_is_lower_bound: false,
            returned,
            truncated: false,
            results: vec![],
            terms,
            budget: None,
            warnings,
            error: None,
            stats,
        };
    }

    match q.sort {
        SortOrder::Path => {}
        SortOrder::CountDesc => pairs.sort_by(|a, b| {
            Reverse(a.1)
                .cmp(&Reverse(b.1))
                .then_with(|| a.0.path.cmp(&b.0.path))
                .then_with(|| a.0.start_line.cmp(&b.0.start_line))
        }),
        SortOrder::CountAsc => pairs.sort_by(|a, b| {
            a.1.cmp(&b.1)
                .then_with(|| a.0.path.cmp(&b.0.path))
                .then_with(|| a.0.start_line.cmp(&b.0.start_line))
        }),
    }

    let mut items: Vec<ResultItem> = pairs.into_iter().map(|(it, _)| it).collect();
    let matched = items.len();
    if let Some(l) = if q.count { None } else { q.limit } {
        items.truncate(l);
    }

    let mut budget_info = None;
    if let Some(limit_tokens) = q.budget_tokens {
        let mut kept = Vec::new();
        let mut used = 0usize;
        for it in items {
            let est = estimate_item_tokens(&it);
            if !kept.is_empty() && used + est > limit_tokens {
                break;
            }
            used += est;
            kept.push(it);
        }
        items = kept;
        budget_info = Some(BudgetInfo {
            limit_tokens,
            used_tokens: used,
            estimated: true,
        });
    }

    for (i, it) in items.iter_mut().enumerate() {
        it.id = format!("r{}", i + 1);
    }
    stats.elapsed_ms = ms(t0);
    Response {
        status: if matched == 0 {
            Status::Empty
        } else {
            Status::Complete
        },
        query: Some(canonical),
        mode: mode.into(),
        matched,
        matched_is_lower_bound: stopped_early,
        returned: items.len(),
        truncated: stopped_early || items.len() < matched,
        results: items,
        terms: vec![],
        budget: budget_info,
        warnings,
        error: None,
        stats,
    }
}

fn accumulate_outcome(
    o: Outcome,
    stats: &mut Stats,
    matched_units_total: &mut usize,
    pairs: &mut Vec<(ResultItem, usize)>,
    agg_terms: &mut HashMap<String, (usize, usize)>,
) {
    stats.files_searched += o.searched as usize;
    stats.binary_skipped += o.binary as usize;
    stats.read_errors += o.error as usize;
    stats.files_matched += (o.matched_units > 0) as usize;
    *matched_units_total += o.matched_units;
    pairs.extend(o.items);
    for (term, cnt) in o.file_terms {
        let e = agg_terms.entry(term).or_insert((0, 0));
        e.0 += cnt;
        e.1 += 1;
    }
}

fn estimate_item_tokens(it: &ResultItem) -> usize {
    let mut chars = it.path.len() + it.symbol.as_ref().map_or(0, |s| s.len() + 6) + 16;
    for l in &it.lines {
        let ascii = l.text.bytes().filter(|b| b.is_ascii()).count();
        let non_ascii = l.text.chars().filter(|c| !c.is_ascii()).count();
        chars += ascii + non_ascii * 8 + 8;
    }
    if let Some(t) = &it.text {
        let ascii = t.bytes().filter(|b| b.is_ascii()).count();
        let non_ascii = t.chars().filter(|c| !c.is_ascii()).count();
        chars += ascii + non_ascii * 8;
    }
    chars.div_ceil(4).max(1)
}

fn plan_with_index(
    ix: &Index,
    q: &Query,
    filter: &FileFilter,
    opts: &SearchOptions,
    walk_cache: Option<&[FileEntry]>,
    stats: &mut Stats,
    trace_out: Option<&mut EvalTrace>,
) -> Vec<Target> {
    let req = Req::from_expr(&q.expr);
    plan_targets_with_req(
        ix, &req, filter, &opts.root, "", &opts.walk, opts.index, walk_cache, stats, trace_out,
    )
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn plan_targets_with_req(
    ix: &Index,
    req: &Req,
    filter: &FileFilter,
    root: &Path,
    rel_prefix: &str,
    walk_opts: &WalkOpts,
    mode: IndexMode,
    walk_cache: Option<&[FileEntry]>,
    stats: &mut Stats,
    mut trace_out: Option<&mut EvalTrace>,
) -> Vec<Target> {
    let allowed: Vec<Option<Vec<u32>>> = ix
        .segs
        .iter()
        .map(|seg| {
            let mut seg_trace = EvalTrace::default();
            let res = req.eval_with_trace(seg, &mut seg_trace);
            if let Some(t) = trace_out.as_deref_mut() {
                t.grams_available += seg_trace.grams_available;
                t.grams_decoded += seg_trace.grams_decoded;
                t.index_skipped |= seg_trace.index_skipped;
            }
            res
        })
        .collect();
    let mut out = Vec::new();
    let mut cands = 0usize;
    let is_dir = root.is_dir();
    if mode == IndexMode::Trust {
        for (si, seg) in ix.segs.iter().enumerate() {
            let mut push_if_ok = |id: u32| {
                if ix.is_dead(si, id) {
                    return;
                }
                let (_, _, flags) = seg.doc_meta(id);
                if flags & (FLAG_BINARY | FLAG_ERROR) != 0 {
                    return;
                }
                let doc_rel = seg.doc_rel(id);
                let local_rel: String = if rel_prefix.is_empty() {
                    doc_rel.into_owned()
                } else if is_dir {
                    let Some(rest) = doc_rel.strip_prefix(rel_prefix) else {
                        return;
                    };
                    let Some(stripped) = rest.strip_prefix('/') else {
                        return;
                    };
                    stripped.to_string()
                } else {
                    if doc_rel.as_ref() != rel_prefix {
                        return;
                    }
                    root.file_name()
                        .map(|n| n.to_string_lossy().into_owned())
                        .unwrap_or_default()
                };
                if rel_prefix.is_empty() || is_dir || !local_rel.is_empty() {
                    stats.files_considered += 1;
                }
                if filter.accepts(&local_rel) {
                    cands += 1;
                    let path = if is_dir {
                        root.join(&local_rel)
                    } else {
                        root.to_path_buf()
                    };
                    out.push(Target {
                        path,
                        rel: local_rel,
                    });
                }
            };
            match &allowed[si] {
                Some(ids) => ids.iter().copied().for_each(&mut push_if_ok),
                None => (0..seg.ndocs() as u32).for_each(&mut push_if_ok),
            }
        }
        if rel_prefix.is_empty() {
            stats.files_considered = ix.live_count();
        }
        out.sort_by(|a, b| a.rel.cmp(&b.rel));
    } else {
        let ok = |si: usize, id: u32| {
            allowed[si]
                .as_ref()
                .is_none_or(|ids| ids.binary_search(&id).is_ok())
        };
        let mut cursors = vec![0u32; ix.segs.len()];
        let mut stale = 0usize;
        let owned;
        let entries: &[FileEntry] = match walk_cache {
            Some(w) => w,
            None => {
                owned = walk(root, walk_opts, filter, true);
                &owned
            }
        };
        for e in entries {
            if walk_cache.is_some() && !filter.accepts(&e.rel) {
                continue;
            }
            stats.files_considered += 1;
            let hit = if rel_prefix.is_empty() {
                ix.advance_cursors(&mut cursors, e.rel.as_bytes())
            } else if is_dir {
                let full = format!("{rel_prefix}/{}", e.rel);
                ix.advance_cursors(&mut cursors, full.as_bytes())
            } else {
                ix.lookup_live(rel_prefix.as_bytes())
            };
            let keep = match hit {
                Some((si, id)) => {
                    let (mt, sz, flags) = ix.segs[si].doc_meta(id);
                    if mt == e.mtime_ns && sz == e.size {
                        let k = flags & FLAG_BINARY == 0
                            && (flags & (FLAG_ERROR | FLAG_OVERSIZE) != 0 || ok(si, id));
                        cands += k as usize;
                        k
                    } else {
                        stale += 1;
                        true
                    }
                }
                None => {
                    stale += 1;
                    true
                }
            };
            if keep {
                out.push(Target {
                    path: e.path.clone(),
                    rel: e.rel.clone(),
                });
            }
        }
        stats.stale_files = Some(stale);
    }
    stats.index_candidates = Some(cands);
    out
}

fn process(
    t: &Target,
    q: &Query,
    c: &Compiled,
    opts: &SearchOptions,
    query_terms: &HashSet<String>,
) -> Outcome {
    let data = match content::load_searchable(&t.path, opts.ocr) {
        Ok(Some(d)) => d,
        Ok(None) => {
            return Outcome {
                binary: true,
                ..Default::default()
            }
        }
        Err(_) => {
            return Outcome {
                error: true,
                ..Default::default()
            }
        }
    };
    let line_mode = if q.count || q.files_only || q.terms_top.is_some() {
        LineMode::None
    } else if q.unit == Unit::Line && !q.group_file {
        LineMode::HitsOnly
    } else {
        LineMode::Full
    };
    let fe = eval_file(c, &t.path, &data, line_mode);
    let mut o = Outcome {
        searched: true,
        ..Default::default()
    };
    if fe.hits.is_empty() {
        return o;
    }
    o.matched_units = fe.hits.len();

    if q.terms_top.is_some() {
        for h in &fe.hits {
            for_each_token(&data[h.start..h.end], &mut |s, e| {
                let raw = String::from_utf8_lossy(&data[h.start + s..h.start + e]);
                let folded = fold_str(&raw);
                if keep_term(&folded, query_terms) {
                    *o.file_terms.entry(folded).or_insert(0) += 1;
                }
                true
            });
        }
        return o;
    }

    if q.count {
        o.items.push((
            ResultItem {
                id: String::new(),
                path: t.rel.clone(),
                unit: q.unit,
                start_line: 0,
                end_line: 0,
                symbol: None,
                lines: vec![],
                more_lines: 0,
                count: Some(fe.hits.len()),
                text: None,
            },
            fe.hits.len(),
        ));
        return o;
    }

    if q.files_only {
        o.items.push((
            ResultItem {
                id: String::new(),
                path: t.rel.clone(),
                unit: Unit::File,
                start_line: 0,
                end_line: 0,
                symbol: None,
                lines: vec![],
                more_lines: 0,
                count: None,
                text: None,
            },
            fe.hits.len(),
        ));
        return o;
    }

    let ls = &fe.line_starts;
    if q.group_file && fe.hits.len() > 1 {
        let first = &fe.hits[0];
        let last = &fe.hits[fe.hits.len() - 1];
        let mut lines: Vec<u32> = fe
            .hits
            .iter()
            .flat_map(|h| h.match_lines.iter().copied())
            .collect();
        lines.sort_unstable();
        lines.dedup();
        let hit_lines = lines.len().max(fe.hits.len());
        let merged = UnitHit {
            start: first.start,
            end: last.end,
            start_line: first.start_line,
            end_line: last.end_line,
            match_lines: lines,
            symbol: None,
        };
        o.items.push((
            render(&t.rel, &merged, Unit::File, &data, ls, q, opts),
            hit_lines,
        ));
    } else {
        for h in &fe.hits {
            let weight = h.match_lines.len().max(1);
            o.items
                .push((render(&t.rel, h, q.unit, &data, ls, q, opts), weight));
        }
    }
    o
}

fn collect_query_terms(e: &Expr) -> HashSet<String> {
    let mut set = HashSet::new();
    fn visit_m(m: &Matcher, set: &mut HashSet<String>) {
        match m {
            Matcher::Lit { text }
            | Matcher::Fuzzy { text, .. }
            | Matcher::Symbol { name: text } => {
                for_each_token(text.as_bytes(), &mut |s, e| {
                    set.insert(fold_str(&text[s..e]));
                    true
                });
            }
            Matcher::Regex { .. } => {}
        }
    }
    fn visit_e(e: &Expr, set: &mut HashSet<String>) {
        match e {
            Expr::Match { m } => visit_m(m, set),
            Expr::And { args } | Expr::Or { args } => args.iter().for_each(|a| visit_e(a, set)),
            Expr::Not { arg } => visit_e(arg, set),
            Expr::Near { a, b, .. } => {
                visit_m(a, set);
                visit_m(b, set);
            }
        }
    }
    visit_e(e, &mut set);
    set
}

fn keep_term(t: &str, query_terms: &HashSet<String>) -> bool {
    if t.chars().count() < 3 || t.bytes().all(|b| b.is_ascii_digit()) || query_terms.contains(t) {
        return false;
    }
    !matches!(
        t,
        "the"
            | "and"
            | "for"
            | "that"
            | "with"
            | "this"
            | "from"
            | "are"
            | "not"
            | "was"
            | "were"
            | "have"
            | "has"
            | "had"
            | "but"
            | "you"
            | "all"
            | "can"
            | "her"
            | "one"
            | "our"
            | "out"
            | "use"
            | "used"
            | "using"
            | "into"
            | "than"
            | "then"
            | "them"
            | "they"
            | "when"
            | "where"
            | "which"
            | "while"
            | "will"
            | "let"
            | "pub"
            | "mut"
            | "const"
            | "var"
            | "def"
            | "return"
            | "self"
            | "true"
            | "false"
            | "none"
            | "null"
            | "import"
            | "class"
            | "struct"
            | "enum"
            | "impl"
            | "trait"
            | "async"
            | "await"
            | "match"
            | "else"
            | "elif"
            | "loop"
            | "break"
            | "continue"
            | "void"
            | "int"
            | "str"
            | "string"
            | "bool"
            | "type"
            | "func"
            | "package"
            | "fn"
            | "mod"
            | "crate"
            | "super"
    )
}

fn trim_line(slice: &[u8], max_cols: usize) -> String {
    let mut e = slice.len();
    while e > 0 && matches!(slice[e - 1], b'\n' | b'\r') {
        e -= 1;
    }
    truncate_chars(&String::from_utf8_lossy(&slice[..e]), max_cols)
}

fn line_text(data: &[u8], ls: &[usize], n: u32, max_cols: usize) -> String {
    let i = (n as usize).saturating_sub(1);
    let s = ls[i];
    let e = ls.get(i + 1).copied().unwrap_or(data.len());
    trim_line(&data[s..e], max_cols)
}

fn truncate_chars(s: &str, max: usize) -> String {
    match s.char_indices().nth(max) {
        Some((i, _)) => format!("{}…", &s[..i]),
        None => s.to_string(),
    }
}

fn render(
    rel: &str,
    h: &UnitHit,
    unit: Unit,
    data: &[u8],
    ls: &[usize],
    q: &Query,
    opts: &SearchOptions,
) -> ResultItem {
    let mut shown: Vec<u32> = h.match_lines.clone();
    let more = shown.len().saturating_sub(opts.max_lines);
    shown.truncate(opts.max_lines);
    if shown.is_empty() {
        shown.push(h.start_line); // e.g. a unit matched only through NOT
    }
    let lines = if ls.is_empty() {
        // HitsOnly fast path for `Unit::Line`: slice matched line and walk context locally.
        let mut map: BTreeMap<u32, (usize, usize, bool)> = BTreeMap::new();
        map.insert(h.start_line, (h.start, h.end, true));
        if q.context > 0 {
            let mut cur_s = h.start;
            for step in 1..=q.context {
                if cur_s == 0 {
                    break;
                }
                let prev_end = cur_s;
                let prev_start = memchr::memrchr(b'\n', &data[..cur_s - 1])
                    .map(|p| p + 1)
                    .unwrap_or(0);
                map.entry(h.start_line - step)
                    .or_insert((prev_start, prev_end, false));
                cur_s = prev_start;
            }
            let mut cur_e = h.end;
            for step in 1..=q.context {
                if cur_e >= data.len() {
                    break;
                }
                let next_start = cur_e;
                let next_end = memchr::memchr(b'\n', &data[cur_e..])
                    .map(|p| cur_e + p + 1)
                    .unwrap_or(data.len());
                map.entry(h.start_line + step)
                    .or_insert((next_start, next_end, false));
                cur_e = next_end;
            }
        }
        map.into_iter()
            .map(|(n, (s, e, is_match))| LineOut {
                line: n,
                text: trim_line(&data[s..e], opts.max_cols),
                context: !is_match,
            })
            .collect()
    } else {
        let nlines = ls.len() as u32;
        let mut map: BTreeMap<u32, bool> = BTreeMap::new();
        for &m in &shown {
            map.insert(m, true);
        }
        if q.context > 0 {
            for &m in &shown {
                let lo = m.saturating_sub(q.context).max(1);
                let hi = (m + q.context).min(nlines);
                for l in lo..=hi {
                    map.entry(l).or_insert(false);
                }
            }
        }
        map.into_iter()
            .map(|(n, is_match)| LineOut {
                line: n,
                text: line_text(data, ls, n, opts.max_cols),
                context: !is_match,
            })
            .collect()
    };
    let text = q.expand_unit.then(|| {
        let mut e = h.end.min(h.start + opts.max_unit_bytes);
        while e < h.end && e > h.start && (data[e] & 0xC0) == 0x80 {
            e -= 1; // keep UTF-8 boundaries
        }
        let mut t = String::from_utf8_lossy(&data[h.start..e]).into_owned();
        if e < h.end {
            t.push_str("\n…[unit truncated]");
        }
        t
    });
    ResultItem {
        id: String::new(),
        path: rel.to_string(),
        unit,
        start_line: h.start_line,
        end_line: h.end_line,
        symbol: h.symbol.clone(),
        lines,
        more_lines: more,
        count: None,
        text,
    }
}

/// Convenience for callers that only have a root path.
pub fn search_in(src: &str, root: &Path) -> Response {
    search(src, &SearchOptions::new(root))
}
