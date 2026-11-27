//! Document table-of-contents and source code declaration skeleton (`lancet outline`).

use crate::adapters;
use crate::code;
use crate::content;
use crate::output::{ErrorInfo, Status};
use crate::query::{FileType, Filters};
use crate::walk::{rel_path, FileFilter, WalkOpts};
use ignore::WalkBuilder;
use rayon::prelude::*;
use serde::Serialize;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::time::Instant;

#[derive(Clone, Debug)]
pub struct OutlineOptions {
    pub path: PathBuf,
    pub max_depth: Option<usize>,
    pub exts: Vec<String>,
    pub limit: Option<usize>,
    pub walk: WalkOpts,
    pub ocr: bool,
}

impl OutlineOptions {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        OutlineOptions {
            path: path.into(),
            max_depth: None,
            exts: Vec::new(),
            limit: None,
            walk: WalkOpts::default(),
            ocr: false,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct OutlineItem {
    pub kind: String,
    pub name: String,
    pub depth: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub start_line: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub end_line: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub signature: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct FileOutline {
    pub path: String,
    pub file_type: FileType,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub total_lines: Option<usize>,
    pub items: Vec<OutlineItem>,
}

#[derive(Clone, Debug, Serialize)]
pub struct OutlineResponse {
    pub status: Status,
    pub files: Vec<FileOutline>,
    pub total_items: usize,
    pub truncated: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<ErrorInfo>,
    pub elapsed_ms: f64,
}

impl OutlineResponse {
    /// One response for several paths (`lancet outline a.rs b.rs`), in argument order.
    pub fn merge(mut rs: Vec<OutlineResponse>) -> OutlineResponse {
        if rs.len() == 1 {
            return rs.pop().unwrap();
        }
        let errors: Vec<String> = rs
            .iter()
            .filter_map(|r| r.error.as_ref().map(|e| e.message.clone()))
            .collect();
        let files: Vec<FileOutline> = rs
            .iter_mut()
            .flat_map(|r| std::mem::take(&mut r.files))
            .collect();
        let status = match rs.iter().find(|r| r.error.is_some()) {
            Some(r) => r.status,
            None if files.is_empty() => Status::Empty,
            None => Status::Complete,
        };
        OutlineResponse {
            status,
            total_items: rs.iter().map(|r| r.total_items).sum(),
            truncated: rs.iter().any(|r| r.truncated),
            error: (!errors.is_empty()).then(|| ErrorInfo {
                message: errors.join("; "),
                pos: None,
            }),
            elapsed_ms: rs.iter().map(|r| r.elapsed_ms).sum(),
            files,
        }
    }

    fn failure(status: Status, message: String, elapsed_ms: f64) -> Self {
        OutlineResponse {
            status,
            files: Vec::new(),
            total_items: 0,
            truncated: false,
            error: Some(ErrorInfo { message, pos: None }),
            elapsed_ms,
        }
    }
}

fn byte_to_line(line_starts: &[usize], byte_off: usize) -> usize {
    match line_starts.binary_search(&byte_off) {
        Ok(idx) => idx + 1,
        Err(idx) => idx.max(1),
    }
}

fn truncate_chars(s: &str, max_chars: usize) -> String {
    if s.chars().count() <= max_chars {
        s.to_string()
    } else {
        let mut out: String = s.chars().take(max_chars).collect();
        out.push_str("...");
        out
    }
}

fn extract_code_outline(path: &Path, bytes: &[u8]) -> Option<FileOutline> {
    if !code::ENABLED || !code::supports_path(path) {
        return None;
    }
    let spans = code::extract_symbols(path, bytes)?;
    let mut line_starts = vec![0usize];
    for pos in memchr::memchr_iter(b'\n', bytes) {
        line_starts.push(pos + 1);
    }
    let text = String::from_utf8_lossy(bytes);
    let lines: Vec<&str> = text.lines().collect();

    let mut items = Vec::with_capacity(spans.len());
    for (idx, sp) in spans.iter().enumerate() {
        let Some(name) = &sp.name else { continue };
        let depth = spans
            .iter()
            .enumerate()
            .filter(|&(j, other)| {
                j != idx
                    && other.start <= sp.start
                    && sp.end <= other.end
                    && (other.start, other.end) != (sp.start, sp.end)
            })
            .count();
        let sl = byte_to_line(&line_starts, sp.start);
        let el = byte_to_line(&line_starts, sp.end.saturating_sub(1).max(sp.start));
        let decl_line = byte_to_line(&line_starts, sp.name_start);
        let sig = lines.get(decl_line.saturating_sub(1)).map(|l| {
            let cleaned = l.trim().trim_end_matches('{').trim();
            truncate_chars(cleaned, 120)
        });
        items.push(OutlineItem {
            kind: sp.kind.to_string(),
            name: name.clone(),
            depth,
            start_line: Some(sl),
            end_line: Some(el),
            signature: sig,
            detail: None,
        });
    }

    Some(FileOutline {
        path: String::new(),
        file_type: FileType::Code,
        total_lines: Some(lines.len()),
        items,
    })
}

fn extract_markdown_outline(bytes: &[u8]) -> FileOutline {
    let text = String::from_utf8_lossy(bytes);
    let lines: Vec<&str> = text.lines().collect();
    let mut items = Vec::new();
    let mut in_fence = false;

    for (idx, &raw) in lines.iter().enumerate() {
        let trimmed = raw.trim_start();
        if trimmed.starts_with("```") || trimmed.starts_with("~~~") {
            in_fence = !in_fence;
            continue;
        }
        if in_fence {
            continue;
        }
        let hashes = trimmed.bytes().take_while(|&b| b == b'#').count();
        if (1..=6).contains(&hashes) {
            let rest = &trimmed[hashes..];
            if rest.starts_with(' ') || rest.starts_with('\t') {
                let title = rest.trim().trim_end_matches('#').trim();
                if !title.is_empty() {
                    items.push(OutlineItem {
                        kind: format!("h{hashes}"),
                        name: truncate_chars(title, 120),
                        depth: hashes - 1,
                        start_line: Some(idx + 1),
                        end_line: None,
                        signature: None,
                        detail: None,
                    });
                }
            }
        }
    }

    FileOutline {
        path: String::new(),
        file_type: FileType::Document,
        total_lines: Some(lines.len()),
        items,
    }
}

fn extract_single_file(path: &Path, rel: String, ocr: bool) -> Option<FileOutline> {
    if let Some(meta_items) = adapters::inspect_container(path) {
        let ft = FileType::from_path(path);
        let items = meta_items
            .into_iter()
            .map(|m| OutlineItem {
                kind: m.kind.to_string(),
                name: m.name,
                depth: 0,
                start_line: None,
                end_line: None,
                signature: None,
                detail: m.detail,
            })
            .collect();
        return Some(FileOutline {
            path: rel,
            file_type: ft,
            total_lines: None,
            items,
        });
    }

    let data = content::load_searchable(path, ocr).ok()??;
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();

    if matches!(ext.as_str(), "md" | "markdown" | "mdx") {
        let mut fo = extract_markdown_outline(&data);
        fo.path = rel;
        return Some(fo);
    }

    if let Some(mut fo) = extract_code_outline(path, &data) {
        fo.path = rel;
        return Some(fo);
    }

    // Adapted documents (PDF, PPTX, DOCX, HWP, HWPX, EPUB) or plain text with headings/markers.
    let text = String::from_utf8_lossy(&data);
    let lines: Vec<&str> = text.lines().collect();
    let mut items = Vec::new();
    for (idx, &ln) in lines.iter().enumerate() {
        let t = ln.trim();
        if (t.starts_with("[Page ") || t.starts_with("[Slide ")) && t.ends_with(']') {
            let preview = lines[(idx + 1)..lines.len().min(idx + 5)]
                .iter()
                .map(|s| s.trim())
                .find(|s| !s.is_empty())
                .map(|s| truncate_chars(s, 90));
            items.push(OutlineItem {
                kind: if t.starts_with("[Page") {
                    "page".into()
                } else {
                    "slide".into()
                },
                name: t.trim_matches(['[', ']']).to_string(),
                depth: 0,
                start_line: Some(idx + 1),
                end_line: None,
                signature: None,
                detail: preview,
            });
        }
    }

    // For generic documents without [Page]/[Slide] markers, surface concise numbered/section headings.
    if items.is_empty() && FileType::from_path(path) == FileType::Document {
        for (idx, &ln) in lines.iter().enumerate() {
            let t = ln.trim();
            if t.is_empty() || t.chars().count() > 80 {
                continue;
            }
            let looks_heading = t.starts_with('#')
                || t.starts_with("제")
                || (t.chars().next().is_some_and(|c| c.is_ascii_digit())
                    && (t.contains(". ") || t.contains("장 ") || t.contains("절 ")));
            if looks_heading {
                items.push(OutlineItem {
                    kind: "section".into(),
                    name: truncate_chars(t, 90),
                    depth: 0,
                    start_line: Some(idx + 1),
                    end_line: None,
                    signature: None,
                    detail: None,
                });
                if items.len() >= 50 {
                    break;
                }
            }
        }
    }

    Some(FileOutline {
        path: rel,
        file_type: FileType::from_path(path),
        total_lines: Some(lines.len()),
        items,
    })
}

pub fn build_outline(opts: &OutlineOptions) -> OutlineResponse {
    let mut r = build_outline_raw(opts);
    for f in &mut r.files {
        f.path = crate::walk::display_path(&opts.path, &f.path);
    }
    r
}

fn build_outline_raw(opts: &OutlineOptions) -> OutlineResponse {
    let t0 = Instant::now();
    if !opts.path.exists() {
        return OutlineResponse::failure(
            Status::InvalidQuery,
            format!("path not found: `{}`", opts.path.display()),
            ms(t0),
        );
    }

    let targets: Vec<(PathBuf, String)> = if opts.path.is_file() {
        let rel = opts.path.to_string_lossy().into_owned();
        vec![(opts.path.clone(), rel)]
    } else {
        let filters = Filters {
            exts: opts
                .exts
                .iter()
                .map(|e| e.trim_start_matches('.').to_ascii_lowercase())
                .filter(|e| !e.is_empty())
                .collect(),
            ..Filters::default()
        };
        let ff = match FileFilter::new(&filters) {
            Ok(f) => f,
            Err(e) => return OutlineResponse::failure(Status::InvalidQuery, e, ms(t0)),
        };

        let mut b = WalkBuilder::new(&opts.path);
        let ig = !opts.walk.no_ignore;
        b.hidden(!opts.walk.hidden)
            .ignore(ig)
            .git_ignore(ig)
            .git_global(ig)
            .git_exclude(ig)
            .parents(ig)
            .max_depth(opts.max_depth)
            .filter_entry(|e| e.file_name() != crate::index::DIR_NAME);

        let mut list = Vec::new();
        for res in b.build() {
            let Ok(ent) = res else { continue };
            if !ent.file_type().is_some_and(|t| t.is_file()) {
                continue;
            }
            let rel = rel_path(&opts.path, ent.path());
            if ff.accepts(&rel) {
                list.push((ent.path().to_path_buf(), rel));
            }
        }
        list.sort_by(|a, b| a.1.cmp(&b.1));
        list
    };

    let mut files: Vec<FileOutline> = targets
        .par_iter()
        .filter_map(|(p, rel)| extract_single_file(p, rel.clone(), opts.ocr))
        .filter(|fo| opts.path.is_file() || !fo.items.is_empty())
        .collect();

    let mut truncated = false;
    if let Some(lim) = opts.limit {
        if files.len() > lim {
            files.truncate(lim);
            truncated = true;
        }
    }

    let total_items: usize = files.iter().map(|f| f.items.len()).sum();
    OutlineResponse {
        status: if files.is_empty()
            || (opts.path.is_file() && total_items == 0 && files[0].total_lines == Some(0))
        {
            Status::Empty
        } else {
            Status::Complete
        },
        files,
        total_items,
        truncated,
        error: None,
        elapsed_ms: ms(t0),
    }
}

pub fn print_outline_human(resp: &OutlineResponse, w: &mut impl Write) -> io::Result<()> {
    if let Some(err) = &resp.error {
        eprintln!("error: {}", err.message);
        return Ok(());
    }
    for (fi, f) in resp.files.iter().enumerate() {
        if fi > 0 {
            writeln!(w)?;
        }
        let lines_note = f
            .total_lines
            .map(|n| format!(" ({n} lines)"))
            .unwrap_or_default();
        writeln!(w, "{} [{}]{}", f.path, f.file_type.name(), lines_note)?;
        if f.items.is_empty() {
            writeln!(w, "  (no structural outline items detected)")?;
            continue;
        }
        for it in &f.items {
            let indent = "  ".repeat(it.depth + 1);
            let loc = match (it.start_line, it.end_line) {
                (Some(s), Some(e)) if e > s => format!(" [L{s}-L{e}]"),
                (Some(s), _) => format!(" [L{s}]"),
                _ => String::new(),
            };
            if let Some(sig) = &it.signature {
                writeln!(w, "{indent}{:<9} {sig}{loc}", it.kind)?;
            } else if let Some(det) = &it.detail {
                writeln!(w, "{indent}{:<9} {}{loc} — {det}", it.kind, it.name)?;
            } else {
                writeln!(w, "{indent}{:<9} {}{loc}", it.kind, it.name)?;
            }
        }
    }
    Ok(())
}

fn ms(t0: Instant) -> f64 {
    t0.elapsed().as_secs_f64() * 1000.0
}
