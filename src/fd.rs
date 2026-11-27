//! Fast `.gitignore`-aware file & folder explorer (`lancet fd`), with
//! optional zero-walk mmap acceleration when `--trust-index` is used.

use crate::index::{self, Index};
use crate::output::{ErrorInfo, Status};
use crate::query::FileType;
use crate::scope::IndexMode;
use crate::walk::{mtime_ns, rel_path, WalkOpts};
use globset::{GlobBuilder, GlobMatcher};
use ignore::{WalkBuilder, WalkState};
use regex::{Regex, RegexBuilder};
use serde::Serialize;
use std::collections::{BTreeMap, HashMap};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum PatternMode {
    #[default]
    Substring,
    Glob,
    Regex,
}

#[derive(Clone, Debug)]
pub struct FdOptions {
    pub root: PathBuf,
    pub pattern: Option<String>,
    pub pattern_mode: PatternMode,
    pub case_sensitive: bool,
    pub include_files: bool,
    pub include_dirs: bool,
    pub exts: Vec<String>,
    pub file_types: Vec<FileType>,
    pub max_depth: Option<usize>,
    pub changed_within_ns: Option<u64>,
    pub min_size: Option<u64>,
    pub max_size: Option<u64>,
    pub limit: Option<usize>,
    pub tree: bool,
    pub walk: WalkOpts,
    pub index: IndexMode,
    pub index_dir: Option<PathBuf>,
    pub need_meta: bool,
}

