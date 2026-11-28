//! `lancet fd`: fd 10.5's command line (flags, output, exit codes), plus
//! lancet's `--tree`, `--dirs`, `--type-class`, `--json` and index modes.

use super::*;
use lancet::fd::{
    find_files, now_ns, parse_time, print_fd_human, CaseMode, FdEntry, FdOptions, OutputStyle,
    OwnerFilter, PatternMode, SizeFilter, Types,
};

pub(crate) fn main(args: &[OsString]) -> ExitCode {
    run_fd(FdCli::parse_from(args))
}

#[derive(Parser)]
#[command(
    name = "lancet fd",
    about = "fd: find entries by name with regex or glob patterns (same flags, output and exit codes), on lancet's walker and index",
    override_usage = "lancet fd [OPTIONS] [pattern] [path]..."
)]
pub(crate) struct FdCli {
    /// The search pattern (a regex, or a glob with -g); then the paths to search
    #[arg(value_name = "pattern] [path")]
    args: Vec<String>,

    /// Include hidden files and directories
    #[arg(short = 'H', long, overrides_with = "no_hidden")]
    hidden: bool,
    #[arg(long, hide = true, overrides_with = "hidden")]
    no_hidden: bool,
    /// Don't respect .gitignore, .ignore, .fdignore or global ignore files
    #[arg(short = 'I', long, overrides_with = "ignore")]
    no_ignore: bool,
    #[arg(long, hide = true, overrides_with = "no_ignore")]
    ignore: bool,
    /// Don't respect .gitignore files
    #[arg(long, overrides_with = "ignore_vcs")]
    no_ignore_vcs: bool,
    #[arg(long, hide = true, overrides_with = "no_ignore_vcs")]
    ignore_vcs: bool,
    /// Respect gitignore rules outside git repositories too
    #[arg(long, overrides_with = "require_git")]
    no_require_git: bool,
    #[arg(long, hide = true, overrides_with = "no_require_git")]
    require_git: bool,
    /// Don't respect ignore files in parent directories
    #[arg(long, overrides_with = "ignore_parent")]
    no_ignore_parent: bool,
    #[arg(long, hide = true, overrides_with = "no_ignore_parent")]
    ignore_parent: bool,
    /// Alias for --no-ignore --hidden
    #[arg(short = 'u', long = "unrestricted", action = ArgAction::Count)]
    unrestricted: u8,
    /// Case-sensitive search (default: smart case)
    #[arg(short = 's', long, overrides_with = "ignore_case")]
    case_sensitive: bool,
    /// Case-insensitive search (default: smart case)
    #[arg(short = 'i', long, overrides_with = "case_sensitive")]
    ignore_case: bool,
    /// Glob-based search instead of a regular expression
    #[arg(short = 'g', long, overrides_with_all = ["regex", "fixed_strings", "exact"])]
    glob: bool,
    /// Regular-expression search (default)
    #[arg(long, overrides_with_all = ["glob", "fixed_strings", "exact"])]
    regex: bool,
    /// Treat the pattern as a literal substring
    #[arg(short = 'F', long, overrides_with_all = ["glob", "regex", "exact"])]
    fixed_strings: bool,
    /// The pattern must equal the whole name (or path with -p)
    #[arg(long, overrides_with_all = ["glob", "regex", "fixed_strings"])]
    exact: bool,
    /// Additional required patterns
    #[arg(long = "and", value_name = "pattern")]
    and: Vec<String>,
    /// Show absolute paths
    #[arg(short = 'a', long, overrides_with = "relative_path")]
    absolute_path: bool,
    #[arg(long, hide = true, overrides_with = "absolute_path")]
    relative_path: bool,
    /// Not supported (it runs `ls`): use `lancet fd -0 … | xargs -0 ls -l`
    #[arg(short = 'l', long)]
    list_details: bool,
    /// Descend into symlinked directories
    #[arg(short = 'L', long, overrides_with = "no_follow")]
    follow: bool,
    #[arg(long, hide = true, overrides_with = "follow")]
    no_follow: bool,
    /// Match the pattern against the full (absolute) path
    #[arg(short = 'p', long)]
    full_path: bool,
    /// Separate results by NUL
    #[arg(short = '0', long)]
    print0: bool,
    /// Maximum depth
    #[arg(short = 'd', long, visible_alias = "maxdepth", value_name = "depth")]
    max_depth: Option<usize>,
    /// Only results at this depth or deeper
    #[arg(long, value_name = "depth")]
    min_depth: Option<usize>,
    /// Only results at exactly this depth
    #[arg(long, value_name = "depth")]
    exact_depth: Option<usize>,
    /// Exclude entries matching the glob (overrides ignore rules)
    #[arg(short = 'E', long, value_name = "glob")]
    exclude: Vec<String>,
    /// Don't descend into directories that match
    #[arg(long)]
    prune: bool,
    /// f, d, l, s, p, b, c, x (executable), e (empty); repeatable
    #[arg(short = 't', long = "type", value_name = "filetype")]
    types: Vec<String>,
    /// Filter by extension; repeatable
    #[arg(short = 'e', long = "extension", value_name = "ext")]
    extensions: Vec<String>,
    /// Size filter: [+-]NUM UNIT (b, k, m, g, t, ki, mi, gi, ti)
    #[arg(short = 'S', long, value_name = "size", allow_hyphen_values = true, value_parser = parse_size_arg)]
    size: Vec<SizeFilter>,
    /// Modified after a date (YYYY-MM-DD[ HH:MM:SS], @timestamp) or within a duration (10h, 1d, 2weeks)
    #[arg(long, value_name = "date|dur", visible_aliases = ["change-newer-than", "newer", "changed-after"])]
    changed_within: Option<String>,
    /// Modified before a date or longer ago than a duration
    #[arg(long, value_name = "date|dur", visible_aliases = ["change-older-than", "older"])]
    changed_before: Option<String>,
    /// Filter by owner: [(user|uid)][:(group|gid)], `!` to negate
    #[arg(
        short = 'o',
        long,
        value_name = "user:group",
        allow_hyphen_values = true
    )]
    owner: Option<String>,
    /// Print each result with a template: {} {/} {//} {.} {/.}
    #[arg(long, value_name = "fmt")]
    format: Option<String>,
    /// Not supported: use `lancet fd -0 … | xargs -0 CMD`
    #[arg(short = 'x', long, num_args = 1.., allow_hyphen_values = true, value_name = "cmd")]
    exec: Vec<String>,
    /// Not supported: use `lancet fd -0 … | xargs -0 CMD`
    #[arg(short = 'X', long, num_args = 1.., allow_hyphen_values = true, value_name = "cmd")]
    exec_batch: Vec<String>,
    #[arg(long, hide = true)]
    batch_size: Option<usize>,
    /// Add a gitignore-format ignore file
    #[arg(long, value_name = "path")]
    ignore_file: Vec<PathBuf>,
    /// Accepted for compatibility; lancet doesn't color fd output
    #[arg(short = 'c', long, value_name = "when")]
    color: Option<String>,
    /// Accepted for compatibility; no effect
    #[arg(long, num_args = 0..=1, require_equals = true, default_missing_value = "auto", value_name = "when")]
    hyperlink: Option<String>,
    /// Skip directories that contain an entry with this name
    #[arg(long, value_name = "name")]
    ignore_contain: Vec<String>,
    /// Worker threads (default: number of CPUs)
    #[arg(short = 'j', long, value_name = "num")]
    threads: Option<usize>,
    /// Stop after this many results (the first ones in path order)
    #[arg(long, value_name = "count")]
    max_results: Option<usize>,
    /// Alias for --max-results=1
    #[arg(short = '1')]
    one: bool,
    /// Print nothing; exit 0 if anything matches, 1 otherwise
    #[arg(short = 'q', long, visible_alias = "has-results")]
    quiet: bool,
    /// Accepted for compatibility
    #[arg(long)]
    show_errors: bool,
    /// Change to this directory first
    #[arg(short = 'C', long, value_name = "path")]
    base_directory: Option<PathBuf>,
    /// Path separator for output
    #[arg(long, value_name = "separator")]
    path_separator: Option<String>,
    /// Paths to search (instead of positional paths)
    #[arg(long, value_name = "search-path")]
    search_path: Vec<String>,
    /// auto (default), always, never: whether results of the implicit `.` keep `./`
    #[arg(long, num_args = 0..=1, require_equals = true, default_missing_value = "always", value_name = "when")]
    strip_cwd_prefix: Option<String>,
    /// Don't cross file systems
    #[arg(long, visible_aliases = ["mount", "xdev"])]
    one_file_system: bool,

    /// lancet: draw the results as a tree (add --dirs for folders only)
    #[arg(long)]
    tree: bool,
    /// lancet: only directories (same as -t d)
    #[arg(long)]
    dirs: bool,
    /// lancet: files of these classes (code, document, spreadsheet, config, log, data, archive, image, other)
    #[arg(long = "type-class", value_delimiter = ',')]
    type_classes: Vec<String>,
    /// lancet: machine-readable JSON
    #[arg(long)]
    json: bool,
    /// lancet: size and mtime_ns in JSON entries
    #[arg(long)]
    meta: bool,
    #[arg(long, hide = true)]
    no_index: bool,
    #[arg(long, conflicts_with = "no_index", hide = true)]
    trust_index: bool,
    #[arg(long, hide = true)]
    index_dir: Option<PathBuf>,
    #[arg(long, hide = true)]
    stats: bool,
}

