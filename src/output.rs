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

impl Status {
    /// The process exit code: 0 done, 1 nothing found, 2 refused or invalid.
    pub fn exit_code(self) -> u8 {
        match self {
            Status::Complete => 0,
            Status::Empty => 1,
            _ => 2,
        }
    }
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

/// Why a command refused, with the status that decides the exit code.
#[derive(Debug)]
pub struct Refusal {
    pub status: Status,
    pub message: String,
}

impl Refusal {
    pub fn new(status: Status, message: impl Into<String>) -> Self {
        Refusal {
            status,
            message: message.into(),
        }
    }
}
