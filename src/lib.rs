//! Lancet — surgical code, AST, and document search engine.
//!
//! Layers:
//! - [`query`]: query IR (`Query`/`Expr`/`Matcher`), strict parser, canonical printer.
//! - [`eval`] + [`matcher`]: per-file evaluation of a compiled query (the scan oracle).
//! - [`index`]: optional trigram index that only *narrows* the candidate file set;
//!   every candidate is verified by the same evaluator, so indexed and scanned
//!   results are identical by construction.
//! - [`search`]: executor that plans scan vs index and renders results.

pub mod adapters;
pub mod ast_grep;
pub mod code;
pub mod content;
pub mod edit;
pub mod edit_cmd;
pub mod eval;
pub mod fd;
pub mod fold;
pub mod index;
pub mod matcher;
pub mod mcp;
pub mod outline;
pub mod output;
pub mod patch;
pub mod query;
pub mod read_cmd;
pub mod refs;
pub mod rename;
pub mod rg;
pub mod search;
pub mod walk;
