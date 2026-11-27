use clap::{ArgAction, ArgGroup, Parser};
use lancet::ast_grep::{print_ast_human, search_ast, AstOptions};
use lancet::edit_cmd::{edit_file, EditOptions, Op, Target};
use lancet::fd::{find_files, parse_duration, parse_size, print_fd_human, FdOptions, PatternMode};
use lancet::index::{self, Index};
use lancet::outline::{build_outline, print_outline_human, OutlineOptions};
use lancet::output::{print_human, Status};
use lancet::query::{parse, FileType};
use lancet::read_cmd::{
    parse_col_spec, parse_line_spec, print_read_human, read_file_range, ReadOptions,
};
use lancet::refs::{find_refs, print_refs_human, RefsOptions};
use lancet::search::{explain, search, search_batch, IndexMode, SearchOptions};
use lancet::walk::WalkOpts;
use std::collections::BTreeMap;
use std::ffi::OsString;
use std::io::{self, IsTerminal, Write};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

#[cfg(all(target_env = "musl", target_pointer_width = "64"))]
#[global_allocator]
static ALLOC: mimalloc::MiMalloc = mimalloc::MiMalloc;

const ABOUT: &str =
    "Lancet: surgical code, AST, and multi-format document search, inspection, and refactoring CLI.

Examples:
  lancet fd --tree --dirs -d 2 .
  lancet rg -w spin_lock src
  lancet read src/search.rs --symbol execute_one
  lancet read report.pdf --page 2
  lancet edit src/net.rs --old-string 'retries = 3' --new-string 'retries = 5' --apply
  lancet refs --rank '*Service' --kind class .
  lancet ast '$X.unwrap()' src --lang rs --rewrite '$X?'
  lancet 'near(\"retry\", \"timeout\", 20) in function' src     (query language)

Commands:
  lancet fd [PATTERN] [PATH...]      find files and folders (`--tree`, globs like '**/*.ts')
  lancet rg [FLAGS] PATTERN [PATH...]   ripgrep: same flags, output and exit codes
  lancet read TARGET...              lines, symbols, pages, sheets, archive entries (`--offset/--limit`)
  lancet edit PATH                   replace / delete / insert by lines, symbol or exact text (`--old-string`)
  lancet write PATH                  create a file, or replace one you have read (`--rev`)
  lancet patch [DIFF|-]              apply a unified diff or a Codex patch, all files or none
  lancet refs SYMBOL [PATH]          definitions and references (`--rank` for the most used)
  lancet outline [PATH]              declarations, headings, sheets, archive contents
  lancet ast PATTERN [PATH]          structural search and rewrite (24 languages)
  lancet rename FILE:LINE OLD NEW    rename a local variable and its uses
  lancet index [PATH]                build or update the index (optional; results are always fresh)
  lancet mcp                         MCP server with one `lancet` tool taking these arguments

Advanced: lancet status, parse, explain, batch (see `lancet <command> --help`)";

#[derive(Parser)]
#[command(name = "lancet", version, about = ABOUT)]
struct SearchCli {
    /// Query (see examples above)
    query: String,
    /// Directory or file to search
    #[arg(default_value = ".")]
    path: PathBuf,
    /// Print machine-readable JSON
    #[arg(long)]
    json: bool,
    /// Ignore the index and scan files directly
    #[arg(long, hide = true)]
    no_index: bool,
    /// Use the index without checking for changed files (fastest, may be stale)
    #[arg(long, conflicts_with = "no_index", hide = true)]
    trust_index: bool,
    /// Index directory (default: PATH/.lancet)
    #[arg(long, hide = true)]
    index_dir: Option<PathBuf>,
    /// Search hidden files and directories
    #[arg(long)]
    hidden: bool,
    /// Don't respect .gitignore/.ignore files
    #[arg(long)]
    no_ignore: bool,
    /// Worker threads (default: number of CPUs)
    #[arg(short = 'j', long)]
    threads: Option<usize>,
    /// Max matching lines shown per result
    #[arg(long, default_value_t = 20)]
    max_lines: usize,
    /// Max characters per shown line
    #[arg(long, default_value_t = 240)]
    max_cols: usize,
    /// Print statistics to stderr
    #[arg(long, hide = true)]
    stats: bool,
    /// Enable external OCR (tesseract/pdftoppm) for images and scanned PDFs
    #[arg(long)]
    ocr: bool,
}

#[derive(Parser)]
#[command(
    name = "lancet index",
    about = "Build or incrementally update the trigram index"
)]
struct IndexCli {
    #[arg(default_value = ".")]
    path: PathBuf,
    #[arg(long)]
    index_dir: Option<PathBuf>,
    #[arg(long)]
    hidden: bool,
    #[arg(long)]
    no_ignore: bool,
    #[arg(short = 'j', long)]
    threads: Option<usize>,
    /// Discard existing segments and rebuild the index from scratch
    #[arg(long)]
    rebuild: bool,
    /// Force LSM segment merge & tombstone compaction
    #[arg(long, conflicts_with = "rebuild")]
    compact: bool,
    /// Enable external OCR (tesseract/pdftoppm) for images and scanned PDFs
    #[arg(long)]
    ocr: bool,
}

#[derive(Parser)]
#[command(name = "lancet status", about = "Show index status")]
struct StatusCli {
    #[arg(default_value = ".")]
    path: PathBuf,
    #[arg(long)]
    index_dir: Option<PathBuf>,
}

#[derive(Parser)]
#[command(
    name = "lancet parse",
    about = "Print the canonical query and IR as JSON"
)]
struct ParseCli {
    query: String,
}

#[derive(Parser)]
#[command(
    name = "lancet explain",
    about = "Explain query execution plan and index selectivity"
)]
struct ExplainCli {
    query: String,
    #[arg(default_value = ".")]
    path: PathBuf,
    #[arg(long, hide = true)]
    index_dir: Option<PathBuf>,
    #[arg(long, hide = true)]
    no_index: bool,
    #[arg(long, conflicts_with = "no_index", hide = true)]
    trust_index: bool,
    #[arg(long)]
    hidden: bool,
    #[arg(long)]
    no_ignore: bool,
}

#[derive(Parser)]
#[command(
    name = "lancet batch",
    about = "Run multiple queries sharing a single walk and index"
)]
struct BatchCli {
    #[arg(default_value = ".")]
    path: PathBuf,
    /// Query strings (repeatable)
    #[arg(short = 'q', long = "query")]
    queries: Vec<String>,
    /// Optional names corresponding to -q entries
    #[arg(long = "name")]
    names: Vec<String>,
    /// JSON object {"name": "query", ...} or array ["query1", ...]
    #[arg(long)]
    queries_json: Option<String>,
    #[arg(long, hide = true)]
    no_index: bool,
    #[arg(long, conflicts_with = "no_index", hide = true)]
    trust_index: bool,
    #[arg(long, hide = true)]
    index_dir: Option<PathBuf>,
    #[arg(long)]
    hidden: bool,
    #[arg(long)]
    no_ignore: bool,
    #[arg(short = 'j', long)]
    threads: Option<usize>,
    #[arg(long, default_value_t = 20)]
    max_lines: usize,
    #[arg(long, default_value_t = 240)]
    max_cols: usize,
    /// Enable external OCR (tesseract/pdftoppm) for images and scanned PDFs
    #[arg(long)]
    ocr: bool,
}

#[derive(Parser)]
#[command(
    name = "lancet fd",
    about = "Fast .gitignore-aware file & folder explorer"
)]
struct FdCli {
    /// Optional [PATTERN] and/or [PATH]
    args: Vec<String>,
    /// Explicit search pattern (useful when PATTERN looks like a directory)
    #[arg(short = 'p', long = "pattern")]
    pattern: Option<String>,
    /// Filter by entry kind: f|file, d|dir
    #[arg(short = 't', long = "type")]
    kinds: Vec<String>,
    /// Filter by file extension (comma-separated or repeatable)
    #[arg(short = 'e', long = "ext", value_delimiter = ',')]
    exts: Vec<String>,
    /// Filter by Lancet file type class (code, document, spreadsheet, config, log, data, archive, image, other)
    #[arg(long = "type-class", value_delimiter = ',')]
    type_classes: Vec<String>,
    /// Interpret PATTERN as a glob (e.g. '*.rs')
    #[arg(short = 'g', long)]
    glob: bool,
    /// Interpret PATTERN as a regex
    #[arg(short = 'r', long, conflicts_with = "glob")]
    regex: bool,
    /// Case-sensitive pattern matching
    #[arg(short = 's', long)]
    case_sensitive: bool,
    /// Maximum directory traversal depth
    #[arg(short = 'd', long)]
    max_depth: Option<usize>,
    /// Only entries modified within duration (e.g. 30m, 2h, 7d)
    #[arg(long)]
    changed_within: Option<String>,
    /// Minimum file size (e.g. 1k, 5m)
    #[arg(long)]
    min_size: Option<String>,
    /// Maximum file size (e.g. 10m, 1g)
    #[arg(long)]
    max_size: Option<String>,
    /// Render matched entries as an indented ASCII directory tree (add --dirs for folders only)
    #[arg(long)]
    tree: bool,
    /// Only directories (same as `-t d`): a project's layout without its files
    #[arg(long)]
    dirs: bool,
    /// Maximum number of entries returned
    #[arg(long)]
    limit: Option<usize>,
    /// Ignore the index and walk the filesystem directly
    #[arg(long, hide = true)]
    no_index: bool,
    /// Read file entries directly from the mmap'd Lancet index without walking disk
    #[arg(long, conflicts_with = "no_index", hide = true)]
    trust_index: bool,
    /// Custom index directory (default: PATH/.lancet)
    #[arg(long, hide = true)]
    index_dir: Option<PathBuf>,
    /// Include hidden files and directories
    #[arg(long)]
    hidden: bool,
    /// Don't respect .gitignore/.ignore files
    #[arg(long)]
    no_ignore: bool,
    /// Worker threads (default: number of CPUs)
    #[arg(short = 'j', long)]
    threads: Option<usize>,
    /// Include size and mtime_ns in JSON entries (costs one stat per entry)
    #[arg(long)]
    meta: bool,
    /// Print machine-readable JSON
    #[arg(long)]
    json: bool,
    /// Print statistics to stderr
    #[arg(long, hide = true)]
    stats: bool,
}