impl FdOptions {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        FdOptions {
            root: root.into(),
            pattern: None,
            pattern_mode: PatternMode::Substring,
            case_sensitive: false,
            include_files: false,
            include_dirs: false,
            exts: Vec::new(),
            file_types: Vec::new(),
            max_depth: None,
            changed_within_ns: None,
            min_size: None,
            max_size: None,
            limit: None,
            tree: false,
            walk: WalkOpts::default(),
            index: IndexMode::Auto,
            index_dir: None,
            need_meta: false,
        }
    }

    fn effective_kinds(&self) -> (bool, bool) {
        if self.include_files || self.include_dirs {
            (self.include_files, self.include_dirs)
        } else if !self.exts.is_empty()
            || !self.file_types.is_empty()
            || self.min_size.is_some()
            || self.max_size.is_some()
        {
            (true, false)
        } else {
            (true, true)
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct FdEntry {
    pub path: String,
    pub kind: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file_type: Option<FileType>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub size: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mtime_ns: Option<u64>,
}

#[derive(Clone, Debug, Serialize)]
pub struct FdResponse {
    pub status: Status,
    pub mode: String,
    pub total: usize,
    pub returned: usize,
    pub truncated: bool,
    pub entries: Vec<FdEntry>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub warnings: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<ErrorInfo>,
    pub elapsed_ms: f64,
}

impl FdResponse {
    /// One response for several roots (`lancet fd PAT a b c`): entries in the order
    /// the roots were given; any root's error makes the whole call an error.
    pub fn merge(mut rs: Vec<FdResponse>) -> FdResponse {
        if rs.len() == 1 {
            return rs.pop().unwrap();
        }
        let errors: Vec<String> = rs
            .iter()
            .filter_map(|r| r.error.as_ref().map(|e| e.message.clone()))
            .collect();
        let entries: Vec<FdEntry> = rs
            .iter_mut()
            .flat_map(|r| std::mem::take(&mut r.entries))
            .collect();
        let status = if !errors.is_empty() {
            rs.iter()
                .find(|r| r.error.is_some())
                .map(|r| r.status)
                .unwrap_or(Status::InvalidQuery)
        } else if entries.is_empty() {
            Status::Empty
        } else {
            Status::Complete
        };
        FdResponse {
            status,
            mode: rs.first().map(|r| r.mode.clone()).unwrap_or_default(),
            total: rs.iter().map(|r| r.total).sum(),
            returned: entries.len(),
            truncated: rs.iter().any(|r| r.truncated),
            entries,
            warnings: rs.iter().flat_map(|r| r.warnings.clone()).collect(),
            error: (!errors.is_empty()).then(|| ErrorInfo {
                message: errors.join("; "),
                pos: None,
            }),
            elapsed_ms: rs.iter().map(|r| r.elapsed_ms).sum(),
        }
    }

    fn failure(status: Status, message: String, elapsed_ms: f64) -> Self {
        FdResponse {
            status,
            mode: "scan".into(),
            total: 0,
            returned: 0,
            truncated: false,
            entries: Vec::new(),
            warnings: Vec::new(),
            error: Some(ErrorInfo { message, pos: None }),
            elapsed_ms,
        }
    }
}

/// Parse a relative duration like `30s`, `15m`, `2h`, `7d`, `1w` into nanoseconds.
pub fn parse_duration(raw: &str) -> Result<u64, String> {
    let s = raw.trim();
    if s.is_empty() {
        return Err("empty duration".into());
    }
    let split = s
        .find(|c: char| !c.is_ascii_digit())
        .ok_or_else(|| format!("duration `{raw}` needs a unit suffix (`s`, `m`, `h`, `d`, `w`)"))?;
    let (num_s, unit_s) = s.split_at(split);
    let n: u64 = num_s
        .parse()
        .map_err(|_| format!("invalid number `{num_s}` in duration `{raw}`"))?;
    let mult_secs: u64 = match unit_s.trim().to_ascii_lowercase().as_str() {
        "s" | "sec" | "secs" => 1,
        "m" | "min" | "mins" => 60,
        "h" | "hr" | "hrs" | "hour" | "hours" => 3600,
        "d" | "day" | "days" => 86_400,
        "w" | "wk" | "wks" | "week" | "weeks" => 604_800,
        other => return Err(format!("unknown duration unit `{other}` in `{raw}`")),
    };
    n.checked_mul(mult_secs)
        .and_then(|secs| secs.checked_mul(1_000_000_000))
        .ok_or_else(|| format!("duration `{raw}` overflows u64"))
}

/// Parse a human byte size like `512`, `10k`, `5M`, `2GB` (base 1024).
pub fn parse_size(raw: &str) -> Result<u64, String> {
    let s = raw.trim();
    if s.is_empty() {
        return Err("empty size".into());
    }
    let split = s.find(|c: char| !c.is_ascii_digit()).unwrap_or(s.len());
    let (num_s, unit_s) = s.split_at(split);
    let n: u64 = num_s
        .parse()
        .map_err(|_| format!("invalid number `{num_s}` in size `{raw}`"))?;
    let mult: u64 = match unit_s.trim().to_ascii_lowercase().as_str() {
        "" | "b" => 1,
        "k" | "kb" | "kib" => 1024,
        "m" | "mb" | "mib" => 1024 * 1024,
        "g" | "gb" | "gib" => 1024 * 1024 * 1024,
        other => return Err(format!("unknown size unit `{other}` in `{raw}`")),
    };
    n.checked_mul(mult)
        .ok_or_else(|| format!("size `{raw}` overflows u64"))
}

enum PatMatcher {
    All,
    Sub {
        needle: String,
        case_sensitive: bool,
    },
    Glob(GlobMatcher),
    Regex(Regex),
}

impl PatMatcher {
    fn compile(
        pattern: Option<&str>,
        mode: PatternMode,
        case_sensitive: bool,
    ) -> Result<Self, String> {
        let Some(p) = pattern else {
            return Ok(PatMatcher::All);
        };
        if p.is_empty() {
            return Ok(PatMatcher::All);
        }
        match mode {
            PatternMode::Substring => Ok(PatMatcher::Sub {
                needle: if case_sensitive {
                    p.to_string()
                } else {
                    p.to_lowercase()
                },
                case_sensitive,
            }),
            PatternMode::Glob => {
                let g = GlobBuilder::new(p)
                    .case_insensitive(!case_sensitive)
                    .build()
                    .map_err(|e| format!("invalid glob pattern `{p}`: {e}"))?;
                Ok(PatMatcher::Glob(g.compile_matcher()))
            }
            PatternMode::Regex => {
                let r = RegexBuilder::new(p)
                    .case_insensitive(!case_sensitive)
                    .build()
                    .map_err(|e| format!("invalid regex pattern `{p}`: {e}"))?;
                Ok(PatMatcher::Regex(r))
            }
        }
    }

    fn matches(&self, rel: &str) -> bool {
        match self {
            PatMatcher::All => true,
            PatMatcher::Sub {
                needle,
                case_sensitive,
            } => {
                if *case_sensitive {
                    rel.contains(needle)
                } else {
                    rel.to_lowercase().contains(needle)
                }
            }
            PatMatcher::Glob(g) => {
                g.is_match(rel)
                    || Path::new(rel)
                        .file_name()
                        .and_then(|n| n.to_str())
                        .is_some_and(|name| g.is_match(name))
            }
            PatMatcher::Regex(r) => r.is_match(rel),
        }
    }
}

struct CompiledFdFilter {
    pat: PatMatcher,
    include_files: bool,
    include_dirs: bool,
    exts: Vec<String>,
    file_types: Vec<FileType>,
    max_depth: Option<usize>,
    min_mtime_ns: Option<u64>,
    min_size: Option<u64>,
    max_size: Option<u64>,
    need_meta: bool,
}

impl CompiledFdFilter {
    fn matches_entry(
        &self,
        rel: &str,
        is_dir: bool,
        depth: usize,
        size: Option<u64>,
        mtime: Option<u64>,
    ) -> bool {
        if let Some(md) = self.max_depth {
            if depth > md {
                return false;
            }
        }
        if is_dir && !self.include_dirs {
            return false;
        }
        if !is_dir && !self.include_files {
            return false;
        }
        if is_dir {
            if !self.exts.is_empty()
                || !self.file_types.is_empty()
                || self.min_size.is_some()
                || self.max_size.is_some()
            {
                return false;
            }
        } else {
            let p = Path::new(rel);
            if !self.exts.is_empty() {
                let ext = p
                    .extension()
                    .and_then(|e| e.to_str())
                    .unwrap_or("")
                    .to_ascii_lowercase();
                if !self.exts.contains(&ext) {
                    return false;
                }
            }
            if !self.file_types.is_empty() && !self.file_types.contains(&FileType::from_path(p)) {
                return false;
            }
            if let Some(min_s) = self.min_size {
                if size.is_none_or(|s| s < min_s) {
                    return false;
                }
            }
            if let Some(max_s) = self.max_size {
                if size.is_none_or(|s| s > max_s) {
                    return false;
                }
            }
        }
        if let Some(cutoff) = self.min_mtime_ns {
            if mtime.is_none_or(|m| m < cutoff) {
                return false;
            }
        }
        self.pat.matches(rel)
    }
}

pub fn find_files(opts: &FdOptions) -> FdResponse {
    if let Some(msg) = crate::walk::missing_root(&opts.root) {
        return FdResponse::failure(Status::InvalidQuery, msg, 0.0);
    }
    let mut r = find_files_raw(opts);
    if !opts.tree {
        // The tree view draws root-relative paths under the root's own label.
        for e in &mut r.entries {
            e.path = crate::walk::display_path(&opts.root, &e.path);
        }
    }
    r
}

fn find_files_raw(opts: &FdOptions) -> FdResponse {
    let t0 = Instant::now();
    let pat = match PatMatcher::compile(
        opts.pattern.as_deref(),
        opts.pattern_mode,
        opts.case_sensitive,
    ) {
        Ok(p) => p,
        Err(e) => return FdResponse::failure(Status::InvalidQuery, e, ms(t0)),
    };
    let (include_files, include_dirs) = opts.effective_kinds();
    let now_ns = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(0);
    let min_mtime_ns = opts.changed_within_ns.map(|d| now_ns.saturating_sub(d));
    let exts: Vec<String> = opts
        .exts
        .iter()
        .map(|e| e.trim_start_matches('.').to_ascii_lowercase())
        .filter(|e| !e.is_empty())
        .collect();
    let need_meta = opts.need_meta
        || min_mtime_ns.is_some()
        || opts.min_size.is_some()
        || opts.max_size.is_some();

    let cf = CompiledFdFilter {
        pat,
        include_files,
        include_dirs,
        exts,
        file_types: opts.file_types.clone(),
        max_depth: opts.max_depth,
        min_mtime_ns,
        min_size: opts.min_size,
        max_size: opts.max_size,
        need_meta,
    };

    let mut warnings = Vec::new();
    let mut mode = "scan".to_string();
    let mut entries: Option<Vec<FdEntry>> = None;

    if opts.index == IndexMode::Trust {
        match index::discover(&opts.root, opts.index_dir.as_deref()) {
            Ok(Some((ix, rel_prefix))) if crate::scope::index_matches_walk(&ix, &opts.walk) => {
                mode = "index_trusted".into();
                entries = Some(collect_from_index(&ix, &opts.root, &rel_prefix, &cf));
            }
            Ok(Some(_)) => warnings.push(
                "the index holds files for other --hidden / --no-ignore settings; walked the disk instead".into(),
            ),
            Ok(None) => {
                warnings
                    .push("no index found; fell back to a full scan (run `lancet index`)".into());
            }
            Err(e) => {
                warnings.push(format!("index unusable ({e:#}); fell back to a full scan"));
            }
        }
    }

    let mut entries = entries.unwrap_or_else(|| collect_from_walk(&opts.root, &opts.walk, &cf));
    entries.sort_by(|a, b| a.path.cmp(&b.path).then_with(|| a.kind.cmp(b.kind)));

    let total = entries.len();
    let mut truncated = false;
    if let Some(lim) = opts.limit {
        if entries.len() > lim {
            entries.truncate(lim);
            truncated = true;
        }
    }
    let returned = entries.len();
    FdResponse {
        status: if total == 0 {
            Status::Empty
        } else {
            Status::Complete
        },
        mode,
        total,
        returned,
        truncated,
        entries,
        warnings,
        error: None,
        elapsed_ms: ms(t0),
    }
}

fn collect_from_index(
    ix: &Index,
    root: &Path,
    rel_prefix: &str,
    cf: &CompiledFdFilter,
) -> Vec<FdEntry> {
    let is_dir_root = root.is_dir();
    let mut dirs: HashMap<String, u64> = HashMap::new();
    let mut out = Vec::new();

    for (si, seg) in ix.segs.iter().enumerate() {
        for id in 0..seg.ndocs() as u32 {
            if ix.is_dead(si, id) {
                continue;
            }
            let doc_rel = seg.doc_rel(id);
            let local_rel: String = if rel_prefix.is_empty() {
                doc_rel.into_owned()
            } else if is_dir_root {
                let Some(rest) = doc_rel.strip_prefix(rel_prefix) else {
                    continue;
                };
                let Some(stripped) = rest.strip_prefix('/') else {
                    continue;
                };
                stripped.to_string()
            } else {
                if doc_rel.as_ref() != rel_prefix {
                    continue;
                }
                root.file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_default()
            };
            if local_rel.is_empty() {
                continue;
            }
            let m = seg.doc_meta(id);
            let (mt, sz) = (m.mtime_ns, m.size);
            if cf.include_dirs {
                let mut cur = local_rel.as_str();
                while let Some(pos) = cur.rfind('/') {
                    let parent = &cur[..pos];
                    dirs.entry(parent.to_string())
                        .and_modify(|m| *m = (*m).max(mt))
                        .or_insert(mt);
                    cur = parent;
                }
            }
            let depth = local_rel.split('/').count();
            let (size_opt, mtime_opt) = if cf.need_meta {
                (Some(sz), Some(mt))
            } else {
                (None, None)
            };
            if cf.matches_entry(&local_rel, false, depth, Some(sz), Some(mt)) {
                out.push(FdEntry {
                    file_type: Some(FileType::from_path(Path::new(&local_rel))),
                    path: local_rel,
                    kind: "file",
                    size: size_opt,
                    mtime_ns: mtime_opt,
                });
            }
        }
    }

    if cf.include_dirs {
        for (dir_rel, max_mt) in dirs {
            let depth = dir_rel.split('/').count();
            let mtime_opt = if cf.need_meta { Some(max_mt) } else { None };
            if cf.matches_entry(&dir_rel, true, depth, None, Some(max_mt)) {
                out.push(FdEntry {
                    path: dir_rel,
                    kind: "dir",
                    file_type: None,
                    size: None,
                    mtime_ns: mtime_opt,
                });
            }
        }
    }

    out
}

struct Flush<'a> {
    out: &'a Mutex<Vec<FdEntry>>,
    buf: Vec<FdEntry>,
}

impl Drop for Flush<'_> {
    fn drop(&mut self) {
        if let Ok(mut o) = self.out.lock() {
            o.append(&mut self.buf);
        }
    }
}

