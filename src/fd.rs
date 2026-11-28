//! `lancet fd`: fd 10.5's matching, filters and output, on lancet's walker (and
//! on the index's file list with `--trust-index`).
//!
//! As in fd: the pattern is a regex matched against the file name (`-p`: the
//! absolute path), smart case, `-g` / `-F` / `--exact` turn it into a glob, a
//! substring or a whole name; `-t`, `-e`, `-S`, `--changed-*`, `-o`, depth,
//! `-E` and `--prune` filter. Output is sorted by path (fd's order is
//! whatever its threads produce).

use crate::index::{self, Index};
use crate::output::{ErrorInfo, Status};
use crate::query::FileType;
use crate::scope::IndexMode;
use crate::walk::{mtime_ns, rel_path, WalkOpts};
use ignore::WalkState;
use regex::bytes::{Regex, RegexBuilder};
use regex_syntax::hir::{Class, Hir, HirKind};
use serde::Serialize;
use std::collections::{BTreeMap, HashMap};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum PatternMode {
    /// fd's default.
    #[default]
    Regex,
    /// `-g`
    Glob,
    /// `-F`: a substring of the name.
    Fixed,
    /// `--exact`: the whole name.
    Exact,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum CaseMode {
    /// Case-insensitive unless a pattern has an uppercase character.
    #[default]
    Smart,
    Sensitive,
    Insensitive,
}

/// `-t` kinds. `None` in [`FdOptions::types`] means every kind.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Types {
    pub file: bool,
    pub dir: bool,
    pub symlink: bool,
    pub socket: bool,
    pub pipe: bool,
    pub block: bool,
    pub char: bool,
    /// `x`: executables only (implies files unless another kind is given).
    pub executable: bool,
    /// `e`: empty files and directories only.
    pub empty: bool,
}

impl Types {
    /// fd's `-t` values, combined as fd combines them.
    pub fn parse(values: &[String]) -> Result<Option<Types>, String> {
        if values.is_empty() {
            return Ok(None);
        }
        let mut t = Types::default();
        for v in values {
            match v.as_str() {
                "f" | "file" => t.file = true,
                "d" | "dir" | "directory" => t.dir = true,
                "l" | "symlink" => t.symlink = true,
                "s" | "socket" => t.socket = true,
                "p" | "pipe" => t.pipe = true,
                "b" | "block-device" => t.block = true,
                "c" | "char-device" => t.char = true,
                "x" | "executable" => {
                    t.executable = true;
                    t.file = true;
                }
                "e" | "empty" => t.empty = true,
                other => return Err(format!("invalid value '{other}' for '--type <filetype>'")),
            }
        }
        if t.empty && !(t.file || t.dir) {
            t.file = true;
            t.dir = true;
        }
        Ok(Some(t))
    }

    fn only_files_and_dirs(&self) -> bool {
        !(self.symlink || self.socket || self.pipe || self.block || self.char)
            && !self.executable
            && !self.empty
    }
}

/// `-S [+-]NUM UNIT`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SizeFilter {
    Min(u64),
    Max(u64),
    Equals(u64),
}

impl SizeFilter {
    pub fn parse(raw: &str) -> Result<SizeFilter, String> {
        let bad = || format!("'{raw}' is not a valid size constraint. See 'fd --help'.");
        let (op, rest) = match raw.as_bytes().first() {
            Some(b'+') => ('+', &raw[1..]),
            Some(b'-') => ('-', &raw[1..]),
            _ => ('=', raw),
        };
        let split = rest.find(|c: char| !c.is_ascii_digit()).ok_or_else(bad)?;
        let (num, unit) = rest.split_at(split);
        let n: u64 = num.parse().map_err(|_| bad())?;
        let mult: u64 = match unit.to_ascii_lowercase().as_str() {
            "b" => 1,
            "k" | "kb" => 1_000,
            "ki" | "kib" => 1 << 10,
            "m" | "mb" => 1_000_000,
            "mi" | "mib" => 1 << 20,
            "g" | "gb" => 1_000_000_000,
            "gi" | "gib" => 1 << 30,
            "t" | "tb" => 1_000_000_000_000,
            "ti" | "tib" => 1 << 40,
            _ => return Err(bad()),
        };
        let bytes = n.checked_mul(mult).ok_or_else(bad)?;
        Ok(match op {
            '+' => SizeFilter::Min(bytes),
            '-' => SizeFilter::Max(bytes),
            _ => SizeFilter::Equals(bytes),
        })
    }

