use clap::{ArgGroup, Parser};
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
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

const ABOUT: &str =
    "Lancet: surgical code, AST, and multi-format document search, inspection, and refactoring CLI.

Examples:
  lancet '\"optimizer\" in ext:py'
  lancet '(\"OOM\" OR \"out of memory\") NOT \"CPU\" in line'
  lancet 'near(\"gradient\", \"clip\", 30) in function | expand parent | limit 5' src
  lancet fd --tree --max-depth 2 .
  lancet ast '$X.unwrap()' src --lang rs --rewrite '$X?'
  lancet edit src/search.rs --symbol execute_one --with-file new.rs --rev <REV>
  lancet read src/search.rs --symbol execute_one
  lancet read report.pdf --page 2
  lancet outline src/search.rs
  lancet refs SearchOptions src

Subcommands:
  lancet fd [PATTERN] [PATH]    fast .gitignore-aware file & folder finder / tree view
  lancet ast <PATTERN> [PATH]   tree-sitter structural search & safe AST rewrite
  lancet read <TARGET>          surgical line / symbol / page / sheet / archive reader
  lancet edit <PATH>            replace / delete / insert lines addressed like `read`
  lancet outline [PATH]         document table-of-contents & code declaration skeleton
  lancet refs <SYMBOL> [PATH]   lexical AST symbol definitions & references
  lancet index [PATH]           build or incrementally update the index (PATH/.lancet)
  lancet status [PATH]          show index status
  lancet parse <QUERY>          print the canonical query and its IR as JSON
  lancet explain <QUERY>        show trigram requirements and index candidate selectivity
  lancet batch [PATH]           run multiple queries over one shared index/walk";

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
    #[arg(long)]
    no_index: bool,
    /// Use the index without checking for changed files (fastest, may be stale)
    #[arg(long, conflicts_with = "no_index")]
    trust_index: bool,
    /// Index directory (default: PATH/.lancet)
    #[arg(long)]
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
    #[arg(long)]
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
    #[arg(long)]
    index_dir: Option<PathBuf>,
    #[arg(long)]
    no_index: bool,
    #[arg(long, conflicts_with = "no_index")]
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
    #[arg(long)]
    no_index: bool,
    #[arg(long, conflicts_with = "no_index")]
    trust_index: bool,
    #[arg(long)]
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
    /// Render matched entries as an indented ASCII directory tree
    #[arg(long)]
    tree: bool,
    /// Maximum number of entries returned
    #[arg(long)]
    limit: Option<usize>,
    /// Ignore the index and walk the filesystem directly
    #[arg(long)]
    no_index: bool,
    /// Read file entries directly from the mmap'd Lancet index without walking disk
    #[arg(long, conflicts_with = "no_index")]
    trust_index: bool,
    /// Custom index directory (default: PATH/.lancet)
    #[arg(long)]
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
    #[arg(long)]
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
    /// Source language (rs/rust, py/python, js, ts, tsx, go)
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
    #[arg(long)]
    no_index: bool,
    /// Use the index without checking mtime/size on disk (fastest)
    #[arg(long, conflicts_with = "no_index")]
    trust_index: bool,
    /// Custom index directory (default: PATH/.lancet or ancestor)
    #[arg(long)]
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
    #[arg(long)]
    stats: bool,
}

