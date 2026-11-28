//! `lancet rg`: command-line arguments and output.

use super::*;

pub(crate) fn main(args: &[OsString]) -> ExitCode {
    run_rg(parse_rg(args))
}

#[derive(Parser)]
#[command(
    name = "lancet rg",
    args_override_self = true,
    about = "ripgrep-compatible search: same flags, semantics, output and exit codes, on lancet's index (always fresh) and document extractors"
)]
pub(crate) struct RgCli {
    /// PATTERN then PATH... (with -e/-f, every argument is a PATH)
    args: Vec<String>,
    /// Also search the paths listed in FILE, one per line (`-` for stdin)
    #[arg(long = "in", value_name = "INPUTFILE")]
    in_files: Vec<PathBuf>,
    /// Also search the paths listed in FILE, separated by NUL bytes
    #[arg(long = "in0", value_name = "INPUTFILE")]
    in0_files: Vec<PathBuf>,
    /// Where each positional / --in / --in0 argument was on the command line, so
    /// listed paths keep their place among the others (filled after parsing).
    #[arg(skip)]
    arg_order: Vec<(usize, ArgKind, usize)>,
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
    /// Search at most NUM bytes of each file
    #[arg(long, value_name = "NUM")]
    max_bytes: Option<u64>,
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
    #[arg(long, hide = true)]
    no_pre: bool,
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ArgKind {
    Positional,
    In,
    In0,
}

/// Marks a path read from --in / --in0 while it sits among the positional arguments.
pub(crate) const PATH_MARK: &str = "\u{0}in:";

/// `RgCli` plus the command-line position of every positional / --in / --in0 value.
pub(crate) fn parse_rg(args: &[OsString]) -> RgCli {
    use clap::{CommandFactory, FromArgMatches};
    let m = RgCli::command().get_matches_from(args);
    let mut cli = RgCli::from_arg_matches(&m).unwrap_or_else(|e| e.exit());
    for (id, kind) in [
        ("args", ArgKind::Positional),
        ("in_files", ArgKind::In),
        ("in0_files", ArgKind::In0),
    ] {
        if let Some(ix) = m.indices_of(id) {
            cli.arg_order
                .extend(ix.enumerate().map(|(n, i)| (i, kind, n)));
        }
    }
    cli.arg_order.sort_unstable_by_key(|a| a.0);
    cli
}

pub(crate) fn run_rg(cli: RgCli) -> ExitCode {
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
    } else if cli.pre.as_deref().is_some_and(|c| !c.is_empty()) && !cli.no_pre {
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
    // Positional arguments and the paths listed by --in / --in0, in command-line order.
    let mut positional: Vec<String> = Vec::new();
    for &(_, kind, n) in &cli.arg_order {
        match kind {
            ArgKind::Positional => positional.push(cli.args[n].clone()),
            ArgKind::In | ArgKind::In0 => {
                let f = if kind == ArgKind::In {
                    &cli.in_files[n]
                } else {
                    &cli.in0_files[n]
                };
                let text = match read_text_arg(f) {
                    Ok(t) => t,
                    Err(e) => return fail(&format!("{}: {e}", f.display())),
                };
                let sep = if kind == ArgKind::In { '\n' } else { '\0' };
                positional.extend(
                    text.split(sep)
                        .map(|l| l.strip_suffix('\r').unwrap_or(l))
                        .filter(|l| !l.is_empty())
                        .map(|l| format!("{PATH_MARK}{l}")),
                );
            }
        }
    }
    if patterns.is_empty() && cli.pattern_file.is_empty() && !cli.files {
        // The first argument typed on the command line; a listed path is never the pattern.
        match positional.iter().position(|a| !a.starts_with(PATH_MARK)) {
            Some(i) => patterns.push(positional.remove(i)),
            None => return fail("a pattern is required (or use --files / --type-list)"),
        }
    }
    let mut o = RgOptions::new(patterns);
    o.paths = positional
        .into_iter()
        .map(|a| PathBuf::from(a.strip_prefix(PATH_MARK).unwrap_or(&a)))
        .collect();
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
    o.max_bytes = cli.max_bytes;
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
