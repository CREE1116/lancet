//! `lancet ast`: command-line arguments and output.

use super::*;

pub(crate) fn main(args: &[OsString]) -> ExitCode {
    run_ast(AstCli::parse_from(args))
}

#[derive(Parser)]
#[command(
    name = "lancet ast",
    about = "Tree-sitter structural AST pattern search & safe rewrite"
)]
pub(crate) struct AstCli {
    /// AST pattern with metavariables ($VAR, $_, $$$ARGS)
    pattern: String,
    /// Directories or files to search
    #[arg(default_value = ".", value_name = "PATH")]
    roots: Vec<PathBuf>,
    /// Source language: rs, py, js, ts, tsx, go, c, cpp, zig, java, cs, kt, scala, swift, rb, php, sh, lua, hcl, html, css, json, yaml, toml
    #[arg(short = 'l', long)]
    lang: Option<String>,
    /// Rewrite matched AST nodes using captured metavariables (dry-run unified diff by default)
    #[arg(short = 'r', long)]
    rewrite: Option<String>,
    /// Write --rewrite changes to disk: every file or none
    #[arg(long)]
    apply: bool,
    /// Rewrite even if it adds Tree-sitter syntax errors
    #[arg(long)]
    allow_syntax_errors: bool,
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

pub(crate) fn run_ast(cli: AstCli) -> ExitCode {
    init_threads(cli.threads);
    let mut opts = AstOptions::new(&cli.pattern, ".");
    opts.roots = cli.roots;
    opts.lang = cli.lang;
    opts.rewrite = cli.rewrite;
    opts.apply = cli.apply;
    opts.allow_syntax_errors = cli.allow_syntax_errors;
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
    exit(resp.status)
}
