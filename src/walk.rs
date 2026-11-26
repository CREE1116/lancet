//! Directory traversal (ripgrep's `ignore` crate: .gitignore, hidden files,
//! no symlink following) plus file-level filters.

use crate::query::{FileType, Filters};
use globset::{Glob, GlobSet, GlobSetBuilder};
use ignore::{WalkBuilder, WalkState};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::UNIX_EPOCH;

#[derive(Clone, Debug, Default)]
pub struct WalkOpts {
    pub hidden: bool,
    pub no_ignore: bool,
    /// 0 = automatic.
    pub threads: usize,
}

#[derive(Clone, Debug)]
pub struct FileEntry {
    pub path: PathBuf,
    /// Path relative to the search root, `/`-separated.
    pub rel: String,
    pub size: u64,
    pub mtime_ns: u64,
    /// Status-change time; unlike mtime it cannot be set back by `cp -p`, `tar`,
    /// `rsync -t` or `touch -r`. 0 where the platform has none (Windows).
    pub ctime_ns: u64,
    /// Inode number (0 where unavailable).
    pub ino: u64,
}

pub struct FileFilter {
    types: Vec<FileType>,
    exts: Vec<String>,
    globs: Option<GlobSet>,
    /// ripgrep `-g/--iglob` semantics (whitelist / `!` negation / directory globs).
    overrides: Option<ignore::overrides::Override>,
    /// ripgrep `-t/-T` file types (ripgrep's default definitions).
    rg_types: Option<ignore::types::Types>,
}

fn has_glob_meta(s: &str) -> bool {
    s.contains(['*', '?', '[', '{'])
}

impl FileFilter {
    pub fn all() -> Self {
        FileFilter {
            types: vec![],
            exts: vec![],
            globs: None,
            overrides: None,
            rg_types: None,
        }
    }

    /// ripgrep-style filter: `globs`/`iglobs` as `-g`/`--iglob` (relative to `root`),
    /// `select`/`negate` as `-t`/`-T` over ripgrep's default type definitions.
    pub fn rg(
        root: &Path,
        globs: &[String],
        iglobs: &[String],
        select: &[String],
        negate: &[String],
    ) -> Result<Self, String> {
        let overrides = if globs.is_empty() && iglobs.is_empty() {
            None
        } else {
            let mut b = ignore::overrides::OverrideBuilder::new(root);
            for g in globs {
                b.add(g).map_err(|e| e.to_string())?;
            }
            b.case_insensitive(true).map_err(|e| e.to_string())?;
            for g in iglobs {
                b.add(g).map_err(|e| e.to_string())?;
            }
            Some(b.build().map_err(|e| e.to_string())?)
        };
        let rg_types = if select.is_empty() && negate.is_empty() {
            None
        } else {
            let mut b = ignore::types::TypesBuilder::new();
            b.add_defaults();
            for t in select {
                b.select(t);
            }
            for t in negate {
                b.negate(t);
            }
            Some(b.build().map_err(|e| e.to_string())?)
        };
        Ok(FileFilter {
            overrides,
            rg_types,
            ..FileFilter::all()
        })
    }

    pub fn new(f: &Filters) -> Result<Self, String> {
        let globs = if f.paths.is_empty() {
            None
        } else {
            let mut b = GlobSetBuilder::new();
            for p in &f.paths {
                let p = p.trim_start_matches("./");
                b.add(Glob::new(p).map_err(|e| e.to_string())?);
                if !has_glob_meta(p) {
                    b.add(
                        Glob::new(&format!("{}/**", p.trim_end_matches('/')))
                            .map_err(|e| e.to_string())?,
                    );
                }
            }
            Some(b.build().map_err(|e| e.to_string())?)
        };
        Ok(FileFilter {
            types: f.types.clone(),
            exts: f.exts.clone(),
            globs,
            overrides: None,
            rg_types: None,
        })
    }