fn collect_from_walk(root: &Path, wopts: &WalkOpts, cf: &CompiledFdFilter) -> Vec<FdEntry> {
    let out: Mutex<Vec<FdEntry>> = Mutex::new(Vec::new());
    let mut b = WalkBuilder::new(root);
    let ig = !wopts.no_ignore;
    b.hidden(!wopts.hidden)
        .ignore(ig)
        .git_ignore(ig)
        .git_global(ig)
        .git_exclude(ig)
        .parents(ig)
        .threads(wopts.threads)
        .max_depth(cf.max_depth)
        .filter_entry(|e| e.file_name() != crate::index::DIR_NAME);
    if ig {
        b.add_custom_ignore_filename(".rgignore"); // as ripgrep does
    }

    b.build_parallel().run(|| {
        let mut g = Flush {
            out: &out,
            buf: Vec::new(),
        };
        Box::new(move |res| {
            let Ok(ent) = res else {
                return WalkState::Continue;
            };
            let Some(ft) = ent.file_type() else {
                return WalkState::Continue;
            };
            let is_dir = ft.is_dir();
            if ent.depth() == 0 || (!is_dir && !ft.is_file()) {
                return WalkState::Continue;
            }
            if is_dir && !cf.include_dirs {
                return WalkState::Continue;
            }
            if !is_dir && !cf.include_files {
                return WalkState::Continue;
            }
            let rel = rel_path(root, ent.path());
            let (sz, mt) = if cf.need_meta {
                match ent.metadata() {
                    Ok(m) => ((!is_dir).then_some(m.len()), Some(mtime_ns(&m))),
                    Err(_) => return WalkState::Continue,
                }
            } else {
                (None, None)
            };
            if cf.matches_entry(&rel, is_dir, ent.depth(), sz, mt) {
                g.buf.push(FdEntry {
                    file_type: (!is_dir).then(|| FileType::from_path(Path::new(&rel))),
                    path: rel,
                    kind: if is_dir { "dir" } else { "file" },
                    size: sz,
                    mtime_ns: mt,
                });
            }
            WalkState::Continue
        })
    });
    out.into_inner().unwrap()
}

