//! `lancet fd`: command-line arguments and output.

use super::*;

pub(crate) fn main(args: &[OsString]) -> ExitCode {
    run_fd(FdCli::parse_from(args))
}

#[derive(Parser)]
#[command(
    name = "lancet fd",
    about = "Fast .gitignore-aware file & folder explorer"
)]
pub(crate) struct FdCli {
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

/// `[PATTERN] [PATH...]`, like fd. A first argument that is an existing directory
/// (or, alone, looks like a path: has a `/` and no glob characters) is a path.
pub(crate) fn resolve_fd_args(
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

pub(crate) fn run_fd(cli: FdCli) -> ExitCode {
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
    exit(resp.status)
}
