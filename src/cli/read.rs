//! `lancet read`: command-line arguments and output.

use super::*;

pub(crate) fn main(args: &[OsString]) -> ExitCode {
    run_read(ReadCli::parse_from(args))
}

#[derive(Parser)]
#[command(
    name = "lancet read",
    about = "Read specific lines, AST symbols, PDF/PPTX pages, spreadsheet cells, or archive entries"
)]
pub(crate) struct ReadCli {
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

pub(crate) fn run_read(cli: ReadCli) -> ExitCode {
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
    let code = Status::exit_code;
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
                if cli.budget_tokens.is_some() {
                    eprintln!(
                        "[lancet read] {which}truncated at ~{} tokens (resume with --lines {next}:)",
                        resp.estimated_tokens
                    );
                } else {
                    eprintln!(
                        "[lancet read] {which}showing lines {}-{} of {}: continue with --offset {next}, or `lancet outline {}` to see its declarations",
                        resp.start_line, resp.end_line, resp.total_lines, resp.path
                    );
                }
            }
        }
    }
    ExitCode::from(worst)
}