#[derive(Parser)]
#[command(
    name = "lancet ast",
    about = "Tree-sitter structural AST pattern search & safe rewrite"
)]
struct AstCli {
    /// AST pattern with metavariables ($VAR, $_, $$$ARGS)
    pattern: String,
    /// Directory or file to search
    #[arg(default_value = ".")]
    path: PathBuf,
    /// Source language: rs, py, js, ts, tsx, go, c, cpp, zig, java, cs, kt, scala, swift, rb, php, sh, lua, hcl, html, css, json, yaml, toml
    #[arg(short = 'l', long)]
    lang: Option<String>,
    /// Rewrite matched AST nodes using captured metavariables (dry-run unified diff by default)
    #[arg(short = 'r', long)]
    rewrite: Option<String>,
    /// Write --rewrite changes to disk atomically
    #[arg(long)]
    apply: bool,
    /// Restrict search to files matching path glob (repeatable)
    #[arg(long = "path")]
    paths: Vec<String>,
    /// Maximum number of AST matches returned
    #[arg(long)]
    limit: Option<usize>,
    /// Ignore the Lancet trigram index and scan all matching language files
    #[arg(long, hide = true)]
    no_index: bool,
    /// Use the index without checking mtime/size on disk (fastest)
    #[arg(long, conflicts_with = "no_index", hide = true)]
    trust_index: bool,
    /// Custom index directory (default: PATH/.lancet or ancestor)
    #[arg(long, hide = true)]
    index_dir: Option<PathBuf>,
    /// Search hidden files and directories
    #[arg(long)]
    hidden: bool,
    /// Don't respect .gitignore/.ignore files
    #[arg(long)]
    no_ignore: bool,
    /// Worker threads (default: number of CPUs)
    #[arg(short = 'j', long)]
    threads: Option<usize>,
    /// Print machine-readable JSON
    #[arg(long)]
    json: bool,
    /// Print execution statistics to stderr
    #[arg(long, hide = true)]
    stats: bool,
}

#[derive(Parser)]
#[command(
    name = "lancet read",
    about = "Read specific lines, AST symbols, PDF/PPTX pages, spreadsheet cells, or archive entries"
)]
struct ReadCli {
    /// File path (supports `file:10-50`, `file:42`, or `archive.zip!inner.log`).
    /// Several targets are read in one call (`a.rs:10-40 b.rs:5-20`); flags apply to each
    #[arg(required = true, num_args = 1..)]
    target: Vec<String>,
    /// 1-based line range (`10:50`, `42`, `100:`)
    #[arg(short = 'l', long, conflicts_with_all = ["offset", "limit"])]
    lines: Option<String>,
    /// First line to read (1-based), as other agents' read tools take it; with --limit
    #[arg(long, value_name = "LINE")]
    offset: Option<usize>,
    /// Number of lines to read (from --offset, or from line 1)
    #[arg(long, value_name = "N")]
    limit: Option<usize>,
    /// Extract exact AST symbol declaration (`fn`, `struct`, `class`, `method`, etc.)
    #[arg(short = 's', long)]
    symbol: Option<String>,
    /// Extract PDF `[Page N]` or PPTX `[Slide N]` range (`2` or `2:5`)
    #[arg(short = 'p', long = "page")]
    page: Option<String>,
    /// Spreadsheet sheet name or 1-based index
    #[arg(long)]
    sheet: Option<String>,
    /// Spreadsheet 1-based row range (`1:25`)
    #[arg(long)]
    rows: Option<String>,
    /// Spreadsheet column range (`A:D` or `1:4`)
    #[arg(long)]
    cols: Option<String>,
    /// Inner entry name inside `.zip` / `.tar` / `.tar.gz`
    #[arg(short = 'e', long)]
    entry: Option<String>,
    /// Truncate output cleanly at estimated token budget
    #[arg(long)]
    budget_tokens: Option<usize>,
    /// Omit line-number gutter and section header
    #[arg(long)]
    raw: bool,
    /// Enable external OCR (tesseract/pdftoppm) for scanned PDFs and images
    #[arg(long)]
    ocr: bool,
    /// Print machine-readable JSON
    #[arg(long)]
    json: bool,
}

#[derive(Parser)]
#[command(
    name = "lancet outline",
    about = "Extract code symbol skeleton, Markdown headings, spreadsheet sheets, or archive entries"
)]
struct OutlineCli {
    /// Target file or directory
    #[arg(default_value = ".")]
    path: PathBuf,
    /// Maximum directory traversal depth when PATH is a directory
    #[arg(short = 'd', long)]
    max_depth: Option<usize>,
    /// Filter by file extension (comma-separated or repeatable)
    #[arg(short = 'e', long = "ext", value_delimiter = ',')]
    exts: Vec<String>,
    /// Maximum number of files returned
    #[arg(long)]
    limit: Option<usize>,
    /// Include hidden files and directories
    #[arg(long)]
    hidden: bool,
    /// Don't respect .gitignore/.ignore files
    #[arg(long)]
    no_ignore: bool,
    /// Worker threads (default: number of CPUs)
    #[arg(short = 'j', long)]
    threads: Option<usize>,
    /// Enable external OCR for scanned PDFs/images
    #[arg(long)]
    ocr: bool,
    /// Print machine-readable JSON
    #[arg(long)]
    json: bool,
}

fn run_rank(opts: &RefsOptions, kinds: &[String], json: bool) -> ExitCode {
    let resp = lancet::refs::rank_symbols(opts, kinds);
    let stdout = io::stdout();
    let mut out = io::BufWriter::new(stdout.lock());
    if json {
        let _ = serde_json::to_writer(&mut out, &resp)
            .map_err(io::Error::other)
            .and_then(|_| writeln!(out));
    } else {
        let w = resp
            .entries
            .iter()
            .map(|e| e.references.to_string().len())
            .max()
            .unwrap_or(1);
        for e in &resp.entries {
            let shared = if e.definitions > 1 {
                format!("  (name shared by {} declarations)", e.definitions)
            } else {
                String::new()
            };
            let _ = writeln!(
                out,
                "{:>w$}  {:<9} {}  {}:{}{shared}",
                e.references, e.kind, e.name, e.path, e.line
            );
        }
        if resp.truncated {
            let _ = writeln!(
                out,
                "… {} more (raise --limit)",
                resp.total - resp.entries.len()
            );
        }
    }
    let _ = out.flush();
    if let Some(err) = &resp.error {
        eprintln!("error: {}", err.message);
    }
    for w in &resp.warnings {
        eprintln!("[lancet refs] warning: {w}");
    }
    match resp.status {
        Status::Complete => ExitCode::SUCCESS,
        Status::Empty => ExitCode::from(1),
        _ => ExitCode::from(2),
    }
}

#[derive(Parser)]
#[command(
    name = "lancet refs",
    about = "Find symbol definitions and lexical AST references (excluding comments & strings)",
    after_help = "Rank declarations by how often they are used:\n  lancet refs --rank '*Service' --kind class src     # the most used *Service classes\n  lancet refs --rank '*' --kind function --limit 10 ."
)]
struct RefsCli {
    /// Exact identifier / symbol name (with --rank: a glob such as `*Service`)
    symbol: String,
    /// List the declarations whose name matches SYMBOL (a glob), most referenced first
    #[arg(long)]
    rank: bool,
    /// With --rank: only these declaration kinds, as the language names them
    /// (class, interface, struct, enum, trait, function, method; Go: type, function, method)
    #[arg(long = "kind", value_delimiter = ',', requires = "rank")]
    kinds: Vec<String>,
    /// Directory or file to search
    #[arg(default_value = ".")]
    path: PathBuf,
    /// Restrict to one AST language: rs, py, js, ts, tsx, go, c, cpp, zig, java, cs, kt, scala, swift, rb, php, sh, lua, hcl, html, css, json, yaml, toml
    #[arg(short = 'l', long)]
    lang: Option<String>,
    /// Filter by file extension (comma-separated or repeatable)
    #[arg(short = 'e', long = "ext", value_delimiter = ',')]
    exts: Vec<String>,
    /// Restrict search to files matching path glob (repeatable)
    #[arg(long = "path")]
    paths: Vec<String>,
    /// Search non-code files (docs, configs, logs) as well
    #[arg(long)]
    all_files: bool,
    /// Maximum number of references returned (definitions are always shown)
    #[arg(long)]
    limit: Option<usize>,
    /// Ignore the Lancet trigram index
    #[arg(long, hide = true)]
    no_index: bool,
    /// Use the index without checking mtime/size on disk (fastest)
    #[arg(long, conflicts_with = "no_index", hide = true)]
    trust_index: bool,
    /// Custom index directory (default: PATH/.lancet or ancestor)
    #[arg(long, hide = true)]
    index_dir: Option<PathBuf>,
    /// Search hidden files and directories
    #[arg(long)]
    hidden: bool,
    /// Don't respect .gitignore/.ignore files
    #[arg(long)]
    no_ignore: bool,
    /// Worker threads (default: number of CPUs)
    #[arg(short = 'j', long)]
    threads: Option<usize>,
    /// Print machine-readable JSON
    #[arg(long)]
    json: bool,
    /// Print execution statistics to stderr
    #[arg(long, hide = true)]
    stats: bool,
}

fn init_threads(n: Option<usize>) {
    if let Some(n) = n {
        let _ = rayon::ThreadPoolBuilder::new()
            .num_threads(n)
            .build_global();
    }
}

fn main() -> ExitCode {
    let args: Vec<OsString> = std::env::args_os().collect();
    match args.get(1).and_then(|a| a.to_str()) {
        Some("fd") => run_fd(FdCli::parse_from(&args[1..])),
        Some("ast") => run_ast(AstCli::parse_from(&args[1..])),
        Some("read") => run_read(ReadCli::parse_from(&args[1..])),
        Some("edit") => run_edit(EditCli::parse_from(&args[1..])),
        Some("outline") => run_outline(OutlineCli::parse_from(&args[1..])),
        Some("refs") => run_refs(RefsCli::parse_from(&args[1..])),
        Some("index") => run_index(IndexCli::parse_from(&args[1..])),
        Some("status") => run_status(StatusCli::parse_from(&args[1..])),
        Some("parse") => run_parse(ParseCli::parse_from(&args[1..])),
        Some("explain") => run_explain(ExplainCli::parse_from(&args[1..])),
        Some("batch") => run_batch(BatchCli::parse_from(&args[1..])),
        Some("rg") => run_rg(RgCli::parse_from(&args[1..])),
        Some("patch") => run_patch(PatchCli::parse_from(&args[1..])),
        Some("rename") => run_rename(RenameCli::parse_from(&args[1..])),
        Some("write") => run_write(WriteCli::parse_from(&args[1..])),
        Some("mcp") => run_mcp(McpCli::parse_from(&args[1..])),
        _ => run_search(SearchCli::parse_from(&args)),
    }
}

fn index_mode(no_index: bool, trust_index: bool) -> IndexMode {
    if no_index {
        IndexMode::Off
    } else if trust_index {
        IndexMode::Trust
    } else {
        IndexMode::Auto
    }
}