#[derive(Default)]
struct TreeNode {
    is_dir: bool,
    children: BTreeMap<String, TreeNode>,
}

impl TreeNode {
    fn insert(&mut self, parts: &[&str], is_dir: bool) {
        if parts.is_empty() {
            return;
        }
        let child = self.children.entry(parts[0].to_string()).or_default();
        if parts.len() == 1 {
            child.is_dir |= is_dir;
        } else {
            child.is_dir = true;
            child.insert(&parts[1..], is_dir);
        }
    }
}

pub fn render_tree(root: &Path, entries: &[FdEntry]) -> String {
    let mut trie = TreeNode::default();
    for e in entries {
        let parts: Vec<&str> = e.path.split('/').filter(|p| !p.is_empty()).collect();
        trie.insert(&parts, e.kind == "dir");
    }
    let label = root.to_str().filter(|s| !s.is_empty()).unwrap_or(".");
    let mut out = format!("{}/\n", label.trim_end_matches('/'));
    render_children(&trie.children, "", &mut out);
    out
}

fn render_children(children: &BTreeMap<String, TreeNode>, prefix: &str, out: &mut String) {
    let len = children.len();
    for (idx, (name, node)) in children.iter().enumerate() {
        let last = idx + 1 == len;
        let conn = if last { "└── " } else { "├── " };
        let suffix = if node.is_dir || !node.children.is_empty() {
            "/"
        } else {
            ""
        };
        out.push_str(prefix);
        out.push_str(conn);
        out.push_str(name);
        out.push_str(suffix);
        out.push('\n');
        if !node.children.is_empty() {
            let next_prefix = format!("{prefix}{}", if last { "    " } else { "│   " });
            render_children(&node.children, &next_prefix, out);
        }
    }
}

pub fn print_fd_human(
    resp: &FdResponse,
    root: &Path,
    tree: bool,
    w: &mut impl Write,
) -> io::Result<()> {
    if let Some(err) = &resp.error {
        eprintln!("error: {}", err.message);
        return Ok(());
    }
    if tree {
        if resp.entries.is_empty() {
            return Ok(());
        }
        return write!(w, "{}", render_tree(root, &resp.entries));
    }
    for e in &resp.entries {
        if e.kind == "dir" {
            writeln!(w, "{}/", e.path)?;
        } else {
            writeln!(w, "{}", e.path)?;
        }
    }
    Ok(())
}

fn ms(t0: Instant) -> f64 {
    t0.elapsed().as_secs_f64() * 1000.0
}
