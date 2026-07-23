# Contributing to vulp

Thanks for looking at vulp. It's a young solo-maintained project, so keep
that in mind — response times may vary, and larger changes are easier to
land if discussed in an issue first.

## Building from source

```bash
git clone https://github.com/dualfroz/fxg.git vulp
cd vulp
cargo build --release
./target/release/vulp --help
```

Requires a recent stable Rust toolchain (edition 2021). `git2` and `keyring`
pull in native dependencies (vendored OpenSSL/libgit2, so no system packages
needed on Linux/macOS/Windows for the build itself).

## Before opening a PR

```bash
cargo build --all-targets
cargo clippy --all-targets -- -D warnings
cargo test
```

All three run in CI (`.github/workflows/ci.yml`) on every PR — a red check
means one of them failed. `cargo clippy` must be warning-free; this project
holds that bar deliberately (see `plan/audit-2026-07-05.md` if you want the
history of how it got there).

## Code layout

- `hub/` — the TUI's tab-based dashboard (rendering, keymap, modals,
  background-job spawning). Business logic (`providers/`, `status/`,
  `config/`, `codeintel/`) is kept separate from rendering (`hub/`,
  `detail/`, `tui/`) — new features should follow that split rather than
  mixing API/git calls into render code.
- `cli_cmds/` — the non-interactive CLI surface (one module per command
  group). Every CLI command should also be reflected in `docs/cli.md`.
- `providers/` — one file per git host (`github.rs`, `gitlab.rs`,
  `gitea.rs`). GitHub is the reference implementation; GitLab/Gitea are
  intentionally more limited (see `docs/cli.md` and `plan/roadmap.md` for
  what's a known gap vs. what's missing).
- Slow operations (API calls, git network ops, shell subprocesses) run on a
  background `std::thread` and report back over an `mpsc` channel polled
  once per frame — see any `spawn_*` function in `hub/modals_spawn.rs` for
  the pattern, including the "don't start a second job on top of a running
  one" guard those functions check first.

## Reporting bugs / proposing features

Open a GitHub issue. For bugs, a repro (even a rough one) beats a long
description — this is a TUI, so exact keypresses + terminal emulator matter.

## What this project won't do

No telemetry, no forced update-nagging beyond an informational tile, no
scope creep into being a general Git GUI — vulp is a workspace-of-many-repos
tool first.
