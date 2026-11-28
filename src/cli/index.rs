//! `lancet index`: command-line arguments and output.

use super::*;

pub(crate) fn main(args: &[OsString]) -> ExitCode {
    run_index(IndexCli::parse_from(args))
}

#[derive(Parser)]
#[command(
    name = "lancet index",
    about = "Build or incrementally update the trigram index"
)]
pub(crate) struct IndexCli {
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
    /// Print the index's state (segments, files, size) as JSON and change nothing
    #[arg(long, conflicts_with_all = ["rebuild", "compact"])]
    status: bool,
    /// Enable external OCR (tesseract/pdftoppm) for images and scanned PDFs
    #[arg(long)]
    ocr: bool,
}

pub(crate) fn run_index(cli: IndexCli) -> ExitCode {
    init_threads(cli.threads);
    let dir = cli
        .index_dir
        .clone()
        .unwrap_or_else(|| lancet::index::default_dir(&cli.path));
    if cli.status {
        return index_status(dir);
    }
    let wopts = WalkOpts {
        hidden: cli.hidden,
        no_ignore: cli.no_ignore,
        threads: cli.threads.unwrap_or(0),
        ..WalkOpts::default()
    };
    let res = if cli.rebuild {
        lancet::index::rebuild_with_ocr(&cli.path, &dir, &wopts, cli.ocr)
    } else if cli.compact {
        match lancet::index::sync_with_ocr(&cli.path, &dir, &wopts, cli.ocr) {
            Ok(mut rep) => match lancet::index::compact(&dir) {
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
        lancet::index::sync_with_ocr(&cli.path, &dir, &wopts, cli.ocr)
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

pub(crate) fn index_status(dir: PathBuf) -> ExitCode {
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
