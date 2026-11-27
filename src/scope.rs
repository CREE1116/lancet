//! Which files a command looks at: the index narrows the candidates for a
//! requirement, and every file changed since indexing is always included, so
//! results are the same with or without the index.

use crate::index::{EvalTrace, Index, Req, FLAG_BINARY, FLAG_ERROR, FLAG_OVERSIZE};
use crate::output::Stats;
use crate::walk::{walk, FileEntry, FileFilter, WalkOpts};
use std::path::{Path, PathBuf};

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

pub(crate) struct Target {
    pub path: PathBuf,
    pub rel: String,
}

/// How a file's path is shown: joined onto the path given, as ripgrep does
/// (`.` gives `./x`), or cleaned of a leading `./` (`x`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Shown {
    AsGiven,
    Clean,
}

/// What to plan: the paths a command was given and how to walk and index them.
pub struct Request<'a> {
    /// Directories or files; empty means the current directory, shown without a prefix.
    pub roots: &'a [PathBuf],
    pub walk: &'a WalkOpts,
    pub index: IndexMode,
    pub index_dir: Option<&'a Path>,
    pub shown: Shown,
}

/// A file to look at.
pub struct Candidate {
    pub path: PathBuf,
    /// The path as output shows it (see [`Shown`]).
    pub shown: String,
    /// Named on the command line (not found by walking a directory).
    pub explicit: bool,
}

/// The files for a request, in output order: each given path in turn, its files
/// sorted component-wise (`a/b.txt` before `a.txt`), a file shown twice kept once.
pub struct Plan {
    pub files: Vec<Candidate>,
    pub summary: Summary,
}

/// What planning found besides the files.
pub struct Summary {
    /// "No such file or directory" messages for paths that don't exist.
    pub missing: Vec<String>,
    /// A directory was among the paths (so output names files).
    pub any_dir: bool,
    /// `scan`, `index`, `index_trusted`, or `mixed` across several paths.
    pub mode: String,
    pub warnings: Vec<String>,
    pub stats: Stats,
}