fn run_search(cli: SearchCli) -> ExitCode {
    init_threads(cli.threads);
    let mut opts = SearchOptions::new(&cli.path);
    opts.index_dir = cli.index_dir;
    opts.walk = WalkOpts {
        hidden: cli.hidden,
        no_ignore: cli.no_ignore,
        threads: cli.threads.unwrap_or(0),
        ..WalkOpts::default()
    };
    opts.index = index_mode(cli.no_index, cli.trust_index);
    opts.ocr = cli.ocr;
    opts.max_lines = cli.max_lines;
    opts.max_cols = cli.max_cols;
    let r = search(&cli.query, &opts);
    let stdout = io::stdout();
    let mut out = io::BufWriter::new(stdout.lock());
    let _ = if cli.json {
        serde_json::to_writer(&mut out, &r)
            .map_err(io::Error::other)
            .and_then(|_| writeln!(out))
    } else {
        print_human(&r, &mut out)
    };
    let _ = out.flush();
    if !cli.json && r.truncated {
        if let Some(b) = &r.budget {
            eprintln!(
                "[lancet] showing {} of {}{} results (~{}/{} tokens)",
                r.returned,
                r.matched,
                if r.matched_is_lower_bound { "+" } else { "" },
                b.used_tokens,
                b.limit_tokens
            );
        } else {
            eprintln!(
                "[lancet] showing {} of {}{} results",
                r.returned,
                r.matched,
                if r.matched_is_lower_bound { "+" } else { "" }
            );
        }
    }
    for w in &r.warnings {
        eprintln!("[lancet] warning: {w}");
    }
    if cli.stats {
        eprintln!(
            "[lancet] mode={} query={} {}",
            r.mode,
            r.query.as_deref().unwrap_or("-"),
            serde_json::to_string(&r.stats).unwrap_or_default()
        );
    }
    match r.status {
        Status::Complete => ExitCode::SUCCESS,
        Status::Empty => ExitCode::from(1),
        _ => ExitCode::from(2),
    }
}

fn run_index(cli: IndexCli) -> ExitCode {
    init_threads(cli.threads);
    let dir = cli
        .index_dir
        .clone()
        .unwrap_or_else(|| index::default_dir(&cli.path));
    let wopts = WalkOpts {
        hidden: cli.hidden,
        no_ignore: cli.no_ignore,
        threads: cli.threads.unwrap_or(0),
        ..WalkOpts::default()
    };
    let res = if cli.rebuild {
        index::rebuild_with_ocr(&cli.path, &dir, &wopts, cli.ocr)
    } else if cli.compact {
        match index::sync_with_ocr(&cli.path, &dir, &wopts, cli.ocr) {
            Ok(mut rep) => match index::compact(&dir) {
                Ok(n) => {
                    rep.compacted += n;
                    if let Ok(Some(ix)) = Index::open(&dir) {
                        rep.segments = ix.segs.len();
                    }
                    Ok(rep)
                }
                Err(e) => Err(e),
            },
            Err(e) => Err(e),
        }
    } else {
        index::sync_with_ocr(&cli.path, &dir, &wopts, cli.ocr)
    };
    match res {
        Ok(rep) => {
            println!("{}", serde_json::to_string(&rep).unwrap());
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("error: {e:#}");
            ExitCode::from(2)
        }
    }
}

fn run_status(cli: StatusCli) -> ExitCode {
    let dir = cli
        .index_dir
        .unwrap_or_else(|| index::default_dir(&cli.path));
    match Index::open(&dir) {
        Ok(Some(ix)) => {
            let bytes: usize = ix.segs.iter().map(|s| s.disk_bytes()).sum();
            println!(
                "{}",
                serde_json::json!({
                    "index_dir": dir,
                    "segments": ix.segs.len(),
                    "live_docs": ix.live_count(),
                    "dead_docs": ix.dead_count(),
                    "disk_bytes": bytes,
                })
            );
            ExitCode::SUCCESS
        }
        Ok(None) => {
            println!(
                "{}",
                serde_json::json!({ "index_dir": dir, "exists": false })
            );
            ExitCode::from(1)
        }
        Err(e) => {
            eprintln!("error: {e:#}");
            ExitCode::from(2)
        }
    }
}

fn run_parse(cli: ParseCli) -> ExitCode {
    match parse(&cli.query) {
        Ok(q) => {
            println!(
                "{}",
                serde_json::json!({ "canonical": q.to_string(), "ir": q })
            );
            ExitCode::SUCCESS
        }
        Err(e) => {
            println!("{}", serde_json::json!({ "error": e }));
            ExitCode::from(2)
        }
    }
}

fn run_explain(cli: ExplainCli) -> ExitCode {
    let mut opts = SearchOptions::new(&cli.path);
    opts.index_dir = cli.index_dir;
    opts.walk = WalkOpts {
        hidden: cli.hidden,
        no_ignore: cli.no_ignore,
        threads: 0,
        ..WalkOpts::default()
    };
    opts.index = index_mode(cli.no_index, cli.trust_index);
    let rep = explain(&cli.query, &opts);
    println!("{}", serde_json::to_string_pretty(&rep).unwrap());
    match rep.status {
        Status::Complete => ExitCode::SUCCESS,
        _ => ExitCode::from(2),
    }
}

fn run_batch(cli: BatchCli) -> ExitCode {
    init_threads(cli.threads);
    let mut pairs: Vec<(String, String)> = Vec::new();
    if let Some(raw) = &cli.queries_json {
        if let Ok(map) = serde_json::from_str::<BTreeMap<String, String>>(raw) {
            pairs.extend(map);
        } else if let Ok(arr) = serde_json::from_str::<Vec<String>>(raw) {
            for (i, q) in arr.into_iter().enumerate() {
                pairs.push((format!("q{}", i + 1), q));
            }
        } else {
            eprintln!("error: --queries-json must be a JSON object {{\"name\": \"query\"}} or array [\"query\"]");
            return ExitCode::from(2);
        }
    }
    for (i, q) in cli.queries.into_iter().enumerate() {
        let name = cli
            .names
            .get(i)
            .cloned()
            .unwrap_or_else(|| format!("q{}", pairs.len() + 1));
        pairs.push((name, q));
    }
    if pairs.is_empty() {
        eprintln!("error: provide at least one -q <QUERY> or --queries-json");
        return ExitCode::from(2);
    }

    let mut opts = SearchOptions::new(&cli.path);
    opts.index_dir = cli.index_dir;
    opts.walk = WalkOpts {
        hidden: cli.hidden,
        no_ignore: cli.no_ignore,
        threads: cli.threads.unwrap_or(0),
        ..WalkOpts::default()
    };
    opts.index = index_mode(cli.no_index, cli.trust_index);
    opts.ocr = cli.ocr;
    opts.max_lines = cli.max_lines;
    opts.max_cols = cli.max_cols;

    let rep = search_batch(&pairs, &opts);
    println!("{}", serde_json::to_string(&rep).unwrap());
    if rep.results.iter().any(|r| {
        matches!(
            r.response.status,
            Status::InvalidQuery | Status::Unsupported
        )
    }) {
        ExitCode::from(2)
    } else {
        ExitCode::SUCCESS
    }
}

/// `[PATTERN] [PATH...]`, like fd. A first argument that is an existing directory
/// (or, alone, looks like a path: has a `/` and no glob characters) is a path.
fn resolve_fd_args(
    args: &[String],
    explicit_pat: Option<String>,
) -> Result<(Option<String>, Vec<PathBuf>), String> {
    let paths = |xs: &[String]| -> Vec<PathBuf> {
        if xs.is_empty() {
            vec![PathBuf::from(".")]
        } else {
            xs.iter().map(PathBuf::from).collect()
        }
    };
    if let Some(pat) = explicit_pat {
        return Ok((Some(pat), paths(args)));
    }
    match args {
        [] => Ok((None, paths(&[]))),
        // `fd . PATH…`: the usual "everything under PATH" (not names containing a dot).
        [dot, rest @ ..] if dot == "." && !rest.is_empty() => Ok((None, paths(rest))),
        [first, ..] if Path::new(first).is_dir() => Ok((None, paths(args))),
        [single] if single.contains('/') && !single.contains(['*', '?', '[']) => {
            // `docs/imgaes`: a mistyped directory, reported as missing rather
            // than searched for as a name that can never match.
            Ok((None, paths(args)))
        }
        [pat, rest @ ..] => Ok((Some(pat.clone()), paths(rest))),
    }
}

