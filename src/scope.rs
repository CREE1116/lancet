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