    fn accepts(&self, size: u64) -> bool {
        match *self {
            SizeFilter::Min(b) => size >= b,
            SizeFilter::Max(b) => size <= b,
            SizeFilter::Equals(b) => size == b,
        }
    }
}

/// `-o [!]user[:[!]group]` (Unix).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct OwnerFilter {
    uid: Option<(bool, u32)>,
    gid: Option<(bool, u32)>,
}

impl OwnerFilter {
    pub fn parse(raw: &str) -> Result<OwnerFilter, String> {
        let (u, g) = match raw.split_once(':') {
            Some((u, g)) => (u, g),
            None => (raw, ""),
        };
        let part = |s: &str, user: bool| -> Result<Option<(bool, u32)>, String> {
            let (neg, name) = match s.strip_prefix('!') {
                Some(n) => (true, n),
                None => (false, s),
            };
            if name.is_empty() {
                return Ok(None);
            }
            let id = match name.parse::<u32>() {
                Ok(id) => id,
                Err(_) => lookup_id(name, user)?,
            };
            Ok(Some((neg, id)))
        };
        let f = OwnerFilter {
            uid: part(u, true)?,
            gid: part(g, false)?,
        };
        if f.uid.is_none() && f.gid.is_none() {
            return Err(format!(
                "'{raw}' is not a valid user/group specifier. See 'fd --help'."
            ));
        }
        Ok(f)
    }

    fn accepts(&self, uid: u32, gid: u32) -> bool {
        let ok =
            |want: Option<(bool, u32)>, have: u32| want.is_none_or(|(neg, id)| (id == have) != neg);
        ok(self.uid, uid) && ok(self.gid, gid)
    }
}

#[cfg(unix)]
fn lookup_id(name: &str, user: bool) -> Result<u32, String> {
    let c = std::ffi::CString::new(name).map_err(|_| format!("'{name}' is not a valid name"))?;
    // SAFETY: getpwnam/getgrnam return a pointer into static storage or null; we read
    // one field right away on this thread.
    unsafe {
        if user {
            let p = libc::getpwnam(c.as_ptr());
            if p.is_null() {
                return Err(format!("Unknown user '{name}'"));
            }
            Ok((*p).pw_uid)
        } else {
            let g = libc::getgrnam(c.as_ptr());
            if g.is_null() {
                return Err(format!("Unknown group '{name}'"));
            }
            Ok((*g).gr_gid)
        }
    }
}

#[cfg(not(unix))]
fn lookup_id(_name: &str, _user: bool) -> Result<u32, String> {
    Err("--owner is only supported on Unix".into())
}

/// A point in time for `--changed-within` / `--changed-before`: a duration back
/// from now (`10h`, `2weeks`, `1d 12h`), `@UNIX_SECONDS`, or a local date
/// `YYYY-MM-DD[ HH:MM:SS]`. Nanoseconds since the epoch.
pub fn parse_time(raw: &str, now_ns: u64) -> Result<u64, String> {
    let bad = || format!("'{raw}' is not a valid date or duration. See 'fd --help'.");
    if let Some(ts) = raw.strip_prefix('@') {
        let secs: i64 = ts.parse().map_err(|_| bad())?;
        return Ok((secs.max(0) as u64).saturating_mul(1_000_000_000));
    }
    if let Ok(d) = parse_duration(raw) {
        return Ok(now_ns.saturating_sub(d));
    }
    local_date_ns(raw).ok_or_else(bad)
}

/// A duration in humantime's syntax (what fd accepts): `<number><unit>` parts,
/// optionally separated by spaces. Nanoseconds.
pub fn parse_duration(raw: &str) -> Result<u64, String> {
    let bad = || format!("'{raw}' is not a valid duration");
    let mut total: u64 = 0;
    let mut rest = raw.trim();
    if rest.is_empty() {
        return Err(bad());
    }
    while !rest.is_empty() {
        let digits = rest
            .find(|c: char| !c.is_ascii_digit())
            .unwrap_or(rest.len());
        if digits == 0 {
            return Err(bad());
        }
        let n: u64 = rest[..digits].parse().map_err(|_| bad())?;
        rest = &rest[digits..];
        let unit_len = rest
            .find(|c: char| !c.is_ascii_alphabetic())
            .unwrap_or(rest.len());
        let unit = &rest[..unit_len];
        rest = rest[unit_len..].trim_start();
        let ns: u64 = match unit {
            "nanos" | "nsec" | "ns" => 1,
            "usec" | "us" => 1_000,
            "millis" | "msec" | "ms" => 1_000_000,
            "seconds" | "second" | "secs" | "sec" | "s" => 1_000_000_000,
            "minutes" | "minute" | "mins" | "min" | "m" => 60_000_000_000,
            "hours" | "hour" | "hrs" | "hr" | "h" => 3_600_000_000_000,
            "days" | "day" | "d" => 86_400_000_000_000,
            "weeks" | "week" | "w" => 604_800_000_000_000,
            "months" | "month" | "M" => 2_630_016_000_000_000,
            "years" | "year" | "y" => 31_557_600_000_000_000,
            _ => return Err(bad()),
        };
        total = n
            .checked_mul(ns)
            .and_then(|v| total.checked_add(v))
            .ok_or_else(bad)?;
    }
    Ok(total)
}