fn run_fd(cli: FdCli) -> ExitCode {
    init_threads(cli.threads);
    let (pattern, roots) = match resolve_fd_args(&cli.args, cli.pattern) {
        Ok(x) => x,
        Err(msg) => {
            if cli.json {
                println!(
                    "{}",
                    serde_json::json!({ "status": "invalid_query", "error": { "message": msg } })
                );
            } else {
                eprintln!("error: {msg}");
            }
            return ExitCode::from(2);
        }
    };

    let mut include_files = false;
    let mut include_dirs = cli.dirs;
    for k in &cli.kinds {
        match k.to_ascii_lowercase().as_str() {
            "f" | "file" => include_files = true,
            "d" | "dir" | "directory" => include_dirs = true,
            other => {
                let msg = format!("unknown --type `{other}` (expected `f`/`file` or `d`/`dir`)");
                if cli.json {
                    println!(
                        "{}",
                        serde_json::json!({ "status": "invalid_query", "error": { "message": msg } })
                    );
                } else {
                    eprintln!("error: {msg}");
                }
                return ExitCode::from(2);
            }
        }
    }

    let mut file_types = Vec::new();
    for tc in &cli.type_classes {
        match FileType::parse(tc) {
            Some(ft) => file_types.push(ft),
            None => {
                let msg = format!("unknown --type-class `{tc}`");
                if cli.json {
                    println!(
                        "{}",
                        serde_json::json!({ "status": "invalid_query", "error": { "message": msg } })
                    );
                } else {
                    eprintln!("error: {msg}");
                }
                return ExitCode::from(2);
            }
        }
    }

    let changed_within_ns = match cli
        .changed_within
        .as_deref()
        .map(parse_duration)
        .transpose()
    {
        Ok(v) => v,
        Err(msg) => {
            if cli.json {
                println!(
                    "{}",
                    serde_json::json!({ "status": "invalid_query", "error": { "message": msg } })
                );
            } else {
                eprintln!("error: {msg}");
            }
            return ExitCode::from(2);
        }
    };
    let min_size = match cli.min_size.as_deref().map(parse_size).transpose() {
        Ok(v) => v,
        Err(msg) => {
            if cli.json {
                println!(
                    "{}",
                    serde_json::json!({ "status": "invalid_query", "error": { "message": msg } })
                );
            } else {
                eprintln!("error: {msg}");
            }
            return ExitCode::from(2);
        }
    };
    let max_size = match cli.max_size.as_deref().map(parse_size).transpose() {
        Ok(v) => v,
        Err(msg) => {
            if cli.json {
                println!(
                    "{}",
                    serde_json::json!({ "status": "invalid_query", "error": { "message": msg } })
                );
            } else {
                eprintln!("error: {msg}");
            }
            return ExitCode::from(2);
        }
    };

    let mut opts = FdOptions::new(&roots[0]);
    opts.pattern = pattern;
    opts.pattern_mode = if cli.regex {
        PatternMode::Regex
    } else if cli.glob {
        PatternMode::Glob
    } else if opts
        .pattern
        .as_deref()
        .is_some_and(|p| p.contains(['*', '?', '[']))
    {
        // `*.md` as a substring matches nothing; nobody means that.
        if !cli.json {
            eprintln!("[lancet fd] pattern looks like a glob; matching it as one (-g)");
        }
        PatternMode::Glob
    } else {
        PatternMode::Substring
    };
    opts.case_sensitive = cli.case_sensitive;
    opts.include_files = include_files;
    opts.include_dirs = include_dirs;
    opts.exts = cli.exts;
    opts.file_types = file_types;
    opts.max_depth = cli.max_depth;
    opts.changed_within_ns = changed_within_ns;
    opts.min_size = min_size;
    opts.max_size = max_size;
    opts.limit = cli.limit;
    opts.tree = cli.tree;
    opts.walk = WalkOpts {
        hidden: cli.hidden,
        no_ignore: cli.no_ignore,
        threads: cli.threads.unwrap_or(0),
        ..WalkOpts::default()
    };
    opts.index = index_mode(cli.no_index, cli.trust_index);
    opts.index_dir = cli.index_dir;
    opts.need_meta = cli.json && cli.meta;

    let resps: Vec<(PathBuf, lancet::fd::FdResponse)> = roots
        .iter()
        .map(|root| {
            let mut o = opts.clone();
            o.root = root.clone();
            (root.clone(), find_files(&o))
        })
        .collect();
    let stdout = io::stdout();
    let mut out = io::BufWriter::new(stdout.lock());
    if !cli.json {
        for (root, r) in &resps {
            if let Some(err) = &r.error {
                eprintln!("error: {}", err.message);
            } else {
                let _ = print_fd_human(r, root, cli.tree, &mut out);
            }
        }
    }
    let resp = lancet::fd::FdResponse::merge(resps.into_iter().map(|(_, r)| r).collect());
    if cli.json {
        let _ = serde_json::to_writer(&mut out, &resp)
            .map_err(io::Error::other)
            .and_then(|_| writeln!(out));
    }
    let _ = out.flush();

    if !cli.json && resp.truncated {
        eprintln!(
            "[lancet fd] showing {} of {} entries",
            resp.returned, resp.total
        );
    }
    for w in &resp.warnings {
        eprintln!("[lancet fd] warning: {w}");
    }
    if cli.stats {
        eprintln!(
            "[lancet fd] mode={} total={} returned={} elapsed_ms={:.2}",
            resp.mode, resp.total, resp.returned, resp.elapsed_ms
        );
    }
    match resp.status {
        Status::Complete => ExitCode::SUCCESS,
        Status::Empty => ExitCode::from(1),
        _ => ExitCode::from(2),
    }
}

fn cli_invalid(json: bool, msg: &str) -> ExitCode {
    if json {
        println!(
            "{}",
            serde_json::json!({ "status": "invalid_query", "error": { "message": msg } })
        );
    } else {
        eprintln!("error: {msg}");
    }
    ExitCode::from(2)
}

#[derive(Parser)]
#[command(
    name = "lancet edit",
    about = "Replace, delete, or insert lines addressed like `lancet read`, or exact text with --find (dry-run diff unless --apply)",
    group(ArgGroup::new("target").required(true).args(["lines", "symbol", "find", "edits"])),
    group(ArgGroup::new("op").args(["with", "with_file", "delete", "insert_before", "insert_after"]).conflicts_with("edits"))
)]
struct EditCli {
    /// File to edit (plain-text files; documents and archives are read-only).
    /// Optional with --edits when every hunk names its own "path"
    path: Option<PathBuf>,
    /// 1-based line range, as in `lancet read` (`10:50`, `42`, `100:`)
    #[arg(short = 'l', long)]
    lines: Option<String>,
    /// The lines of an AST symbol, exactly as `lancet read --symbol` shows them
    /// (doc comments and attributes above it included: replace them too, or they go)
    #[arg(short = 's', long)]
    symbol: Option<String>,
    /// Exact TEXT to change (not whole lines; may span lines). Refused unless it
    /// occurs exactly once, or pass --replace-all to change every occurrence
    #[arg(
        short = 'f',
        long = "old-string",
        visible_alias = "find",
        allow_hyphen_values = true
    )]
    find: Option<String>,
    /// With --old-string: change every occurrence instead of requiring exactly one
    #[arg(long = "replace-all", visible_alias = "all", requires = "find")]
    all: bool,
    /// With --old-string: require exactly N occurrences and change all of them
    #[arg(long, value_name = "N", requires = "find")]
    expected_replacements: Option<usize>,
    /// Replace the addressed lines (or the --old-string text) with TEXT
    #[arg(
        long = "new-string",
        visible_alias = "with",
        allow_hyphen_values = true
    )]
    with: Option<String>,
    /// Replace the addressed lines with the contents of FILE (`-` reads stdin)
    #[arg(long)]
    with_file: Option<PathBuf>,
    /// Delete the addressed lines
    #[arg(long)]
    delete: bool,
    /// Insert TEXT before the addressed lines
    #[arg(long, allow_hyphen_values = true)]
    insert_before: Option<String>,
    /// Insert TEXT after the addressed lines
    #[arg(long, allow_hyphen_values = true)]
    insert_after: Option<String>,
    /// Several hunks in one atomic call, from a JSON file (`-` reads stdin). Hunks may
    /// name their file (`"path"`, with that file's `"rev"`) to change several files at once:
    /// `[{"lines": "10:12", "with": "…"}, {"symbol": "f", "delete": true}]`.
    /// All hunks use the original file's line numbers.
    #[arg(long)]
    edits: Option<PathBuf>,
    /// Refuse unless the file's content revision equals REV (from read/ast/edit JSON)
    #[arg(long)]
    rev: Option<String>,
    /// Write the change atomically (default: print a dry-run diff)
    #[arg(long)]
    apply: bool,
    /// Write even if the edit adds Tree-sitter syntax errors
    #[arg(long)]
    allow_syntax_errors: bool,
    /// Print machine-readable JSON
    #[arg(long)]
    json: bool,
}

