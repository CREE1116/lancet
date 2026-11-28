//! `lancet refs`: command-line arguments and output.

use super::*;

pub(crate) fn main(args: &[OsString]) -> ExitCode {
    run_refs(RefsCli::parse_from(args))
}

pub(crate) fn run_rank(opts: &RefsOptions, kinds: &[String], json: bool) -> ExitCode {
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
    exit(resp.status)
}

#[derive(Parser)]
#[command(
    name = "lancet refs",
    about = "Find symbol definitions and lexical AST references (excluding comments & strings)",
    after_help = "Rank declarations by how often they are used:\n  lancet refs --rank '*Service' --kind class src     # the most used *Service classes\n  lancet refs --rank '*' --kind function --limit 10 ."
)]
pub(crate) struct RefsCli {
    /// Exact identifier / symbol name (with --rank: a glob such as `*Service`)
    symbol: String,
    /// List the declarations whose name matches SYMBOL (a glob), most referenced first
    #[arg(long)]
    rank: bool,
    /// With --rank: only these declaration kinds, as the language names them
    /// (class, interface, struct, enum, trait, function, method; Go: type, function, method)
    #[arg(long = "kind", value_delimiter = ',', requires = "rank")]
    kinds: Vec<String>,
    /// Directories or files to search
    #[arg(default_value = ".", value_name = "PATH")]
    roots: Vec<PathBuf>,
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

pub(crate) fn run_refs(cli: RefsCli) -> ExitCode {
    init_threads(cli.threads);
    let mut opts = RefsOptions::new(&cli.symbol, ".");
    opts.roots = cli.roots;
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
    exit(resp.status)
}
