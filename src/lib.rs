//! Lancet — surgical code, AST, and document search engine.
//!
//! Layers (see docs/ARCHITECTURE.md):
//! - [`walk`], [`index`], [`fold`]: traversal, the trigram index that only
//!   *narrows* candidate files, case folding.
//! - [`scope`]: which files a command looks at (index + freshness).
//! - commands: [`rg`], [`fd`], [`read_cmd`], [`outline`], [`refs`], [`ast_grep`],
//!   [`edit_cmd`], [`patch`], [`rename`]; every write goes through [`change`].

pub mod adapters;
pub mod ast_grep;
pub mod change;
pub mod code;
pub mod content;
pub mod edit;
pub mod edit_cmd;

pub mod fd;
pub mod fold;
pub mod index;

pub mod mcp;
pub mod outline;
pub mod output;
pub mod patch;
pub mod query;
pub mod read_cmd;
pub mod refs;
pub mod rename;
pub mod rg;
pub mod scope;
pub mod scopes;
pub mod walk;