#[derive(Parser)]
#[command(
    name = "lancet rg",
    args_override_self = true,
    about = "ripgrep-compatible search: same flags, semantics, output and exit codes, on lancet's index (always fresh) and document extractors"
)]
struct RgCli {
    /// PATTERN then PATH... (with -e/-f, every argument is a PATH)
    args: Vec<String>,
    /// A pattern to search for (repeatable)
    #[arg(short = 'e', long = "regexp", allow_hyphen_values = true)]
    regexp: Vec<String>,
    /// Read patterns from FILE, one per line (`-` for stdin)
    #[arg(short = 'f', long = "file")]
    pattern_file: Vec<PathBuf>,
    /// Treat patterns as literal strings
    #[arg(short = 'F', long)]
    fixed_strings: bool,
    /// Case-insensitive search
    #[arg(short = 'i', long, overrides_with_all = ["case_sensitive", "smart_case"])]
    ignore_case: bool,
    /// Case-sensitive search (default)
    #[arg(short = 's', long, overrides_with_all = ["ignore_case", "smart_case"])]
    case_sensitive: bool,
    /// Case-insensitive unless the pattern has an uppercase letter
    #[arg(short = 'S', long, overrides_with_all = ["ignore_case", "case_sensitive"])]
    smart_case: bool,
    /// Only matches surrounded by word boundaries
    #[arg(short = 'w', long)]
    word_regexp: bool,
    /// Only matches spanning the whole line
    #[arg(short = 'x', long)]
    line_regexp: bool,
    /// Print lines that do not match
    #[arg(short = 'v', long)]
    invert_match: bool,
    /// Allow matches to span lines
    #[arg(short = 'U', long)]
    multiline: bool,
    /// With -U, let `.` match newlines
    #[arg(long)]
    multiline_dotall: bool,
    /// Print only the matched parts
    #[arg(short = 'o', long)]
    only_matching: bool,
    /// Number of matching lines per file
    #[arg(short = 'c', long)]
    count: bool,
    /// Number of matches per file
    #[arg(long)]
    count_matches: bool,
    /// Only print paths with at least one match
    #[arg(short = 'l', long)]
    files_with_matches: bool,
    /// Only print paths without a match
    #[arg(long)]
    files_without_match: bool,
    /// No output; exit 0 on the first match
    #[arg(short = 'q', long)]
    quiet: bool,
    /// Stop after NUM matching lines per file
    #[arg(short = 'm', long)]
    max_count: Option<u64>,
    /// Lines of context after each match
    #[arg(short = 'A', long, overrides_with = "passthru")]
    after_context: Option<usize>,
    /// Lines of context before each match
    #[arg(short = 'B', long, overrides_with = "passthru")]
    before_context: Option<usize>,
    /// Lines of context before and after each match
    #[arg(short = 'C', long, overrides_with = "passthru")]
    context: Option<usize>,
    /// Print every line of every searched file, matches and non-matches alike
    /// (the last of --passthru / -A / -B / -C wins, as in ripgrep)
    #[arg(long, alias = "passthrough", overrides_with_all = ["after_context", "before_context", "context"])]
    passthru: bool,
    /// Show line numbers (default)
    #[arg(short = 'n', long, overrides_with = "no_line_number")]
    line_number: bool,
    /// Hide line numbers
    #[arg(short = 'N', long, overrides_with = "line_number")]
    no_line_number: bool,
    /// Always print the file path
    #[arg(short = 'H', long, overrides_with = "no_filename")]
    with_filename: bool,
    /// Never print the file path
    #[arg(short = 'I', long, overrides_with = "with_filename")]
    no_filename: bool,
    /// Show the column of the first match
    #[arg(long)]
    column: bool,
    /// path:line:column:line for every match
    #[arg(long)]
    vimgrep: bool,
    /// ripgrep's JSON Lines output (begin/match/context/end/summary)
    #[arg(long)]
    json: bool,
    /// NUL after file paths
    #[arg(short = '0', long)]
    null: bool,
    /// Replace matches in the output (`$1`, `${name}` supported)
    #[arg(short = 'r', long, allow_hyphen_values = true)]
    replace: Option<String>,
    /// Search binary files as text
    #[arg(short = 'a', long)]
    text: bool,
    /// Also search inside PDFs, Office files, HWP/HWPX, EPUB and archives
    /// (lancet extension; off by default so output matches ripgrep)
    #[arg(long)]
    docs: bool,
    /// Include/exclude paths by glob (`!` negates; repeatable)
    #[arg(short = 'g', long, allow_hyphen_values = true)]
    glob: Vec<String>,
    /// Like --glob, case-insensitive
    #[arg(long, allow_hyphen_values = true)]
    iglob: Vec<String>,
    /// Only search files of TYPE (ripgrep's type list)
    #[arg(short = 't', long = "type")]
    type_: Vec<String>,
    /// Skip files of TYPE
    #[arg(short = 'T', long)]
    type_not: Vec<String>,
    /// Add a file type: `NAME:GLOB` (repeatable; `NAME:include:A,B` merges types)
    #[arg(long, value_name = "TYPESPEC")]
    type_add: Vec<String>,
    /// Print the known file types
    #[arg(long)]
    type_list: bool,
    /// -u: no ignore files, -uu: also hidden, -uuu: also binary
    #[arg(short = 'u', long, action = ArgAction::Count)]
    unrestricted: u8,
    /// Search hidden files and directories
    #[arg(long, short = '.')]
    hidden: bool,
    /// Don't respect .gitignore/.ignore/.rgignore
    #[arg(long)]
    no_ignore: bool,
    /// Follow symbolic links while traversing directories
    #[arg(short = 'L', long)]
    follow: bool,
    /// Limit directory traversal depth (1 = direct children only)
    #[arg(long, value_name = "NUM")]
    max_depth: Option<usize>,
    /// Ignore files larger than NUM bytes (K, M, G suffixes supported)
    #[arg(long, value_name = "NUM+SUFFIX")]
    max_filesize: Option<String>,
    /// Print the files that would be searched
    #[arg(long)]
    files: bool,
    /// Worker threads
    #[arg(short = 'j', long)]
    threads: Option<usize>,
    /// Accepted for compatibility; output is always sorted by path
    #[arg(long, value_name = "SORTBY")]
    sort: Option<String>,
    /// Omit lines longer than NUM bytes ("[Omitted long matching line]")
    #[arg(short = 'M', long, value_name = "NUM")]
    max_columns: Option<usize>,
    /// With -M: print the first NUM bytes of a long line instead
    #[arg(long)]
    max_columns_preview: bool,
    /// Print the byte offset of each line (of each match with -o)
    #[arg(short = 'b', long)]
    byte_offset: bool,
    /// Remove leading whitespace from printed lines
    #[arg(long)]
    trim: bool,
    /// Print each file's name on its own line above its matches
    #[arg(long, overrides_with = "no_heading")]
    heading: bool,
    /// Same as --color always --heading --line-number
    #[arg(short = 'p', long)]
    pretty: bool,
    /// String printed between context groups (default `--`)
    #[arg(long, value_name = "SEP", allow_hyphen_values = true)]
    context_separator: Option<String>,
    /// Print nothing between context groups
    #[arg(long)]
    no_context_separator: bool,
    /// Separator after the path / line / column of a matching line (default `:`)
    #[arg(long, value_name = "SEP", allow_hyphen_values = true)]
    field_match_separator: Option<String>,
    /// Separator after the path / line / column of a context line (default `-`)
    #[arg(long, value_name = "SEP", allow_hyphen_values = true)]
    field_context_separator: Option<String>,
    /// Print SEP instead of `/` in paths
    #[arg(long, value_name = "SEP")]
    path_separator: Option<String>,
    /// With -c / --count-matches: also print files with no match
    #[arg(long)]
    include_zero: bool,
    /// Print totals after the results
    #[arg(long)]
    stats: bool,
    /// Sort results in reverse (path, modified, accessed, created)
    #[arg(long, value_name = "SORTBY")]
    sortr: Option<String>,
    /// Same as --sort path
    #[arg(long, hide = true)]
    sort_files: bool,
    /// Ignore files listed in PATH (gitignore format; repeatable)
    #[arg(long, value_name = "PATH")]
    ignore_file: Vec<PathBuf>,
    /// Don't respect .gitignore / .git/info/exclude
    #[arg(long)]
    no_ignore_vcs: bool,
    /// Don't respect .ignore / .rgignore
    #[arg(long)]
    no_ignore_dot: bool,
    /// Don't respect ignore files in parent directories
    #[arg(long)]
    no_ignore_parent: bool,
    /// Don't respect the global gitignore
    #[arg(long)]
    no_ignore_global: bool,
    /// Don't respect .git/info/exclude
    #[arg(long)]
    no_ignore_exclude: bool,
    /// Don't respect --ignore-file files
    #[arg(long)]
    no_ignore_files: bool,
    /// Respect .gitignore even outside a git repository
    #[arg(long)]
    no_require_git: bool,
    /// Match globs in ignore files case-insensitively
    #[arg(long)]
    ignore_file_case_insensitive: bool,
    /// Suppress messages about invalid ignore files (no effect)
    #[arg(long, hide = true)]
    no_ignore_messages: bool,
    /// Match --glob case-insensitively
    #[arg(long)]
    glob_case_insensitive: bool,
    /// Clear a file type's globs (before --type-add)
    #[arg(long, value_name = "TYPE")]
    type_clear: Vec<String>,
    /// Don't cross into other file systems
    #[arg(long)]
    one_file_system: bool,
    /// Stop a file's search at the first non-matching line after a match
    #[arg(long)]
    stop_on_nonmatch: bool,
    /// Don't print errors about files and paths (exit status still reports them)
    #[arg(long)]
    no_messages: bool,
    /// Text encoding: auto (BOM sniffing, default), none, or a WHATWG label (latin1, sjis, utf-16le, …)
    #[arg(short = 'E', long, value_name = "ENCODING")]
    encoding: Option<String>,
    /// Treat CRLF as a line terminator ($ matches before \r\n)
    #[arg(long)]
    crlf: bool,
    /// Use NUL as the line terminator
    #[arg(long)]
    null_data: bool,
    /// Search binary files in directories too (printing "binary file matches")
    #[arg(long)]
    binary: bool,
    /// Search inside .gz files
    #[arg(short = 'z', long)]
    search_zip: bool,
    /// Disable Unicode in the regex (\w, \d, case folding are ASCII-only)
    #[arg(long)]
    no_unicode: bool,
    /// Print lancet's version
    #[arg(short = 'V', long)]
    version: bool,
    // Accepted for compatibility; they change nothing lancet does.
    #[arg(long, hide = true)]
    mmap: bool,
    #[arg(long, hide = true)]
    no_mmap: bool,
    #[arg(long, hide = true)]
    line_buffered: bool,
    #[arg(long, hide = true)]
    block_buffered: bool,
    #[arg(long, hide = true)]
    no_config: bool,
    #[arg(long, hide = true)]
    no_pcre2_unicode: bool,
    #[arg(long, hide = true)]
    debug: bool,
    #[arg(long, hide = true)]
    trace: bool,
    #[arg(long, hide = true, value_name = "NUM")]
    dfa_size_limit: Option<String>,
    #[arg(long, hide = true, value_name = "NUM")]
    regex_size_limit: Option<String>,
    #[arg(long, hide = true, value_name = "PATH")]
    hostname_bin: Option<String>,
    #[arg(long, hide = true, value_name = "FORMAT")]
    hyperlink_format: Option<String>,
    #[arg(long, hide = true, value_name = "SPEC")]
    colors: Vec<String>,
    // Refused with a reason (see docs/rg-parity.md).
    #[arg(short = 'P', long, hide = true)]
    pcre2: bool,
    #[arg(long, hide = true)]
    pcre2_version: bool,
    #[arg(long, hide = true, value_name = "ENGINE")]
    engine: Option<String>,
    #[arg(long, hide = true)]
    auto_hybrid_regex: bool,
    #[arg(long, hide = true, value_name = "COMMAND")]
    pre: Option<String>,
    #[arg(long, hide = true, value_name = "GLOB")]
    pre_glob: Vec<String>,
    #[arg(long, hide = true, value_name = "KIND")]
    generate: Option<String>,
    /// Accepted for compatibility; output never uses headings
    #[arg(long)]
    no_heading: bool,
    /// Color output: auto (TTY only, default), always, ansi, or never
    #[arg(long, value_name = "WHEN")]
    color: Option<String>,
    /// Ignore the lancet index (search like plain ripgrep)
    #[arg(long, hide = true)]
    no_index: bool,
    /// Answer from the lancet index without checking for changed files
    #[arg(long, hide = true, conflicts_with = "no_index")]
    trust_index: bool,
    /// Custom index directory (default: PATH/.lancet or an ancestor)
    #[arg(long, hide = true)]
    index_dir: Option<PathBuf>,
}