#[cfg(unix)]
fn local_date_ns(raw: &str) -> Option<u64> {
    let (date, time) = match raw.split_once(' ') {
        Some((d, t)) => (d, t),
        None => (raw, "00:00:00"),
    };
    let d: Vec<i32> = date
        .split('-')
        .map(|x| x.parse().ok())
        .collect::<Option<_>>()?;
    let t: Vec<i32> = time
        .split(':')
        .map(|x| x.parse().ok())
        .collect::<Option<_>>()?;
    if d.len() != 3 || t.len() != 3 || !(1..=12).contains(&d[1]) || !(1..=31).contains(&d[2]) {
        return None;
    }
    // SAFETY: `tm` is fully initialized (zeroed, then the fields mktime reads);
    // mktime only reads and normalizes it.
    let secs = unsafe {
        let mut tm: libc::tm = std::mem::zeroed();
        tm.tm_year = d[0] - 1900;
        tm.tm_mon = d[1] - 1;
        tm.tm_mday = d[2];
        tm.tm_hour = t[0];
        tm.tm_min = t[1];
        tm.tm_sec = t[2];
        tm.tm_isdst = -1;
        libc::mktime(&mut tm)
    };
    (secs >= 0).then(|| secs as u64 * 1_000_000_000)
}

#[cfg(not(unix))]
fn local_date_ns(_raw: &str) -> Option<u64> {
    // shortcut: no local-time conversion without libc; dates need Unix, durations and @timestamps work everywhere.
    None
}

/// Smart case: does the regex `pattern` contain an uppercase character (in a
/// literal or a class bound)? Escapes like `\S` don't count. As fd does it.
pub fn pattern_has_uppercase_char(pattern: &str) -> bool {
    fn walk(h: &Hir) -> bool {
        match h.kind() {
            HirKind::Literal(l) => match std::str::from_utf8(&l.0) {
                Ok(s) => s.chars().any(char::is_uppercase),
                Err(_) => l.0.iter().any(|b| char::from(*b).is_uppercase()),
            },
            HirKind::Class(Class::Unicode(c)) => c
                .ranges()
                .iter()
                .any(|r| r.start().is_uppercase() || r.end().is_uppercase()),
            HirKind::Class(Class::Bytes(c)) => c.ranges().iter().any(|r| {
                char::from(r.start()).is_uppercase() || char::from(r.end()).is_uppercase()
            }),
            HirKind::Capture(c) => walk(&c.sub),
            HirKind::Repetition(r) => walk(&r.sub),
            HirKind::Concat(hs) | HirKind::Alternation(hs) => hs.iter().any(walk),
            _ => false,
        }
    }
    regex_syntax::ParserBuilder::new()
        .utf8(false)
        .build()
        .parse(pattern)
        .map(|h| walk(&h))
        .unwrap_or(false)
}

#[derive(Clone, Debug)]
pub struct FdOptions {
    pub root: PathBuf,
    /// The pattern and every `--and` pattern; all must match. Empty: match all.
    pub patterns: Vec<String>,
    pub pattern_mode: PatternMode,
    pub case: CaseMode,
    /// `-p`: match against the absolute path instead of the name.
    pub full_path: bool,
    pub types: Option<Types>,
    pub exts: Vec<String>,
    /// lancet's `--type-class` (files only).
    pub file_types: Vec<FileType>,
    pub min_depth: Option<usize>,
    pub max_depth: Option<usize>,
    /// `-E` globs: excluded, and not descended into.
    pub excludes: Vec<String>,
    pub sizes: Vec<SizeFilter>,
    /// `--changed-within`: modified after this (ns since the epoch).
    pub changed_after_ns: Option<u64>,
    /// `--changed-before`: modified before this.
    pub changed_before_ns: Option<u64>,
    pub owner: Option<OwnerFilter>,
    /// `--prune`: don't descend into directories that match.
    pub prune: bool,
    /// `--ignore-contain NAME`: skip directories that contain NAME.
    pub ignore_contain: Vec<String>,
    /// `--max-results` (the first N in path order).
    pub limit: Option<usize>,
    pub tree: bool,
    pub walk: WalkOpts,
    pub index: IndexMode,
    pub index_dir: Option<PathBuf>,
    /// Report size and mtime in JSON entries.
    pub need_meta: bool,
}

