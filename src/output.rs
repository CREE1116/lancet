//! Status, statistics and error types shared by every command.

use serde::Serialize;

#[derive(Serialize, Clone, Copy, PartialEq, Eq, Debug)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    /// Search finished over the requested scope and found results.
    Complete,
    /// Search finished over the requested scope and found nothing.
    Empty,
    InvalidQuery,
    Unsupported,
    /// The file changed since the caller's `rev` was taken; nothing was written.
    Conflict,
}

#[derive(Serialize, Default, Debug, Clone)]
pub struct Stats {
    pub files_considered: usize,
    pub files_searched: usize,
    pub files_matched: usize,
    pub binary_skipped: usize,
    pub read_errors: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub index_candidates: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stale_files: Option<usize>,
    pub elapsed_ms: f64,
}

#[derive(Serialize, Debug, Clone)]
pub struct ErrorInfo {
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pos: Option<usize>,
}