/// Plans the files every searching command reads. `filter` builds the name filter
/// for one directory (its globs are relative to it); `req` is what a file's content
/// must contain, which the index uses to skip files that can't match. A file named
/// on the command line is taken as it is.
///
/// Walking first and searching afterwards measured faster than overlapping the two
/// (searching on the walker's threads, or streaming files into rayon): see
/// `bench/results/baseline-step1.md`.
pub fn plan(
    r: &Request,
    req: &Req,
    filter: &dyn Fn(&Path) -> Result<FileFilter, String>,
) -> Result<Plan, String> {
    let dot = [PathBuf::from(".")];
    let implicit = r.roots.is_empty();
    let roots = if implicit { &dot[..] } else { r.roots };
    // The index holds the default walk; these rules need a live one.
    let index = if (r.walk.follow && r.index == IndexMode::Trust)
        || !r.walk.ignore.is_default()
        || r.walk.one_file_system
    {
        IndexMode::Off
    } else {
        r.index
    };
    let mut files: Vec<Candidate> = Vec::new();
    let mut p = Summary {
        missing: Vec::new(),
        any_dir: false,
        mode: String::new(),
        warnings: Vec::new(),
        stats: Stats::default(),
    };
    let note_mode = |p: &mut Summary, m: &str| {
        p.mode = match p.mode.as_str() {
            "" => m.to_string(),
            same if same == m => m.to_string(),
            _ => "mixed".into(),
        }
    };
    for root in roots {
        let group = files.len();
        if root.is_file() {
            if r.walk
                .max_filesize
                .is_none_or(|max| root.metadata().is_ok_and(|m| m.len() <= max))
            {
                p.stats.files_considered += 1;
                files.push(Candidate {
                    path: root.clone(),
                    shown: root.to_string_lossy().replace('\\', "/"),
                    explicit: true,
                });
            }
            continue;
        }
        if !root.is_dir() {
            p.missing.push(format!(
                "{}: No such file or directory (os error 2)",
                root.display()
            ));
            continue;
        }
        p.any_dir = true;
        let filt = filter(root)?;
        let mut targets = None;
        if index != IndexMode::Off {
            match crate::index::discover(root, r.index_dir) {
                Ok(Some((ix, prefix))) => {
                    note_mode(
                        &mut p,
                        if index == IndexMode::Trust {
                            "index_trusted"
                        } else {
                            "index"
                        },
                    );
                    let mut st = Stats::default();
                    targets = Some(plan_targets_with_req(
                        &ix, req, &filt, root, &prefix, r.walk, index, None, &mut st, None,
                    ));
                    p.stats.files_considered += st.files_considered;
                    for (dst, src) in [
                        (&mut p.stats.index_candidates, st.index_candidates),
                        (&mut p.stats.stale_files, st.stale_files),
                    ] {
                        if let Some(n) = src {
                            *dst = Some(dst.unwrap_or(0) + n);
                        }
                    }
                }
                Ok(None) if index == IndexMode::Trust => p
                    .warnings
                    .push("no index found; fell back to a full scan (run `lancet index`)".into()),
                Ok(None) => {}
                Err(e) => p
                    .warnings
                    .push(format!("index unusable ({e:#}); searched without it")),
            }
        }
        let targets = targets.unwrap_or_else(|| {
            note_mode(&mut p, "scan");
            let ts: Vec<Target> = walk(root, r.walk, &filt, false)
                .into_iter()
                .map(|e| Target {
                    path: e.path,
                    rel: e.rel,
                })
                .collect();
            p.stats.files_considered += ts.len();
            ts
        });
        let base = root.to_string_lossy().replace('\\', "/");
        let base = base.trim_end_matches('/');
        files.extend(targets.into_iter().map(|t| {
            let shown = match (implicit, r.shown) {
                (true, _) => t.rel,
                (false, Shown::AsGiven) => format!("{base}/{}", t.rel),
                (false, Shown::Clean) => crate::walk::display_path(root, &t.rel),
            };
            Candidate {
                path: t.path,
                shown,
                explicit: false,
            }
        }));
        files[group..].sort_by(|a, b| a.shown.split('/').cmp(b.shown.split('/')));
    }
    let mut seen = std::collections::HashSet::new();
    files.retain(|c| seen.insert(c.shown.clone()));
    if p.mode.is_empty() {
        p.mode = "scan".into();
    }
    Ok(Plan { files, summary: p })
}

/// Whether `--trust-index` may answer from the index alone: it must hold exactly the
/// files this walk would visit (same hidden / ignore rules, no whitelisting glob
/// reaching past them). Otherwise the files are walked, as without `--trust-index`.
pub(crate) fn trust_covers(ix: &Index, walk: &WalkOpts, filter: &FileFilter) -> bool {
    let s = ix.manifest.scope;
    index_matches_walk(ix, walk) && (!filter.whitelists() || (s.hidden && s.no_ignore))
}

/// The index was synced with the same hidden / ignore rules as `walk`.
pub(crate) fn index_matches_walk(ix: &Index, walk: &WalkOpts) -> bool {
    let s = ix.manifest.scope;
    s.hidden == walk.hidden && s.no_ignore == walk.no_ignore
}

#[allow(clippy::too_many_arguments)]
fn plan_targets_with_req(
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
    let mode = if mode == IndexMode::Trust && !trust_covers(ix, walk_opts, filter) {
        IndexMode::Auto
    } else {
        mode
    };
    if mode == IndexMode::Trust {
        for (si, seg) in ix.segs.iter().enumerate() {
            let mut push_if_ok = |id: u32| {
                if ix.is_dead(si, id) {
                    return;
                }
                let flags = seg.doc_meta(id).flags;
                if flags & (FLAG_BINARY | FLAG_ERROR) != 0 {
                    return;
                }
                if walk_opts
                    .max_filesize
                    .is_some_and(|max| seg.doc_meta(id).size > max)
                {
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
                if walk_opts
                    .max_depth
                    .is_some_and(|d| local_rel.split('/').count() > d)
                {
                    return;
                }
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
                    let meta = ix.segs[si].doc_meta(id);
                    let flags = meta.flags;
                    if meta.matches(e) {
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
