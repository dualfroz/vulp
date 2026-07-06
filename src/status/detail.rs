//! Re-exports the repo-detail view's entry point under `status::detail` so
//! callers reach it as `crate::status::detail::run`, alongside the rest of
//! the `status` module's repo-summary functions it operates on.

pub use crate::detail::run;