impl FdOptions {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        FdOptions {
            root: root.into(),
            patterns: Vec::new(),
            pattern_mode: PatternMode::Regex,
            case: CaseMode::Smart,
            full_path: false,
            types: None,
            exts: Vec::new(),
            file_types: Vec::new(),
            min_depth: None,
            max_depth: None,
            excludes: Vec::new(),
            sizes: Vec::new(),
            changed_after_ns: None,
            changed_before_ns: None,
            owner: None,
            prune: false,
            ignore_contain: Vec::new(),
            limit: None,
            tree: false,
            walk: WalkOpts::default(),
            index: IndexMode::Auto,
            index_dir: None,
            need_meta: false,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct FdEntry {
    /// Relative to the search root, `/`-separated.
    pub path: String,
    /// `file`, `dir`, `symlink`, `socket`, `pipe`, `block` or `char`.
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

/// Everything the matcher needs to know about one entry; metadata is read
/// only when a filter asks for it.
struct Candidate<'a> {
    rel: &'a str,
    name: &'a [u8],
    path: &'a Path,
    kind: &'static str,
    depth: usize,
    meta: &'a dyn Fn() -> Option<std::fs::Metadata>,
    /// Size and mtime known without a stat (from the index).
    known: Option<(u64, u64)>,
}

struct Matcher {
    regexes: Vec<Regex>,
    full_path: bool,
    cwd: PathBuf,
    exts: Vec<String>,
    types: Option<Types>,
    file_types: Vec<FileType>,
    min_depth: usize,
    sizes: Vec<SizeFilter>,
    after: Option<u64>,
    before: Option<u64>,
    owner: Option<OwnerFilter>,
    need_meta: bool,
}

impl Matcher {
    fn compile(o: &FdOptions) -> Result<Matcher, String> {
        let mut sources = Vec::new();
        for p in o.patterns.iter().filter(|p| !p.is_empty()) {
            sources.push(match o.pattern_mode {
                PatternMode::Regex => p.clone(),
                PatternMode::Fixed => regex::escape(p),
                PatternMode::Exact => format!("^{}$", regex::escape(p)),
                PatternMode::Glob => globset::GlobBuilder::new(p)
                    .literal_separator(o.full_path)
                    .build()
                    .map_err(|e| format!("{e}"))?
                    .regex()
                    .to_owned(),
            });
        }
        let case_sensitive = match o.case {
            CaseMode::Sensitive => true,
            CaseMode::Insensitive => false,
            CaseMode::Smart => sources.iter().any(|s| pattern_has_uppercase_char(s)),
        };
        let regexes = sources
            .iter()
            .map(|s| {
                RegexBuilder::new(s)
                    .case_insensitive(!case_sensitive)
                    .dot_matches_new_line(true)
                    .build()
                    .map_err(|e| {
                        let glob_like = o.pattern_mode == PatternMode::Regex
                            && (s.starts_with('*') || s.contains("**"));
                        if glob_like {
                            format!("{e}\n(the pattern is a regex; for a glob add -g: lancet fd -g '{s}')")
                        } else {
                            e.to_string()
                        }
                    })
            })
            .collect::<Result<_, _>>()?;
        Ok(Matcher {
            regexes,
            full_path: o.full_path,
            cwd: std::env::current_dir().unwrap_or_default(),
            exts: o
                .exts
                .iter()
                .map(|e| e.trim_start_matches('.').to_lowercase())
                .filter(|e| !e.is_empty())
                .collect(),
            types: o.types,
            file_types: o.file_types.clone(),
            min_depth: o.min_depth.unwrap_or(0),
            sizes: o.sizes.clone(),
            after: o.changed_after_ns,
            before: o.changed_before_ns,
            owner: o.owner,
            need_meta: o.need_meta,
        })
    }

    /// The absolute path fd matches `-p` against: the cwd joined with the path,
    /// without resolving symlinks.
    fn absolute(&self, p: &Path) -> PathBuf {
        if p.is_absolute() {
            return p.to_path_buf();
        }
        let p = p.strip_prefix(".").unwrap_or(p);
        self.cwd.join(p)
    }

