//! `lancet outline`: command-line arguments and output.

use super::*;

pub(crate) fn main(args: &[OsString]) -> ExitCode {
    run_outline(OutlineCli::parse_from(args))
}

#[derive(Parser)]
#[command(
    name = "lancet outline",
    about = "Extract code symbol skeleton, Markdown headings, spreadsheet sheets, or archive entries"
)]
pub(crate) struct OutlineCli {
    /// Files or directories
    #[arg(default_value = ".")]
    paths: Vec<PathBuf>,
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

pub(crate) fn run_outline(cli: OutlineCli) -> ExitCode {
    init_threads(cli.threads);
    let resp = OutlineResponse::merge(
        cli.paths
            .iter()
            .map(|p| {
                let mut opts = OutlineOptions::new(p);
                opts.max_depth = cli.max_depth;
                opts.exts = cli.exts.clone();
                opts.limit = cli.limit;
                opts.walk = WalkOpts {
                    hidden: cli.hidden,
                    no_ignore: cli.no_ignore,
                    threads: cli.threads.unwrap_or(0),
                    ..WalkOpts::default()
                };
                opts.ocr = cli.ocr;
                build_outline(&opts)
            })
            .collect(),
    );
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
    exit(resp.status)
}
