//! Functions carried over from `vulp`'s pre-TUI CLI command set, kept only
//! where the TUI hub still calls into them directly: onboarding an existing
//! local folder onto a remote (`adopt`) and scanning source for `TODO`/
//! `FIXME`/`HACK` comments (`refactor`).

pub mod adopt;
pub mod refactor;