fn run_rg(cli: RgCli) -> ExitCode {
    use lancet::rg::{list_files, parse_filesize, run, Case, RgOptions};
    let fail = |msg: &str| {
        eprintln!("lancet rg: {msg}");
        ExitCode::from(2)
    };
    if cli.version {
        println!("lancet {}", env!("CARGO_PKG_VERSION"));
        return ExitCode::SUCCESS;
    }
    if cli.pcre2_version {
        println!("PCRE2 is not available in this build of lancet.");
        return ExitCode::from(1);
    }
    let refused = if cli.pcre2 || cli.engine.as_deref() == Some("pcre2") {
        Some("-P/--pcre2: lancet uses Rust regex only, which keeps matching linear-time and lets the index derive a sound trigram filter")
    } else if cli
        .engine
        .as_deref()
        .is_some_and(|e| e != "default" && e != "auto")
    {
        Some("--engine: only the default (Rust regex) engine is available")
    } else if cli.pre.is_some() || !cli.pre_glob.is_empty() {
        Some("--pre/--pre-glob: lancet doesn't run a process per file; documents and archives are read in-process with --docs")
    } else if cli.generate.is_some() {
        Some("--generate: lancet has no man page or completion generator yet")
    } else {
        None
    };
    if let Some(why) = refused {
        return fail(&format!("not supported: {why} (see docs/rg-parity.md)"));
    }
    let _ = (
        cli.auto_hybrid_regex,
        cli.mmap,
        cli.no_mmap,
        cli.line_buffered,
        cli.block_buffered,
    );
    let _ = (
        cli.no_config,
        cli.no_pcre2_unicode,
        cli.debug,
        cli.trace,
        cli.no_ignore_messages,
    );
    let _ = (
        &cli.dfa_size_limit,
        &cli.regex_size_limit,
        &cli.hostname_bin,
        &cli.hyperlink_format,
        &cli.colors,
    );
    if let Some(label) = cli.encoding.as_deref() {
        if label != "auto"
            && label != "none"
            && encoding_rs::Encoding::for_label(label.as_bytes()).is_none()
        {
            return fail(&format!("unknown encoding '{label}'"));
        }
    }
    if cli.type_list {
        let b = match lancet::walk::types_builder(&cli.type_clear, &cli.type_add) {
            Ok(b) => b,
            Err(e) => return fail(&e),
        };
        for d in b.definitions() {
            println!("{}: {}", d.name(), d.globs().join(", "));
        }
        return ExitCode::SUCCESS;
    }
    init_threads(cli.threads);
    let mut patterns = cli.regexp.clone();
    for f in &cli.pattern_file {
        match read_text_arg(f) {
            Ok(t) => patterns.extend(t.lines().map(str::to_string)),
            Err(e) => return fail(&format!("{}: {e}", f.display())),
        }
    }
    let mut args = cli.args.clone().into_iter();
    if patterns.is_empty() && cli.pattern_file.is_empty() && !cli.files {
        match args.next() {
            Some(p) => patterns.push(p),
            None => return fail("a pattern is required (or use --files / --type-list)"),
        }
    }
    let mut o = RgOptions::new(patterns);
    o.paths = args.map(PathBuf::from).collect();
    o.fixed_strings = cli.fixed_strings;
    o.case = if cli.ignore_case {
        Case::Insensitive
    } else if cli.smart_case {
        Case::Smart
    } else {
        Case::Sensitive
    };
    o.word = cli.word_regexp;
    o.line_regexp = cli.line_regexp;
    o.invert = cli.invert_match;
    o.multiline = cli.multiline;
    o.multiline_dotall = cli.multiline_dotall;
    o.only_matching = cli.only_matching;
    o.count = cli.count;
    o.count_matches = cli.count_matches;
    o.files_with_matches = cli.files_with_matches;
    o.files_without_match = cli.files_without_match;
    o.quiet = cli.quiet;
    o.max_count = cli.max_count;
    o.before = cli.before_context.or(cli.context).unwrap_or(0);
    o.after = cli.after_context.or(cli.context).unwrap_or(0);
    if cli.passthru {
        // Whole files as context; `usize::MAX / 4` leaves room for `line + after`.
        (o.passthru, o.before, o.after) = (true, usize::MAX / 4, usize::MAX / 4);
    }
    o.line_number = !cli.no_line_number;
    o.with_filename = if cli.with_filename {
        Some(true)
    } else if cli.no_filename {
        Some(false)
    } else {
        None
    };
    o.column = cli.column;
    o.vimgrep = cli.vimgrep;
    o.json = cli.json;
    let max_filesize = match cli.max_filesize.as_deref() {
        Some(s) => match parse_filesize(s) {
            Ok(v) => Some(v),
            Err(e) => return fail(&e),
        },
        None => None,
    };
    let color = match cli.color.as_deref() {
        None if cli.pretty => true,
        None | Some("auto") => {
            io::stdout().is_terminal()
                && std::env::var_os("NO_COLOR").is_none()
                && std::env::var("TERM").as_deref() != Ok("dumb")
        }
        Some("always") | Some("ansi") => true,
        Some("never") => false,
        Some(other) => {
            return fail(&format!(
                "invalid --color value '{other}'; expected auto, always, ansi, or never"
            ))
        }
    };
    o.null = cli.null;
    o.color = color;
    o.replace = cli.replace;
    o.text = cli.text || cli.unrestricted >= 3;
    o.docs = cli.docs;
    o.globs = cli.glob;
    o.iglobs = cli.iglob;
    o.types = cli.type_;
    o.types_not = cli.type_not;
    o.type_add = cli.type_add;
    o.type_clear = cli.type_clear;
    o.line_number = o.line_number || cli.pretty;
    o.heading = (cli.heading || cli.pretty) && !cli.no_heading;
    o.max_columns = cli.max_columns.filter(|&m| m > 0);
    o.max_columns_preview = cli.max_columns_preview;
    o.byte_offset = cli.byte_offset;
    o.trim = cli.trim;
    o.context_separator = if cli.no_context_separator {
        None
    } else {
        Some(
            cli.context_separator
                .unwrap_or_else(|| "--".into())
                .into_bytes(),
        )
    };
    if let Some(s) = cli.field_match_separator {
        o.field_match_separator = s.into_bytes();
    }
    if let Some(s) = cli.field_context_separator {
        o.field_context_separator = s.into_bytes();
    }
    o.path_separator = cli.path_separator;
    o.include_zero = cli.include_zero;
    o.stats = cli.stats;
    o.sort = match (cli.sort.as_deref(), cli.sortr.as_deref(), cli.sort_files) {
        (_, Some(k), _) => match lancet::rg::SortBy::parse(k) {
            Ok(b) => Some((b, true)),
            Err(e) => return fail(&e),
        },
        (Some(k), None, _) => match lancet::rg::SortBy::parse(k) {
            Ok(b) => Some((b, false)),
            Err(e) => return fail(&e),
        },
        (None, None, _) => None,
    };
    o.glob_case_insensitive = cli.glob_case_insensitive;
    o.stop_on_nonmatch = cli.stop_on_nonmatch;
    o.no_messages = cli.no_messages;
    o.encoding = cli.encoding;
    o.crlf = cli.crlf;
    o.null_data = cli.null_data;
    o.binary = cli.binary;
    o.search_zip = cli.search_zip;
    o.no_unicode = cli.no_unicode;
    o.walk = WalkOpts {
        hidden: cli.hidden || cli.unrestricted >= 2,
        no_ignore: cli.no_ignore || cli.unrestricted >= 1,
        threads: cli.threads.unwrap_or(0),
        follow: cli.follow,
        max_depth: cli.max_depth,
        max_filesize,
        ignore: lancet::walk::IgnoreOpts {
            files: cli.ignore_file,
            no_vcs: cli.no_ignore_vcs,
            no_dot: cli.no_ignore_dot,
            no_parent: cli.no_ignore_parent,
            no_global: cli.no_ignore_global,
            no_exclude: cli.no_ignore_exclude,
            no_files: cli.no_ignore_files,
            no_require_git: cli.no_require_git,
            case_insensitive: cli.ignore_file_case_insensitive,
        },
        one_file_system: cli.one_file_system,
    };
    o.index = index_mode(cli.no_index, cli.trust_index);
    o.index_dir = cli.index_dir;

    let stdout = io::stdout();
    let mut out = io::BufWriter::new(stdout.lock());
    let res = if cli.files {
        list_files(&o, &mut out)
    } else {
        run(&o, &mut out)
    };
    let _ = out.flush();
    match res {
        Ok(true) => ExitCode::SUCCESS,
        Ok(false) => ExitCode::from(1),
        // Already reported (paths that don't exist), like ripgrep: exit 2.
        Err(msg) if msg.is_empty() => ExitCode::from(2),
        Err(msg) => fail(&msg),
    }
}

#[derive(Parser)]
#[command(
    name = "lancet patch",
    about = "Apply a unified diff (`git diff`, `diff -u`) with lancet edit's guarantees (dry-run unless --apply)"
)]
struct PatchCli {
    /// Patch file (`-` or omitted: stdin)
    patch: Option<PathBuf>,
    /// Directory the patch's paths are relative to
    #[arg(long, default_value = ".")]
    root: PathBuf,
    /// Strip N leading path components, like `patch -pN` (1 removes `a/` and `b/`)
    #[arg(short = 'p', long = "strip", default_value_t = 1)]
    strip: usize,
    /// Write the changes (default: verify and print the resulting diff)
    #[arg(long)]
    apply: bool,
    /// Write even if a file gains Tree-sitter syntax errors
    #[arg(long)]
    allow_syntax_errors: bool,
    /// Print machine-readable JSON
    #[arg(long)]
    json: bool,
}

#[derive(Parser)]
#[command(
    name = "lancet rename",
    about = "Rename a local variable, parameter or closure/loop variable and every use of it (dry-run diff unless --apply)",
    long_about = "Rename a local binding by position. A per-language scope resolver (Rust, Python, JavaScript, TypeScript, Go) links every use to its declaration, and the file is resolved again after renaming: if any name would refer to something else (shadowing, capture), nothing is written. Names declared outside a function, fields, methods, Python parameters, and names used inside macros or eval are refused rather than guessed."
)]
struct RenameCli {
    /// Where the name occurs: FILE:LINE or FILE:LINE:COL (1-based byte column, as `lancet rg --column` prints)
    target: String,
    /// The current name
    old: String,
    /// The new name
    new: String,
    /// Refuse unless the file's content revision equals REV (from read/edit JSON)
    #[arg(long)]
    rev: Option<String>,
    /// Write the change atomically (default: print a dry-run diff)
    #[arg(long)]
    apply: bool,
    /// Print machine-readable JSON
    #[arg(long)]
    json: bool,
}

fn run_rename(cli: RenameCli) -> ExitCode {
    // FILE:LINE[:COL]; the file name itself may contain ':'.
    let mut parts = cli.target.rsplitn(3, ':').collect::<Vec<_>>();
    parts.reverse();
    let nums: Vec<Option<usize>> = parts.iter().map(|p| p.parse().ok()).collect();
    let (file, line, col) = match (parts.as_slice(), nums.as_slice()) {
        ([f, _, _], [_, Some(l), Some(c)]) => (f.to_string(), *l, Some(*c)),
        ([f, rest, _], [_, None, Some(l)]) => (format!("{f}:{rest}"), *l, None),
        ([f, _], [_, Some(l)]) => (f.to_string(), *l, None),
        _ => {
            return cli_invalid(
                cli.json,
                &format!("expected FILE:LINE or FILE:LINE:COL, got `{}`", cli.target),
            )
        }
    };
    let o = lancet::rename::RenameOptions {
        path: PathBuf::from(file),
        line,
        col,
        old: cli.old,
        new: cli.new,
        rev: cli.rev,
        apply: cli.apply,
    };
    let resp = lancet::rename::rename(&o);
    let stdout = io::stdout();
    let mut out = io::BufWriter::new(stdout.lock());
    if cli.json {
        let _ = serde_json::to_writer(&mut out, &resp)
            .map_err(io::Error::other)
            .and_then(|_| writeln!(out));
    } else {
        let _ = write!(out, "{}", resp.diff);
        let _ = out.flush();
        if let Some(err) = &resp.error {
            eprintln!("error: {}", err.message);
        } else {
            let comments = if resp.comment_mentions > 0 {
                format!(
                    "; {} comment mention(s) of `{}` left unchanged",
                    resp.comment_mentions, resp.old
                )
            } else {
                String::new()
            };
            eprintln!(
                "[lancet rename] {}: `{}` -> `{}`, {} site(s) in {}{comments}; {}",
                resp.mode,
                resp.old,
                resp.new,
                resp.sites.len(),
                resp.path,
                rev_note(resp.applied, resp.rev.as_deref(), resp.rev_after.as_deref()),
            );
        }
    }
    let _ = out.flush();
    match resp.status {
        Status::Complete => ExitCode::SUCCESS,
        Status::Empty => ExitCode::from(1),
        _ => ExitCode::from(2),
    }
}