    /// `Some((size, mtime))` to report when `c` passes every filter.
    fn accepts(&self, c: &Candidate) -> Option<(Option<u64>, Option<u64>)> {
        if c.depth < self.min_depth {
            return None;
        }
        if !self.regexes.is_empty() {
            let abs;
            let hay: &[u8] = if self.full_path {
                abs = self.absolute(c.path).to_string_lossy().into_owned();
                abs.as_bytes()
            } else {
                c.name
            };
            if !self.regexes.iter().all(|r| r.is_match(hay)) {
                return None;
            }
        }
        if !self.exts.is_empty() {
            let name = String::from_utf8_lossy(c.name).to_lowercase();
            let ok = self.exts.iter().any(|e| {
                name.len() > e.len() + 1
                    && name.ends_with(e.as_str())
                    && name.as_bytes()[name.len() - e.len() - 1] == b'.'
            });
            if !ok {
                return None;
            }
        }
        let mut meta: Option<Option<std::fs::Metadata>> = None;
        let mut md = || meta.get_or_insert_with(|| (c.meta)()).clone();
        if let Some(t) = &self.types {
            let kind_ok = match c.kind {
                "file" => t.file,
                "dir" => t.dir,
                "symlink" => t.symlink,
                "socket" => t.socket,
                "pipe" => t.pipe,
                "block" => t.block,
                "char" => t.char,
                _ => false,
            };
            if !kind_ok {
                return None;
            }
            if t.executable && !md().is_some_and(|m| is_executable(&m)) {
                return None;
            }
            if t.empty {
                let empty = match c.kind {
                    "file" => md().is_some_and(|m| m.len() == 0),
                    "dir" => std::fs::read_dir(c.path).is_ok_and(|mut d| d.next().is_none()),
                    _ => false,
                };
                if !empty {
                    return None;
                }
            }
        }
        if !self.file_types.is_empty()
            && (c.kind != "file"
                || !self
                    .file_types
                    .contains(&FileType::from_path(Path::new(c.rel))))
        {
            return None;
        }
        if let Some(o) = &self.owner {
            if !md().is_some_and(|m| owner_of(&m).is_some_and(|(u, g)| o.accepts(u, g))) {
                return None;
            }
        }
        let mut size = None;
        let mut mtime = None;
        if !self.sizes.is_empty() {
            // As fd: only files (symlinks followed) have a size to compare.
            let len = match c.known {
                Some((s, _)) => Some(s),
                None => std::fs::metadata(c.path)
                    .ok()
                    .filter(|m| m.is_file())
                    .map(|m| m.len()),
            };
            match len {
                Some(l) if self.sizes.iter().all(|f| f.accepts(l)) => size = Some(l),
                _ => return None,
            }
        }
        if self.after.is_some() || self.before.is_some() || self.need_meta {
            let mt = match c.known {
                Some((_, m)) => Some(m),
                None => md().map(|m| mtime_ns(&m)),
            };
            if self.after.is_some_and(|a| mt.is_none_or(|m| m <= a))
                || self.before.is_some_and(|b| mt.is_none_or(|m| m >= b))
            {
                return None;
            }
            mtime = mt;
            if self.need_meta && size.is_none() && c.kind == "file" {
                size = c.known.map(|k| k.0).or_else(|| md().map(|m| m.len()));
            }
        }
        Some((size, mtime))
    }
}

#[cfg(unix)]
fn is_executable(m: &std::fs::Metadata) -> bool {
    use std::os::unix::fs::PermissionsExt;
    m.permissions().mode() & 0o111 != 0
}

#[cfg(not(unix))]
fn is_executable(_m: &std::fs::Metadata) -> bool {
    false
}

#[cfg(unix)]
fn owner_of(m: &std::fs::Metadata) -> Option<(u32, u32)> {
    use std::os::unix::fs::MetadataExt;
    Some((m.uid(), m.gid()))
}

#[cfg(not(unix))]
fn owner_of(_m: &std::fs::Metadata) -> Option<(u32, u32)> {
    None
}

fn kind_of(ft: &std::fs::FileType) -> &'static str {
    #[cfg(unix)]
    {
        use std::os::unix::fs::FileTypeExt;
        if ft.is_socket() {
            return "socket";
        }
        if ft.is_fifo() {
            return "pipe";
        }
        if ft.is_block_device() {
            return "block";
        }
        if ft.is_char_device() {
            return "char";
        }
    }
    if ft.is_symlink() {
        "symlink"
    } else if ft.is_dir() {
        "dir"
    } else {
        "file"
    }
}

/// Search one root. Paths in the result are relative to it.
pub fn find_files(opts: &FdOptions) -> FdResponse {
    let t0 = Instant::now();
    if !opts.root.is_dir() {
        return FdResponse::failure(
            Status::InvalidQuery,
            format!(
                "Search path \"{}\" is not a directory.",
                opts.root.display()
            ),
            0.0,
        );
    }
    let m = match Matcher::compile(opts) {
        Ok(m) => m,
        Err(e) => return FdResponse::failure(Status::InvalidQuery, e, ms(t0)),
    };
    let mut warnings = Vec::new();
    let mut mode = "scan".to_string();
    let mut entries: Option<Vec<FdEntry>> = None;
    if opts.index == IndexMode::Trust {
        let listable = opts.types.is_none_or(|t| t.only_files_and_dirs())
            && !opts.walk.follow
            && opts.excludes.is_empty()
            && opts.ignore_contain.is_empty()
            && opts.owner.is_none()
            && !opts.prune;
        match index::discover(&opts.root, opts.index_dir.as_deref()) {
            Ok(Some((ix, rel_prefix)))
                if listable && crate::scope::index_matches_walk(&ix, &opts.walk) =>
            {
                mode = "index_trusted".into();
                entries = Some(collect_from_index(&ix, &opts.root, &rel_prefix, &m, opts));
            }
            Ok(Some(_)) => {}
            Ok(None) => warnings
                .push("no index found; fell back to a full scan (run `lancet index`)".into()),
            Err(e) => warnings.push(format!("index unusable ({e:#}); fell back to a full scan")),
        }
    }
    let mut entries = entries.unwrap_or_else(|| collect_from_walk(&opts.root, opts, &m));
    entries.sort_by(|a, b| a.path.cmp(&b.path));
    let total = entries.len();
    let truncated = opts.limit.is_some_and(|l| total > l);
    if let Some(l) = opts.limit {
        entries.truncate(l);
    }
    FdResponse {
        status: if total == 0 {
            Status::Empty
        } else {
            Status::Complete
        },
        mode,
        total,
        returned: entries.len(),
        truncated,
        entries,
        warnings,
        error: None,
        elapsed_ms: ms(t0),
    }
}

/// The index holds regular files; directories are their parents (so empty
/// directories don't appear: `--trust-index` trades them for speed).
fn collect_from_index(
    ix: &Index,
    root: &Path,
    rel_prefix: &str,
    m: &Matcher,
    o: &FdOptions,
) -> Vec<FdEntry> {
    let no_meta = || None;
    let want_dirs = o.types.is_none_or(|t| t.dir);
    let mut dirs: HashMap<String, u64> = HashMap::new();
    let mut out = Vec::new();
    for (si, seg) in ix.segs.iter().enumerate() {
        for id in 0..seg.ndocs() as u32 {
            if ix.is_dead(si, id) {
                continue;
            }
            let doc_rel = seg.doc_rel(id);
            let local: &str = if rel_prefix.is_empty() {
                &doc_rel
            } else {
                match doc_rel
                    .strip_prefix(rel_prefix)
                    .and_then(|r| r.strip_prefix('/'))
                {
                    Some(r) => r,
                    None => continue,
                }
            };
            let depth = local.split('/').count();
            if o.max_depth.is_some_and(|d| depth > d) {
                continue;
            }
            let meta = seg.doc_meta(id);
            if want_dirs {
                let mut cur = local;
                while let Some(pos) = cur.rfind('/') {
                    cur = &cur[..pos];
                    let e = dirs.entry(cur.to_string()).or_insert(0);
                    *e = (*e).max(meta.mtime_ns);
                }
            }
            let path = root.join(local);
            let name = local.rsplit('/').next().unwrap_or(local).as_bytes();
            let c = Candidate {
                rel: local,
                name,
                path: &path,
                kind: "file",
                depth,
                meta: &no_meta,
                known: Some((meta.size, meta.mtime_ns)),
            };
            if let Some((size, mtime)) = m.accepts(&c) {
                out.push(FdEntry {
                    file_type: Some(FileType::from_path(Path::new(local))),
                    path: local.to_string(),
                    kind: "file",
                    size: size.filter(|_| o.need_meta),
                    mtime_ns: mtime.filter(|_| o.need_meta),
                });
            }
        }
    }
    for (dir, mt) in dirs {
        let depth = dir.split('/').count();
        let path = root.join(&dir);
        let name = dir.rsplit('/').next().unwrap_or(&dir).as_bytes().to_vec();
        let c = Candidate {
            rel: &dir,
            name: &name,
            path: &path,
            kind: "dir",
            depth,
            meta: &no_meta,
            known: None,
        };
        // Directories have no size; a size filter rejects them, as in a walk.
        if !o.sizes.is_empty() {
            continue;
        }
        let c_known = Candidate {
            known: Some((0, mt)),
            ..c
        };
        if m.accepts(&c_known).is_some() {
            out.push(FdEntry {
                path: dir.clone(),
                kind: "dir",
                file_type: None,
                size: None,
                mtime_ns: o.need_meta.then_some(mt),
            });
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

fn collect_from_walk(root: &Path, o: &FdOptions, m: &Matcher) -> Vec<FdEntry> {
    let out: Mutex<Vec<FdEntry>> = Mutex::new(Vec::new());
    let mut wopts = o.walk.clone();
    wopts.max_depth = o.max_depth;
    let mut b = crate::walk::builder(root, &wopts);
    if !wopts.no_ignore && !wopts.ignore.no_dot {
        b.add_custom_ignore_filename(".fdignore"); // as fd does
    }
    if !o.excludes.is_empty() {
        let mut ob = ignore::overrides::OverrideBuilder::new(root);
        for e in &o.excludes {
            if ob.add(&format!("!{e}")).is_err() {
                eprintln!("lancet fd: invalid --exclude glob `{e}`");
            }
        }
        if let Ok(ov) = ob.build() {
            b.overrides(ov);
        }
    }
    let contain = o.ignore_contain.clone();
    b.filter_entry(move |e| {
        e.file_name() != index::DIR_NAME
            && (contain.is_empty()
                || !e.file_type().is_some_and(|t| t.is_dir())
                || !contain
                    .iter()
                    .any(|n| e.path().join(n).symlink_metadata().is_ok()))
    });
    b.build_parallel().run(|| {
        let mut g = Flush {
            out: &out,
            buf: Vec::new(),
        };
        Box::new(move |res| {
            let Ok(ent) = res else {
                return WalkState::Continue;
            };
            if ent.depth() == 0 {
                return WalkState::Continue;
            }
            let Some(ft) = ent.file_type() else {
                return WalkState::Continue;
            };
            let kind = kind_of(&ft);
            let rel = rel_path(root, ent.path());
            let name = ent.file_name().to_string_lossy();
            let meta = || ent.metadata().ok();
            let c = Candidate {
                rel: &rel,
                name: name.as_bytes(),
                path: ent.path(),
                kind,
                depth: ent.depth(),
                meta: &meta,
                known: None,
            };
            let Some((size, mtime)) = m.accepts(&c) else {
                return WalkState::Continue;
            };
            let pruned = o.prune && kind == "dir";
            g.buf.push(FdEntry {
                file_type: (kind == "file").then(|| FileType::from_path(Path::new(&rel))),
                path: rel,
                kind,
                size: size.filter(|_| o.need_meta),
                mtime_ns: mtime.filter(|_| o.need_meta),
            });
            if pruned {
                WalkState::Skip
            } else {
                WalkState::Continue
            }
        })
    });
    out.into_inner().unwrap_or_default()
}

/// Nanoseconds since the epoch, now.
pub fn now_ns() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(0)
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

/// lancet's `--tree`: the entries drawn as an indented tree under the root.
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
        out.push_str(prefix);
        out.push_str(if last { "└── " } else { "├── " });
        out.push_str(name);
        if node.is_dir || !node.children.is_empty() {
            out.push('/');
        }
        out.push('\n');
        if !node.children.is_empty() {
            let next = format!("{prefix}{}", if last { "    " } else { "│   " });
            render_children(&node.children, &next, out);
        }
    }
}

/// How fd prints one result.
#[derive(Clone, Debug, Default)]
pub struct OutputStyle {
    /// The search path was given (so it prefixes results as written).
    pub explicit_root: bool,
    /// Keep `./` on results of an implicit `.` root (fd: with `-0`, or `--strip-cwd-prefix=never`).
    pub keep_dot_prefix: bool,
    pub absolute: bool,
    pub separator: Option<String>,
    pub format: Option<String>,
    pub null: bool,
}

/// The text fd prints for `e` found under `root`, without the terminator.
pub fn fd_line(root: &Path, e: &FdEntry, style: &OutputStyle, cwd: &Path) -> String {
    let base: PathBuf = if style.explicit_root {
        root.to_path_buf()
    } else {
        PathBuf::from(".")
    };
    let joined = base.join(&e.path);
    let mut s = if style.absolute || joined.is_absolute() {
        let p = joined.strip_prefix(".").unwrap_or(&joined);
        cwd.join(p).to_string_lossy().into_owned()
    } else {
        let j = joined.to_string_lossy().into_owned();
        match j.strip_prefix("./") {
            // fd keeps `./` on a name starting with `-`, so it can't pass for a flag.
            Some(rest)
                if !style.explicit_root && !style.keep_dot_prefix && !rest.starts_with('-') =>
            {
                rest.to_string()
            }
            _ => j,
        }
    };
    s = s.replace('\\', "/");
    if let Some(fmt) = &style.format {
        s = apply_format(fmt, &s);
    } else if e.kind == "dir" {
        s.push('/');
    }
    if let Some(sep) = &style.separator {
        if style.format.is_none() {
            s = s.replace('/', sep);
        }
    }
    s
}

/// fd's `--format` placeholders: `{}` `{/}` `{//}` `{.}` `{/.}`, `{{` and `}}`.
fn apply_format(fmt: &str, path: &str) -> String {
    let p = Path::new(path);
    let base = p
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.to_string());
    let parent = match p.parent().map(|d| d.to_string_lossy().into_owned()) {
        Some(d) if !d.is_empty() => d,
        _ => ".".to_string(),
    };
    let no_ext = |s: &str| -> String {
        let q = Path::new(s);
        match (q.file_stem(), q.extension()) {
            (Some(stem), Some(_)) => match q.parent().map(|d| d.to_string_lossy().into_owned()) {
                Some(d) if !d.is_empty() => format!("{d}/{}", stem.to_string_lossy()),
                _ => stem.to_string_lossy().into_owned(),
            },
            _ => s.to_string(),
        }
    };
    let mut out = String::new();
    let mut rest = fmt;
    while !rest.is_empty() {
        let (tok, rep): (&str, String) = if rest.starts_with("{{") {
            ("{{", "{".into())
        } else if rest.starts_with("}}") {
            ("}}", "}".into())
        } else if rest.starts_with("{//}") {
            ("{//}", parent.clone())
        } else if rest.starts_with("{/.}") {
            ("{/.}", no_ext(&base))
        } else if rest.starts_with("{/}") {
            ("{/}", base.clone())
        } else if rest.starts_with("{.}") {
            ("{.}", no_ext(path))
        } else if rest.starts_with("{}") {
            ("{}", path.to_string())
        } else {
            let c = rest.chars().next().unwrap_or_default();
            (&rest[..c.len_utf8()], c.to_string())
        };
        out.push_str(&rep);
        rest = &rest[tok.len()..];
    }
    out
}

pub fn print_fd_human(
    resp: &FdResponse,
    root: &Path,
    tree: bool,
    style: &OutputStyle,
    w: &mut impl Write,
) -> io::Result<()> {
    if tree {
        if resp.entries.is_empty() {
            return Ok(());
        }
        return write!(w, "{}", render_tree(root, &resp.entries));
    }
    let cwd = std::env::current_dir().unwrap_or_default();
    let end: &[u8] = if style.null { b"\0" } else { b"\n" };
    for e in &resp.entries {
        w.write_all(fd_line(root, e, style, &cwd).as_bytes())?;
        w.write_all(end)?;
    }
    Ok(())
}

fn ms(t0: Instant) -> f64 {
    t0.elapsed().as_secs_f64() * 1000.0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sizes_parse_as_fd() {
        assert_eq!(SizeFilter::parse("+1k"), Ok(SizeFilter::Min(1000)));
        assert_eq!(SizeFilter::parse("-2ki"), Ok(SizeFilter::Max(2048)));
        assert_eq!(SizeFilter::parse("10b"), Ok(SizeFilter::Equals(10)));
        assert!(SizeFilter::parse("10").is_err());
    }

    #[test]
    fn durations_parse_as_humantime() {
        assert_eq!(parse_duration("1d"), Ok(86_400_000_000_000));
        assert_eq!(parse_duration("2weeks"), Ok(2 * 604_800_000_000_000));
        assert_eq!(parse_duration("1h 30min"), Ok(5_400_000_000_000));
        assert!(parse_duration("1x").is_err());
        assert_eq!(parse_time("@10", 0), Ok(10_000_000_000));
    }

    #[test]
    fn smart_case_ignores_escapes() {
        assert!(!pattern_has_uppercase_char(r"\S+\.rs"));
        assert!(pattern_has_uppercase_char("Main"));
        assert!(pattern_has_uppercase_char("[A-Z]x"));
    }

    #[test]
    fn format_placeholders() {
        assert_eq!(
            apply_format("{} {/} {//} {.} {/.} {{}}", "src/main.rs"),
            "src/main.rs main.rs src src/main main {}"
        );
        assert_eq!(apply_format("{//}", "run.sh"), ".");
    }
}