#[derive(Parser)]
#[command(
    name = "lancet read",
    about = "Read specific lines, AST symbols, PDF/PPTX pages, spreadsheet cells, or archive entries"
)]
struct ReadCli {
    /// File path (supports `file:10-50`, `file:42`, or `archive.zip!inner.log`)
    target: String,
    /// 1-based line range (`10:50`, `42`, `100:`)
    #[arg(short = 'l', long)]
    lines: Option<String>,
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

#[derive(Parser)]
#[command(
    name = "lancet refs",
    about = "Find symbol definitions and lexical AST references (excluding comments & strings)"
)]
struct RefsCli {
    /// Exact identifier / symbol name
    symbol: String,
    /// Directory or file to search
    #[arg(default_value = ".")]
    path: PathBuf,
    /// Restrict to a specific AST language (`rs`, `py`, `js`, `ts`, `tsx`, `go`)
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
    #[arg(long)]
    no_index: bool,
    /// Use the index without checking mtime/size on disk (fastest)
    #[arg(long, conflicts_with = "no_index")]
    trust_index: bool,
    /// Custom index directory (default: PATH/.lancet or ancestor)
    #[arg(long)]
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
    #[arg(long)]
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

fn resolve_fd_args(
    args: &[String],
    explicit_pat: Option<String>,
) -> Result<(Option<String>, PathBuf), String> {
    if let Some(pat) = explicit_pat {
        return match args {
            [] => Ok((Some(pat), PathBuf::from("."))),
            [p] => Ok((Some(pat), PathBuf::from(p))),
            _ => Err("too many positional arguments when `--pattern` is set".into()),
        };
    }
    match args {
        [] => Ok((None, PathBuf::from("."))),
        [single] => {
            let p = Path::new(single);
            if p.is_dir() {
                Ok((None, PathBuf::from(single)))
            } else {
                Ok((Some(single.clone()), PathBuf::from(".")))
            }
        }
        [pat, path] => Ok((Some(pat.clone()), PathBuf::from(path))),
        _ => Err("usage: `lancet fd [PATTERN] [PATH]`".into()),
    }
}

fn run_fd(cli: FdCli) -> ExitCode {
    init_threads(cli.threads);
    let (pattern, root) = match resolve_fd_args(&cli.args, cli.pattern) {
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
    let mut include_dirs = false;
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

    let mut opts = FdOptions::new(&root);
    opts.pattern = pattern;
    opts.pattern_mode = if cli.regex {
        PatternMode::Regex
    } else if cli.glob {
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
    };
    opts.index = index_mode(cli.no_index, cli.trust_index);
    opts.index_dir = cli.index_dir;
    opts.need_meta = cli.json && cli.meta;

    let resp = find_files(&opts);
    let stdout = io::stdout();
    let mut out = io::BufWriter::new(stdout.lock());
    let _ = if cli.json {
        serde_json::to_writer(&mut out, &resp)
            .map_err(io::Error::other)
            .and_then(|_| writeln!(out))
    } else {
        print_fd_human(&resp, &root, cli.tree, &mut out)
    };
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
    about = "Replace, delete, or insert lines addressed like `lancet read` (dry-run diff unless --apply)",
    group(ArgGroup::new("target").required(true).args(["lines", "symbol"])),
    group(ArgGroup::new("op").required(true).args(["with", "with_file", "delete", "insert_before", "insert_after"]))
)]
struct EditCli {
    /// File to edit (plain-text files; documents and archives are read-only)
    path: PathBuf,
    /// 1-based line range, as in `lancet read` (`10:50`, `42`, `100:`)
    #[arg(short = 'l', long)]
    lines: Option<String>,
    /// The lines of an AST symbol, exactly as `lancet read --symbol` shows them
    /// (doc comments and attributes above it included: replace them too, or they go)
    #[arg(short = 's', long)]
    symbol: Option<String>,
    /// Replace the addressed lines with TEXT
    #[arg(long, allow_hyphen_values = true)]
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

fn run_edit(cli: EditCli) -> ExitCode {
    let target = match (&cli.lines, cli.symbol) {
        (Some(l), _) => match parse_line_spec(l) {
            Ok((a, b)) => Target::Lines(a, b),
            Err(msg) => return cli_invalid(cli.json, &msg),
        },
        (None, Some(s)) => Target::Symbol(s),
        (None, None) => unreachable!("clap requires --lines or --symbol"),
    };
    let op = if cli.delete {
        Op::Delete
    } else if let Some(t) = cli.with {
        Op::Replace(t)
    } else if let Some(f) = cli.with_file {
        let text = if f.as_os_str() == "-" {
            let mut s = String::new();
            io::Read::read_to_string(&mut io::stdin(), &mut s).map(|_| s)
        } else {
            std::fs::read_to_string(&f)
        };
        match text {
            Ok(t) => Op::Replace(t),
            Err(e) => return cli_invalid(cli.json, &format!("cannot read {}: {e}", f.display())),
        }
    } else if let Some(t) = cli.insert_before {
        Op::InsertBefore(t)
    } else if let Some(t) = cli.insert_after {
        Op::InsertAfter(t)
    } else {
        unreachable!("clap requires one edit operation")
    };

    let mut opts = EditOptions::new(cli.path, target, op);
    opts.rev = cli.rev;
    opts.apply = cli.apply;
    opts.allow_syntax_errors = cli.allow_syntax_errors;
    let resp = edit_file(&opts);

    let stdout = io::stdout();
    let mut out = io::BufWriter::new(stdout.lock());
    if cli.json {
        let _ = serde_json::to_writer(&mut out, &resp)
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
            let hint = if resp.applied {
                ""
            } else {
                " (re-run with --apply to write)"
            };
            eprintln!(
                "[lancet edit] {}: lines {}-{} of {}, rev {}{hint}",
                resp.mode,
                resp.start_line,
                resp.end_line,
                resp.path,
                resp.rev_after.as_deref().unwrap_or("-"),
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

    let mut opts = ReadOptions::new(&cli.target);
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

    let resp = read_file_range(&opts);
    let stdout = io::stdout();
    let mut out = io::BufWriter::new(stdout.lock());
    let _ = if cli.json {
        serde_json::to_writer(&mut out, &resp)
            .map_err(io::Error::other)
            .and_then(|_| writeln!(out))
    } else {
        print_read_human(&resp, cli.raw, &mut out)
    };
    let _ = out.flush();

    if !cli.json && resp.truncated {
        if let Some(next) = resp.next_line {
            eprintln!(
                "[lancet read] truncated at ~{} tokens (resume with --lines {next}:)",
                resp.estimated_tokens
            );
        }
    }
    match resp.status {
        Status::Complete => ExitCode::SUCCESS,
        Status::Empty => ExitCode::from(1),
        _ => ExitCode::from(2),
    }
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
    };
    opts.index = index_mode(cli.no_index, cli.trust_index);
    opts.index_dir = cli.index_dir;

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