#[derive(Parser)]
#[command(
    name = "lancet write",
    about = "Create a file, or replace a whole file you have read (--rev); preview unless --apply"
)]
struct WriteCli {
    /// File to create or replace
    path: PathBuf,
    /// The new content
    #[arg(long, allow_hyphen_values = true, conflicts_with = "with_file")]
    content: Option<String>,
    /// Read the new content from FILE (`-` reads stdin)
    #[arg(long, value_name = "FILE")]
    with_file: Option<PathBuf>,
    /// Required to replace an existing file: the rev you read it at
    #[arg(long)]
    rev: Option<String>,
    /// Write the file (default: print a preview diff)
    #[arg(long)]
    apply: bool,
    /// Write even if the content has more Tree-sitter syntax errors than the file it replaces
    #[arg(long)]
    allow_syntax_errors: bool,
    /// Print machine-readable JSON
    #[arg(long)]
    json: bool,
}

fn run_write(cli: WriteCli) -> ExitCode {
    let content = match (cli.content, &cli.with_file) {
        (Some(c), _) => c,
        (None, Some(f)) => match read_text_arg(f) {
            Ok(t) => t,
            Err(e) => return cli_invalid(cli.json, &format!("cannot read {}: {e}", f.display())),
        },
        (None, None) => {
            return cli_invalid(
                cli.json,
                "give the content with --content TEXT or --with-file FILE (`-` = stdin)",
            )
        }
    };
    let o = lancet::edit_cmd::WriteOptions {
        path: cli.path,
        content,
        rev: cli.rev,
        apply: cli.apply,
        allow_syntax_errors: cli.allow_syntax_errors,
    };
    let resp = lancet::edit_cmd::write_file(&o);
    let stdout = io::stdout();
    let mut out = io::BufWriter::new(stdout.lock());
    if cli.json {
        let _ = serde_json::to_writer(&mut out, &resp)
            .map_err(io::Error::other)
            .and_then(|_| writeln!(out));
    } else {
        let _ = write!(out, "{}", resp.diff);
        let _ = out.flush();
        if let Some(err) = &resp.error {
            eprintln!("error: {}", err.message);
        } else if resp.status == Status::Empty {
            eprintln!(
                "[lancet write] no change: {} already has that content",
                resp.path
            );
        } else {
            let after = resp.rev_after.as_deref().unwrap_or("-");
            match (resp.applied, resp.rev.as_deref()) {
                (false, None) => eprintln!(
                    "[lancet write] dry-run: would create {} ({} lines); re-run with --apply",
                    resp.path, resp.end_line
                ),
                (false, Some(r)) => eprintln!(
                    "[lancet write] dry-run: {}; {}",
                    resp.path,
                    rev_note(false, Some(r), Some(after))
                ),
                (true, None) => eprintln!("[lancet write] created {} (rev {after})", resp.path),
                (true, Some(r)) => eprintln!(
                    "[lancet write] applied: {}; {}",
                    resp.path,
                    rev_note(true, Some(r), Some(after))
                ),
            }
        }
    }
    let _ = out.flush();
    match resp.status {
        Status::Complete => ExitCode::SUCCESS,
        Status::Empty => ExitCode::from(1),
        _ => ExitCode::from(2),
    }
}

/// Which revision to pass next: before writing, the file's current rev (what
/// `--rev` must match); after writing, the new one (for the next edit).
fn rev_note(applied: bool, rev: Option<&str>, rev_after: Option<&str>) -> String {
    let (r, a) = (rev.unwrap_or("-"), rev_after.unwrap_or("-"));
    if applied {
        format!("rev {r} -> {a} (pass --rev {a} to the next edit)")
    } else {
        format!("file is at rev {r}: re-run with --rev {r} --apply to write (afterwards it will be at rev {a})")
    }
}

fn run_patch(cli: PatchCli) -> ExitCode {
    let src = cli.patch.as_deref().unwrap_or(Path::new("-"));
    let text = match read_text_arg(src) {
        Ok(t) => t,
        Err(e) => return cli_invalid(cli.json, &format!("cannot read {}: {e}", src.display())),
    };
    let mut o = lancet::patch::PatchOptions::new(text, &cli.root);
    o.strip = cli.strip;
    o.apply = cli.apply;
    o.allow_syntax_errors = cli.allow_syntax_errors;
    let resp = lancet::patch::apply_patch(&o);
    let stdout = io::stdout();
    let mut out = io::BufWriter::new(stdout.lock());
    if cli.json {
        let _ = serde_json::to_writer(&mut out, &resp)
            .map_err(io::Error::other)
            .and_then(|_| writeln!(out));
    } else {
        for f in &resp.files {
            let _ = write!(out, "{}", f.diff);
        }
        let _ = out.flush();
        if let Some(err) = &resp.error {
            eprintln!("error: {}", err.message);
        } else {
            let moved = resp
                .files
                .iter()
                .flat_map(|f| &f.offsets)
                .filter(|o| **o != 0)
                .count();
            let note = if moved > 0 {
                format!(", {moved} hunk(s) found at an offset")
            } else {
                String::new()
            };
            let hint = if resp.applied {
                ""
            } else {
                " (re-run with --apply to write)"
            };
            eprintln!(
                "[lancet patch] {}: {} file(s){note}{hint}",
                resp.mode,
                resp.files.len()
            );
        }
    }
    let _ = out.flush();
    match resp.status {
        Status::Complete => ExitCode::SUCCESS,
        Status::Empty => ExitCode::from(1),
        _ => ExitCode::from(2),
    }
}

#[derive(Parser)]
#[command(
    name = "lancet mcp",
    about = "Run a Model Context Protocol (MCP) server over stdio: one `lancet` tool that takes CLI arguments"
)]
struct McpCli {
    /// Directory commands run in (relative paths resolve against it)
    #[arg(long, default_value = ".")]
    root: PathBuf,
    /// Refuse commands that write: edit, patch, rename, index, and anything with --apply
    #[arg(long)]
    read_only: bool,
    /// Add --trust-index to searches that don't pick an index mode themselves
    #[arg(long, hide = true)]
    trust_index: bool,
    /// Add -j N to searches that don't set it themselves
    #[arg(short = 'j', long)]
    threads: Option<usize>,
}

fn run_mcp(cli: McpCli) -> ExitCode {
    let mut cfg = lancet::mcp::McpConfig::new(cli.root);
    cfg.read_only = cli.read_only;
    cfg.trust_index = cli.trust_index;
    cfg.threads = cli.threads;
    match lancet::mcp::serve_stdio(cfg) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("lancet mcp: {e}");
            ExitCode::from(2)
        }
    }
}

fn read_text_arg(f: &Path) -> io::Result<String> {
    if f.as_os_str() == "-" {
        let mut s = String::new();
        io::Read::read_to_string(&mut io::stdin(), &mut s).map(|_| s)
    } else {
        std::fs::read_to_string(f)
    }
}

fn run_edit(cli: EditCli) -> ExitCode {
    if let Some(f) = &cli.edits {
        let files = match read_text_arg(f)
            .map_err(|e| format!("cannot read {}: {e}", f.display()))
            .and_then(|j| {
                lancet::edit_cmd::parse_file_hunks(&j, cli.path.as_deref(), cli.rev.as_deref())
            }) {
            Ok(fs) => fs,
            Err(msg) => return cli_invalid(cli.json, &msg),
        };
        if let ([one], Some(p)) = (files.as_slice(), &cli.path) {
            if one.path == *p {
                // Every hunk is for PATH: the single-file edit, same output as before.
                let mut opts = EditOptions::multi(p.clone(), one.hunks.clone());
                opts.rev = one.rev.clone();
                opts.apply = cli.apply;
                opts.allow_syntax_errors = cli.allow_syntax_errors;
                return report_edit(&edit_file(&opts), cli.json);
            }
        }
        let resp = lancet::edit_cmd::edit_files(&files, cli.apply, cli.allow_syntax_errors);
        return report_multi_edit(&resp, cli.json);
    }
    let Some(path) = cli.path.clone() else {
        return cli_invalid(
            cli.json,
            "give the file to edit (or --edits with a \"path\" in every hunk)",
        );
    };
    let target = match (&cli.lines, cli.symbol, cli.find) {
        (Some(l), _, _) => match parse_line_spec(l) {
            Ok((a, b)) => Target::Lines(a, b),
            Err(msg) => return cli_invalid(cli.json, &msg),
        },
        (None, Some(s), _) => Target::Symbol(s),
        (None, None, Some(text)) => Target::Find {
            text,
            all: cli.all,
            count: cli.expected_replacements,
        },
        (None, None, None) => unreachable!("clap requires --lines, --symbol, --find or --edits"),
    };
    let op = if cli.delete {
        Op::Delete
    } else if let Some(t) = cli.with {
        Op::Replace(t)
    } else if let Some(f) = cli.with_file {
        match read_text_arg(&f) {
            Ok(t) => Op::Replace(t),
            Err(e) => return cli_invalid(cli.json, &format!("cannot read {}: {e}", f.display())),
        }
    } else if let Some(t) = cli.insert_before {
        Op::InsertBefore(t)
    } else if let Some(t) = cli.insert_after {
        Op::InsertAfter(t)
    } else {
        return cli_invalid(
            cli.json,
            "give one of --with, --with-file, --delete, --insert-before, --insert-after (or --edits)",
        );
    };

    let mut opts = EditOptions::new(path, target, op);
    opts.rev = cli.rev;
    opts.apply = cli.apply;
    opts.allow_syntax_errors = cli.allow_syntax_errors;
    report_edit(&edit_file(&opts), cli.json)
}

fn report_multi_edit(resp: &lancet::edit_cmd::MultiEditResponse, json: bool) -> ExitCode {
    let stdout = io::stdout();
    let mut out = io::BufWriter::new(stdout.lock());
    if json {
        let _ = serde_json::to_writer(&mut out, resp)
            .map_err(io::Error::other)
            .and_then(|_| writeln!(out));
    } else {
        for f in &resp.files {
            let _ = write!(out, "{}", f.diff);
        }
        let _ = out.flush();
        if let Some(err) = &resp.error {
            eprintln!("error: {}", err.message);
        } else if resp.status == Status::Empty {
            eprintln!("[lancet edit] no change");
        } else {
            eprintln!(
                "[lancet edit] {}: {} file(s){}",
                resp.mode,
                resp.files.len(),
                if resp.applied {
                    " (pass each file's new rev to its next edit)"
                } else {
                    " — re-run with --apply; put each file's rev in its hunks' \"rev\" field"
                }
            );
            for f in &resp.files {
                let (r, a) = (
                    f.rev.as_deref().unwrap_or("-"),
                    f.rev_after.as_deref().unwrap_or("-"),
                );
                if resp.applied {
                    eprintln!("  {}: rev {r} -> {a}", f.path);
                } else {
                    eprintln!("  {}: at rev {r} (afterwards {a})", f.path);
                }
            }
        }
    }
    let _ = out.flush();
    match resp.status {
        Status::Complete => ExitCode::SUCCESS,
        Status::Empty => ExitCode::from(1),
        _ => ExitCode::from(2),
    }
}

