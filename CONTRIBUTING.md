# Contributing to vulp

vulp is maintained by one person, so response times vary and larger changes
are easier to land if they are discussed in an issue first.

## Building from source

```bash
git clone https://github.com/dualfroz/vulp.git
cd vulp
cargo build --release
./target/release/vulp --help
```

Requires a recent stable Rust toolchain (edition 2021). `git2` is built with
vendored OpenSSL and libgit2, so no system packages are needed for the build
on Linux, macOS or Windows.

## Before opening a PR

```bash
cargo build --all-targets
cargo clippy --all-targets -- -D warnings
cargo test
```

All three run in CI (`.github/workflows/ci.yml`) on every PR. `cargo clippy`
must be warning-free.

Unit tests live next to the code they cover (`#[cfg(test)] mod tests`).
Integration tests in `tests/` run the compiled `vulp` binary against
temporary git repositories and a local bare remote, with `HOME` pointed at a
scratch directory so they never touch your real `~/.vulp`. They need the
`git` binary on `PATH` and run on Unix-like systems only.

## Code layout

- `hub/` - the TUI dashboard (rendering, keymap, modals, background jobs).
  Business logic (`providers/`, `status/`, `config/`, `codeintel/`) is kept
  separate from rendering (`hub/`, `detail/`, `tui/`); new features should
  follow that split instead of mixing API or git calls into render code.
- `cli_cmds/` - the non-interactive CLI surface, one module per command
  group. Every CLI command should also be documented in `docs/cli.md`.
- `providers/` - one file per git host (`github.rs`, `gitlab.rs`,
  `gitea.rs`). GitHub is the reference implementation; GitLab and Gitea are
  more limited (see the provider table in `README.md`).
- Slow operations (API calls, git network operations, subprocesses) run on a
  background `std::thread` and report back over an `mpsc` channel polled once
  per frame. See any `spawn_*` function in `hub/modals_spawn.rs` for the
  pattern, including the guard against starting a second job on top of a
  running one.

## Reporting bugs and proposing features

Open a GitHub issue. For bugs, a rough repro beats a long description; since
this is a TUI, the exact keypresses and terminal emulator matter.

## Out of scope

No telemetry, no update nagging beyond an informational tile, and no growth
into a general Git GUI. vulp is a tool for workspaces with many repositories.