    pub fn accepts(&self, rel: &str) -> bool {
        let p = Path::new(rel);
        if !self.types.is_empty() && !self.types.contains(&FileType::from_path(p)) {
            return false;
        }
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
        if let Some(g) = &self.globs {
            if !g.is_match(rel) {
                return false;
            }
        }
        if let Some(ov) = &self.overrides {
            if ov.matched(p, false).is_ignore() {
                return false;
            }
            // Directory globs (`!target/`) exclude everything below, as in a walk.
            let mut a = p.parent();
            while let Some(d) = a.filter(|d| !d.as_os_str().is_empty()) {
                if ov.matched(d, true).is_ignore() {
                    return false;
                }
                a = d.parent();
            }
        }
        if let Some(t) = &self.rg_types {
            if t.matched(p, false).is_ignore() {
                return false;
            }
        }
        true
    }
}

pub fn rel_path(root: &Path, path: &Path) -> String {
    match path.strip_prefix(root) {
        Ok(r) if !r.as_os_str().is_empty() => r.to_string_lossy().replace('\\', "/"),
        _ => path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default(),
    }
}

/// `(ctime_ns, inode)`; zeros on platforms without them.
pub fn change_stamp(m: &std::fs::Metadata) -> (u64, u64) {
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        let ct = (m.ctime().max(0) as u64)
            .saturating_mul(1_000_000_000)
            .saturating_add(m.ctime_nsec().max(0) as u64);
        (ct, m.ino())
    }
    #[cfg(not(unix))]
    {
        let _ = m;
        (0, 0)
    }
}

pub fn mtime_ns(m: &std::fs::Metadata) -> u64 {
    m.modified()
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(0)
}

struct Flush<'a> {
    out: &'a Mutex<Vec<FileEntry>>,
    buf: Vec<FileEntry>,
}

impl Drop for Flush<'_> {
    fn drop(&mut self) {
        if let Ok(mut o) = self.out.lock() {
            o.append(&mut self.buf);
        }
    }
}

/// Walk `root`, returning accepted files sorted by relative path.
pub fn walk(root: &Path, opts: &WalkOpts, filter: &FileFilter, need_meta: bool) -> Vec<FileEntry> {
    let out: Mutex<Vec<FileEntry>> = Mutex::new(Vec::new());
    let mut b = WalkBuilder::new(root);
    let ig = !opts.no_ignore;
    b.hidden(!opts.hidden)
        .ignore(ig)
        .git_ignore(ig)
        .git_global(ig)
        .git_exclude(ig)
        .parents(ig)
        .threads(opts.threads)
        .filter_entry(|e| e.file_name() != crate::index::DIR_NAME);
    if ig {
        b.add_custom_ignore_filename(".rgignore"); // as ripgrep does
    }
    if let Some(ov) = &filter.overrides {
        // In the walker, as in ripgrep: whitelist globs win over hidden/ignore rules
        // and `!dir/` prunes whole directories.
        b.overrides(ov.clone());
    }
    b.build_parallel().run(|| {
        // Per-thread buffer, flushed into `out` when the visitor is dropped.
        let mut g = Flush {
            out: &out,
            buf: Vec::new(),
        };
        Box::new(move |res| {
            let Ok(ent) = res else {
                return WalkState::Continue;
            };
            if ent.file_type().is_some_and(|t| t.is_file()) {
                let rel = rel_path(root, ent.path());
                if filter.accepts(&rel) {
                    let (size, mtime, (ctime, ino)) = if need_meta {
                        match ent.metadata() {
                            Ok(m) => (m.len(), mtime_ns(&m), change_stamp(&m)),
                            Err(_) => return WalkState::Continue,
                        }
                    } else {
                        (0, 0, (0, 0))
                    };
                    g.buf.push(FileEntry {
                        path: ent.path().to_path_buf(),
                        rel,
                        size,
                        mtime_ns: mtime,
                        ctime_ns: ctime,
                        ino,
                    });
                }
            }
            WalkState::Continue
        })
    });
    let mut v = out.into_inner().unwrap();
    v.sort_unstable_by(|a, b| a.rel.cmp(&b.rel));
    v
}