fn report_edit(resp: &lancet::edit_cmd::EditResponse, json: bool) -> ExitCode {
    let stdout = io::stdout();
    let mut out = io::BufWriter::new(stdout.lock());
    if json {
        let _ = serde_json::to_writer(&mut out, resp)
            .map_err(io::Error::other)
            .and_then(|_| writeln!(out));
    } else {
        if !resp.diff.is_empty() {
            let _ = write!(out, "{}", resp.diff);
        }
        let _ = out.flush();
        if let Some(err) = &resp.error {
            eprintln!("error: {}", err.message);
        } else if resp.status == Status::Empty {
            eprintln!("[lancet edit] no change");
        } else {
            let what = if resp.hunks.len() > 1 {
                format!("{} hunks", resp.hunks.len())
            } else {
                format!("lines {}-{}", resp.start_line, resp.end_line)
            };
            eprintln!(
                "[lancet edit] {}: {what} of {}; {}",
                resp.mode,
                resp.path,
                rev_note(resp.applied, resp.rev.as_deref(), resp.rev_after.as_deref()),
            );
        }
    }
    let _ = out.flush();
    match resp.status {
        Status::Complete => ExitCode::SUCCESS,
        Status::Empty => ExitCode::from(1),
        _ => ExitCode::from(2),
    }
}

fn run_read(cli: ReadCli) -> ExitCode {
    let lines = match cli.lines.as_deref().map(parse_line_spec).transpose() {
        Ok(v) => v,
        Err(msg) => return cli_invalid(cli.json, &msg),
    };
    let lines = match (cli.offset, cli.limit) {
        (None, None) => lines,
        (_, Some(0)) => return cli_invalid(cli.json, "--limit must be at least 1"),
        (off, lim) => {
            let start = off.unwrap_or(1).max(1);
            Some((start, lim.map(|n| start + n - 1)))
        }
    };
    let pages = match cli.page.as_deref() {
        Some(raw) => match parse_line_spec(raw) {
            Ok((s, e_opt)) => Some((s, e_opt.unwrap_or(s))),
            Err(msg) => return cli_invalid(cli.json, &msg),
        },
        None => None,
    };
    let rows = match cli.rows.as_deref().map(parse_line_spec).transpose() {
        Ok(v) => v,
        Err(msg) => return cli_invalid(cli.json, &msg),
    };
    let cols = match cli.cols.as_deref().map(parse_col_spec).transpose() {
        Ok(v) => v,
        Err(msg) => return cli_invalid(cli.json, &msg),
    };

    let mut opts = ReadOptions::new(&cli.target[0]);
    opts.lines = lines;
    opts.symbol = cli.symbol;
    opts.pages = pages;
    opts.sheet = cli.sheet;
    opts.rows = rows;
    opts.cols = cols;
    opts.entry = cli.entry;
    opts.budget_tokens = cli.budget_tokens;
    opts.raw = cli.raw;
    opts.ocr = cli.ocr;

    let resps: Vec<_> = cli
        .target
        .iter()
        .map(|t| {
            let mut o = opts.clone();
            o.target = t.clone();
            read_file_range(&o)
        })
        .collect();
    let stdout = io::stdout();
    let mut out = io::BufWriter::new(stdout.lock());
    let multi = resps.len() > 1;
    let code = |s: Status| match s {
        Status::Complete => 0u8,
        Status::Empty => 1,
        _ => 2,
    };
    let worst = resps.iter().map(|r| code(r.status)).max().unwrap_or(0);
    if cli.json {
        let _ = if multi {
            let status = resps
                .iter()
                .max_by_key(|r| code(r.status))
                .map(|r| r.status);
            serde_json::to_writer(
                &mut out,
                &serde_json::json!({"status": status, "results": resps}),
            )
        } else {
            serde_json::to_writer(&mut out, &resps[0])
        }
        .map_err(io::Error::other)
        .and_then(|_| writeln!(out));
    } else {
        for (i, resp) in resps.iter().enumerate() {
            if multi && i > 0 {
                let _ = writeln!(out);
            }
            let _ = print_read_human(resp, cli.raw, &mut out);
        }
    }
    let _ = out.flush();

    if !cli.json {
        for (resp, img) in resps
            .iter()
            .filter_map(|r| r.image.as_ref().map(|i| (r, i)))
        {
            eprintln!(
                "[lancet read] {} is an image ({} KB). The CLI prints text: look at it with an image-capable tool (the `lancet` MCP tool returns it), or pass --ocr for its text",
                resp.path,
                img.bytes.div_ceil(1024)
            );
        }
        for resp in resps
            .iter()
            .filter(|r| r.status == Status::Empty && r.lines.is_empty())
        {
            eprintln!(
                "[lancet read] {}: nothing in that range (the file has {} lines)",
                resp.path, resp.total_lines
            );
        }
        for resp in resps.iter().filter(|r| r.truncated) {
            if let Some(next) = resp.next_line {
                let which = if multi {
                    format!("{}: ", resp.path)
                } else {
                    String::new()
                };
                eprintln!(
                    "[lancet read] {which}truncated at ~{} tokens (resume with --lines {next}:)",
                    resp.estimated_tokens
                );
            }
        }
    }
    ExitCode::from(worst)
}

fn run_outline(cli: OutlineCli) -> ExitCode {
    init_threads(cli.threads);
    let mut opts = OutlineOptions::new(&cli.path);
    opts.max_depth = cli.max_depth;
    opts.exts = cli.exts;
    opts.limit = cli.limit;
    opts.walk = WalkOpts {
        hidden: cli.hidden,
        no_ignore: cli.no_ignore,
        threads: cli.threads.unwrap_or(0),
        ..WalkOpts::default()
    };
    opts.ocr = cli.ocr;

    let resp = build_outline(&opts);
    let stdout = io::stdout();
    let mut out = io::BufWriter::new(stdout.lock());
    let _ = if cli.json {
        serde_json::to_writer(&mut out, &resp)
            .map_err(io::Error::other)
            .and_then(|_| writeln!(out))
    } else {
        print_outline_human(&resp, &mut out)
    };
    let _ = out.flush();

    if !cli.json && resp.truncated {
        eprintln!(
            "[lancet outline] output truncated to {} files",
            resp.files.len()
        );
    }
    match resp.status {
        Status::Complete => ExitCode::SUCCESS,
        Status::Empty => ExitCode::from(1),
        _ => ExitCode::from(2),
    }
}

fn run_refs(cli: RefsCli) -> ExitCode {
    init_threads(cli.threads);
    let mut opts = RefsOptions::new(&cli.symbol, &cli.path);
    opts.lang = cli.lang;
    opts.exts = cli.exts;
    opts.paths = cli.paths;
    opts.all_files = cli.all_files;
    opts.limit = cli.limit;
    opts.walk = WalkOpts {
        hidden: cli.hidden,
        no_ignore: cli.no_ignore,
        threads: cli.threads.unwrap_or(0),
        ..WalkOpts::default()
    };
    opts.index = index_mode(cli.no_index, cli.trust_index);
    opts.index_dir = cli.index_dir;
    if cli.rank {
        return run_rank(&opts, &cli.kinds, cli.json);
    }

    let resp = find_refs(&opts);
    let stdout = io::stdout();
    let mut out = io::BufWriter::new(stdout.lock());
    let _ = if cli.json {
        serde_json::to_writer(&mut out, &resp)
            .map_err(io::Error::other)
            .and_then(|_| writeln!(out))
    } else {
        print_refs_human(&resp, &mut out)
    };
    let _ = out.flush();

    for w in &resp.warnings {
        eprintln!("[lancet refs] warning: {w}");
    }
    if cli.stats {
        eprintln!(
            "[lancet refs] mode={} defs={} refs={} {}",
            resp.mode,
            resp.definitions.len(),
            resp.total_references,
            serde_json::to_string(&resp.stats).unwrap_or_default()
        );
    }
    match resp.status {
        Status::Complete => ExitCode::SUCCESS,
        Status::Empty => ExitCode::from(1),
        _ => ExitCode::from(2),
    }
}

fn run_ast(cli: AstCli) -> ExitCode {
    init_threads(cli.threads);
    let mut opts = AstOptions::new(&cli.pattern, &cli.path);
    opts.lang = cli.lang;
    opts.rewrite = cli.rewrite;
    opts.apply = cli.apply;
    opts.paths = cli.paths;
    opts.limit = cli.limit;
    opts.walk = WalkOpts {
        hidden: cli.hidden,
        no_ignore: cli.no_ignore,
        threads: cli.threads.unwrap_or(0),
        ..WalkOpts::default()
    };
    opts.index = index_mode(cli.no_index, cli.trust_index);
    opts.index_dir = cli.index_dir;

    let resp = search_ast(&opts);
    let stdout = io::stdout();
    let mut out = io::BufWriter::new(stdout.lock());
    let _ = if cli.json {
        serde_json::to_writer(&mut out, &resp)
            .map_err(io::Error::other)
            .and_then(|_| writeln!(out))
    } else {
        print_ast_human(&resp, &mut out)
    };
    let _ = out.flush();

    if !cli.json && !resp.diffs.is_empty() {
        let total_rep: usize = resp.diffs.iter().map(|d| d.replacements).sum();
        let total_skip: usize = resp.diffs.iter().map(|d| d.skipped_overlaps).sum();
        let action = if cli.apply { "applied" } else { "dry-run" };
        let hint = if cli.apply {
            ""
        } else {
            " (re-run with --apply to write)"
        };
        let skip_note = if total_skip > 0 {
            format!(", {total_skip} overlapping nested match(es) skipped")
        } else {
            String::new()
        };
        eprintln!(
            "[lancet ast] {action}: {total_rep} replacement(s) across {} file(s){skip_note}{hint}",
            resp.diffs.len()
        );
    } else if !cli.json && resp.truncated {
        eprintln!(
            "[lancet ast] showing {} of {} matches",
            resp.returned, resp.matched
        );
    }
    for w in &resp.warnings {
        eprintln!("[lancet ast] warning: {w}");
    }
    if cli.stats {
        eprintln!(
            "[lancet ast] mode={} lang={} {}",
            resp.mode,
            resp.lang.as_deref().unwrap_or("-"),
            serde_json::to_string(&resp.stats).unwrap_or_default()
        );
    }
    match resp.status {
        Status::Complete => ExitCode::SUCCESS,
        Status::Empty => ExitCode::from(1),
        _ => ExitCode::from(2),
    }
}