fn parse_size_arg(s: &str) -> Result<SizeFilter, String> {
    SizeFilter::parse(s)
}

/// An fd-style failure: message on stderr, exit 1 (fd's code for its own errors).
fn fd_error(json: bool, msg: &str) -> ExitCode {
    if json {
        println!(
            "{}",
            serde_json::json!({ "status": "invalid_query", "error": { "message": msg } })
        );
    } else {
        eprintln!("[lancet fd] error: {msg}");
    }
    ExitCode::from(1)
}

pub(crate) fn run_fd(cli: FdCli) -> ExitCode {
    init_threads(cli.threads);
    if cli.list_details || !cli.exec.is_empty() || !cli.exec_batch.is_empty() {
        let flag = if cli.list_details { "-l" } else { "-x/-X" };
        eprintln!(
            "[lancet fd] error: {flag} is not supported: lancet doesn't run a process per result. Use `lancet fd -0 … | xargs -0 CMD` (or `xargs -0 ls -l`)."
        );
        return ExitCode::from(2);
    }
    if let Some(dir) = &cli.base_directory {
        if !dir.is_dir() || std::env::set_current_dir(dir).is_err() {
            return fd_error(
                cli.json,
                &format!(
                    "The '--base-directory' path '{}' is not a directory.",
                    dir.display()
                ),
            );
        }
    }

    // `fd [pattern] [path]...`, or `fd --search-path P… [pattern]`.
    let (pattern, path_args): (Option<String>, Vec<String>) = if cli.search_path.is_empty() {
        let mut it = cli.args.iter().cloned();
        (it.next(), it.collect())
    } else {
        if cli.args.len() > 1 {
            return fd_error(
                cli.json,
                "with --search-path, only a pattern may be given as an argument",
            );
        }
        (cli.args.first().cloned(), cli.search_path.clone())
    };
    if let Some(p) = &pattern {
        if !cli.full_path && p.contains('/') && p.contains(['*', '?', '[']) {
            // A Glob-tool pattern (`src/**/*.ts`): say how fd spells it.
            let name = p.rsplit('/').next().unwrap_or(p);
            let dir: Vec<&str> = p
                .split('/')
                .take_while(|seg| !seg.contains(['*', '?', '[']))
                .collect();
            let dir = dir.join("/");
            return fd_error(
                cli.json,
                &format!(
                    "The search pattern '{p}' contains a path separator. fd matches names: use `lancet fd -g '{name}'{}` (or `--full-path` to match whole paths).",
                    if dir.is_empty() { String::new() } else { format!(" {dir}") }
                ),
            );
        }
        if !cli.full_path && p.contains('/') {
            return fd_error(
                cli.json,
                &format!(
                    "The search pattern '{p}' contains a path-separation character and will not lead to any search results.\n\nIf you want to search for all files inside the '{p}' directory, use a match-all pattern:\n\n  lancet fd . '{p}'\n\nInstead, if you want your pattern to match the full file path, use:\n\n  lancet fd --full-path '{p}'"
                ),
            );
        }
    }
    let explicit_root = !path_args.is_empty();
    let mut roots: Vec<PathBuf> = Vec::new();
    for p in if explicit_root {
        path_args
    } else {
        vec![".".to_string()]
    } {
        let pb = PathBuf::from(&p);
        if pb.is_dir() {
            roots.push(pb);
        } else if !cli.json {
            eprintln!("[lancet fd] error: Search path \"{p}\" is not a directory.");
        }
    }
    if roots.is_empty() {
        return fd_error(cli.json, "No valid search paths given.");
    }

    let mut types = match Types::parse(&cli.types) {
        Ok(t) => t,
        Err(e) => {
            eprintln!("error: {e}");
            return ExitCode::from(2);
        }
    };
    if cli.dirs {
        let t = types.get_or_insert_with(Types::default);
        t.dir = true;
    }
    let mut file_types = Vec::new();
    for tc in &cli.type_classes {
        match FileType::parse(tc) {
            Some(ft) => file_types.push(ft),
            None => {
                eprintln!("error: unknown --type-class `{tc}`");
                return ExitCode::from(2);
            }
        }
    }
    let now = now_ns();
    let time = |s: &Option<String>| s.as_deref().map(|v| parse_time(v, now)).transpose();
    let (after, before) = match (time(&cli.changed_within), time(&cli.changed_before)) {
        (Ok(a), Ok(b)) => (a, b),
        (Err(e), _) | (_, Err(e)) => return fd_error(cli.json, &e),
    };
    let owner = match cli.owner.as_deref().map(OwnerFilter::parse).transpose() {
        Ok(o) => o,
        Err(e) => return fd_error(cli.json, &e),
    };

    let mut opts = FdOptions::new(&roots[0]);
    opts.patterns = pattern.into_iter().chain(cli.and.iter().cloned()).collect();
    opts.pattern_mode = if cli.glob {
        PatternMode::Glob
    } else if cli.fixed_strings {
        PatternMode::Fixed
    } else if cli.exact {
        PatternMode::Exact
    } else {
        PatternMode::Regex
    };
    opts.case = if cli.case_sensitive {
        CaseMode::Sensitive
    } else if cli.ignore_case {
        CaseMode::Insensitive
    } else {
        CaseMode::Smart
    };
    opts.full_path = cli.full_path;
    opts.types = types;
    opts.exts = cli.extensions.clone();
    opts.file_types = file_types;
    opts.min_depth = cli.exact_depth.or(cli.min_depth);
    opts.max_depth = cli.exact_depth.or(cli.max_depth);
    opts.excludes = cli.exclude.clone();
    opts.sizes = cli.size.clone();
    opts.changed_after_ns = after;
    opts.changed_before_ns = before;
    opts.owner = owner;
    opts.prune = cli.prune;
    opts.ignore_contain = cli.ignore_contain.clone();
    opts.limit = if cli.quiet || cli.one {
        Some(1)
    } else {
        cli.max_results
    };
    opts.tree = cli.tree;
    let unrestricted = cli.unrestricted > 0;
    opts.walk = WalkOpts {
        hidden: cli.hidden || unrestricted,
        no_ignore: cli.no_ignore || unrestricted,
        threads: cli.threads.unwrap_or(0),
        follow: cli.follow,
        one_file_system: cli.one_file_system,
        ignore: lancet::walk::IgnoreOpts {
            files: cli.ignore_file.clone(),
            no_vcs: cli.no_ignore_vcs,
            no_parent: cli.no_ignore_parent,
            no_require_git: cli.no_require_git,
            ..Default::default()
        },
        ..WalkOpts::default()
    };
    opts.index = index_mode(cli.no_index, cli.trust_index);
    opts.index_dir = cli.index_dir.clone();
    opts.need_meta = cli.json && cli.meta;

    let resps: Vec<(PathBuf, lancet::fd::FdResponse)> = roots
        .iter()
        .map(|root| {
            let mut o = opts.clone();
            o.root = root.clone();
            (root.clone(), find_files(&o))
        })
        .collect();
    let found = resps.iter().any(|(_, r)| r.total > 0);
    if cli.quiet {
        return ExitCode::from(if found { 0 } else { 1 });
    }
    let stdout = io::stdout();
    let mut out = io::BufWriter::new(stdout.lock());
    let mut failed = false;
    if cli.json {
        let mut entries: Vec<FdEntry> = Vec::new();
        let mut warnings = Vec::new();
        let (mut total, mut truncated) = (0, false);
        for (root, r) in &resps {
            failed |= r.error.is_some();
            total += r.total;
            truncated |= r.truncated;
            warnings.extend(r.warnings.iter().cloned());
            entries.extend(r.entries.iter().map(|e| FdEntry {
                path: lancet::walk::display_path(root, &e.path),
                ..e.clone()
            }));
        }
        let errors: Vec<String> = resps
            .iter()
            .filter_map(|(_, r)| r.error.as_ref().map(|e| e.message.clone()))
            .collect();
        let body = serde_json::json!({
            "status": if !errors.is_empty() { "invalid_query" } else if entries.is_empty() { "empty" } else { "complete" },
            "mode": resps.first().map(|(_, r)| r.mode.clone()).unwrap_or_default(),
            "total": total,
            "returned": entries.len(),
            "truncated": truncated,
            "entries": entries,
            "warnings": warnings,
            "error": (!errors.is_empty()).then(|| serde_json::json!({ "message": errors.join("; ") })),
        });
        let _ = serde_json::to_writer(&mut out, &body)
            .map_err(io::Error::other)
            .and_then(|_| writeln!(out));
    } else {
        let style = OutputStyle {
            explicit_root,
            keep_dot_prefix: match cli.strip_cwd_prefix.as_deref() {
                Some("always") => false,
                Some("never") => true,
                _ => cli.print0,
            },
            absolute: cli.absolute_path,
            separator: cli.path_separator.clone(),
            format: cli.format.clone(),
            null: cli.print0,
        };
        for (root, r) in &resps {
            if let Some(err) = &r.error {
                failed = true;
                eprintln!("[lancet fd] error: {}", err.message);
            } else {
                let _ = print_fd_human(r, root, cli.tree, &style, &mut out);
            }
        }
    }
    let _ = out.flush();
    for (_, r) in &resps {
        for w in &r.warnings {
            eprintln!("[lancet fd] warning: {w}");
        }
        if cli.stats {
            eprintln!(
                "[lancet fd] mode={} total={} returned={} elapsed_ms={:.2}",
                r.mode, r.total, r.returned, r.elapsed_ms
            );
        }
    }
    // fd's exit codes: 0 whether or not anything matched, 1 for its own errors.
    ExitCode::from(if failed { 1 } else { 0 })
}
