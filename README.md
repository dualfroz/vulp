# vulp

vulp is a git manager for a workspace that holds many repositories. It scans
a workspace folder, shows the state of every repository (branch, uncommitted
changes, commits ahead of or behind the remote, language, license, cached
provider metadata) and runs the same operation across many of them at once.
It has two front ends over the same code: a terminal UI for people, and a
non-interactive CLI with `--json` output and stable exit codes for scripts
and AI agents. It talks to GitHub, GitLab and Gitea/Forgejo, and works
offline for everything that only needs local git.

## Features

**Workspace overview**

- Recursive scan of `workspace_root` (up to 5 levels deep, skipping
  `node_modules`, `target`, `vendor`, `dist`, `build`, virtualenvs and similar),
  streamed to the UI while it runs.
- Per repository: branch and local branch count, dirty state, ahead/behind
  against `origin/<branch>`, last commit and its age, latest tag, stash count,
  dominant languages by file count (assets only counted when they dominate),
  license file detection, owner and remote repository name parsed from the
  `origin` URL.
- Cached provider data after a refresh: stars, open PR and issue counts, CI
  status of the current branch, open Dependabot alerts.
- Display labels, bookmarks, pins, and projects (named, coloured groups of
  repositories) used for filtering and batch operations.

**Git operations**

- Batch `pull`, `push` and `commit + push` over a list of repositories, a
  project, or the whole workspace. A failure in one repository does not stop
  the others; the exit code reports it.
- Release: annotated tag, tag push, and a provider release when an account
  is configured.
- New repositories: create on the provider from a template (`templates.toml`),
  clone from a URL, link or adopt an existing local folder, publish an
  existing local repository.
- In the repository detail view: commit history, branches (create, switch,
  delete), file browser with preview, commit, tag, and remote
  settings (description, topics, visibility, collaborators, rename, archive,
  delete).
- `LICENSE` generation (MIT or Apache-2.0) for repositories without one. It
  never overwrites an existing license and never commits.

**Safety**

- Secret scan before every commit made through vulp: the staged diff is
  checked for GitHub and GitLab tokens, AWS access keys, `DISCORD_TOKEN`
  assignments and private key blocks. A hit blocks the commit and unstages the
  changes; the matched value is never printed. `--allow-secrets` overrides it.
- `protected_repos`: mutating commands refuse to touch listed repositories
  unless `--force` is passed.
- `--dry-run` on every mutating command prints what would happen and changes
  nothing.
- Every action vulp performs is appended to a local activity log. With
  `VULP_AGENT` set, entries are marked as done by an agent.

**Deploy drift**

- If a deploy host mirrors the workspace layout under `deploy_root`, one
  batched remote command collects the deployed HEAD of every mirrored
  repository and each one is reported as `in_sync`, `behind:N` or `diverged`
  against local HEAD. Repositories without a mirror are simply not reported.

**Statistics and digest**

- Commit statistics per repository for 1d, 7d, 30d, 90d, 1y or all time.
- Author-filtered analytics that only count your own commits (configured
  emails, `git config user.email`, and GitHub no-reply addresses of your
  accounts): streaks, weekly digest, and classification of vendored clones
  that contain none of your commits.
- Weekly digest (own commits vs. last week, active days, streak, top
  repositories, tags, PR and issue counts, todos), archived as Markdown under
  `~/.vulp/digests/` and optionally posted to a Discord webhook.

**Todos**

- Cross-repository todo list with statuses, projects and per-repository
  assignment, plus an importer that scans source files for `TODO`, `FIXME` and
  `HACK` markers.

**Code intelligence (tree-sitter)**

- For Rust, Python, JavaScript, TypeScript/TSX and Go: list functions (with
  arguments, length and reference counts), find usages of an identifier,
  rename an identifier across a repository, and convert function names to
  `snake_case`, `camelCase` or `PascalCase`. Renames only touch identifier
  nodes, never strings or comments, and are shown as a dry run before they
  are applied.

**Settings profiles**

- Settings snapshots can be saved as named profiles and synced through a
  private `vulp-settings` repository on your provider account.

## Install

Prebuilt binaries are published for Linux x86_64 and Windows x86_64.

```bash
curl -fsSL https://raw.githubusercontent.com/dualfroz/vulp/main/install/vulp-installer.sh | bash
```

```powershell
irm https://raw.githubusercontent.com/dualfroz/vulp/main/install/vulp-installer.ps1 | iex
```

Both installers honour `VULP_REPO` and `VULP_INSTALL_DIR`.

## Build and run

Requires a stable Rust toolchain (edition 2021) and `git` on `PATH`. OpenSSL
and libgit2 are vendored, so no system libraries are needed.

```bash
cargo build --release
./target/release/vulp --help
cargo install --path .
```

First run:

```bash
vulp config        # interactive settings: workspace_root, username, email, ...
vulp auth import   # import the account and token from an authenticated gh CLI
# or: vulp auth add --provider github --token <token> [--alias work] [--url https://git.example.com]
vulp               # open the TUI
```

## Terminal UI

`vulp` without arguments opens the hub. Tabs: Overview, Repos, Projects,
Todos, PRs, Issues, Stats, Activity, Settings (switch with `Tab`, the arrow
keys or `1`-`9`). `m` opens the action menu for the current tab and selection,
`?` lists every key binding, `Enter` opens the repository or project detail
view. `Ctrl+Z` suspends to the shell and restores the screen on resume.

## CLI

Every hub operation has a non-interactive counterpart. A few examples:

```bash
vulp list --dirty --json
vulp status my-repo
vulp brief --json
vulp sync --project backend --action commit-push -m "chore: update dependencies"
vulp release my-repo --tag v1.2.0 --notes "Bug fixes"
vulp todo add "Write migration guide" && vulp todo done 3
vulp project assign backend api worker
vulp repo license --all --kind mit
vulp stats --period 7d --json
vulp refresh
```

Contract: results on stdout, progress and errors on stderr, no prompts. Exit
codes are `0` success, `1` execution error (including a protected repository
or a failed sync), `2` bad usage, `3` missing or invalid provider account,
`4` repository, todo or project not found. The full command reference, JSON
shapes and recipes for agents are in [docs/cli.md](docs/cli.md).

## Configuration

`vulp config` edits `~/.vulp/config.toml`. The most relevant keys:

| Key | Default | Meaning |
|---|---|---|
| `workspace_root` | `~/projects` | folder that is scanned for repositories |
| `default_provider` | `github` | provider used when none is given |
| `default_branch`, `default_visibility` | `main`, `private` | defaults for new repositories |
| `editor` | `$EDITOR`, else `vim` | used by "open in editor" (may include flags) |
| `clipboard_cmd` | first of `wl-copy`, `xclip`, `pbcopy` | command used for copy actions |
| `protected_repos` | `[]` | repositories that need `--force` |
| `license_holder`, `default_license` | git `user.name`, `mit` | `LICENSE` generation |
| `stats_author_emails` | git `user.email` | identities counted as yours |
| `deploy_ssh_cmd`, `deploy_root` | empty | deploy drift (both required) |
| `hide_external_clones` | `false` | hide clones with none of your commits |

Data files live in `~/.vulp/`: `config.toml`, `accounts.toml`, `.tokens`
(provider tokens and the digest webhook, mode 0600; the OS keyring is used as
a secondary store), `projects.toml`, `todos.toml`, `labels.toml`,
`bookmarks.toml`, `pins.toml`, `templates.toml`, `meta_cache.toml`,
`stats_cache.toml`, `activity.log` (JSON lines) and `digests/`.

## Providers

| Provider | `--provider` | Supported |
|---|---|---|
| GitHub | `github` | everything, including PR and issue lists, CI status, Dependabot alerts and search counts for the digest |
| GitLab | `gitlab` | create, delete, rename, archive, visibility, topics, branches, PR creation, reviewers, labels, collaborators, releases, metadata |
| Gitea / Forgejo | `gitea` | same as GitLab; needs `--url` |

## Tests

```bash
cargo test
cargo clippy --all-targets -- -D warnings
```

Unit tests sit next to the code they cover: secret detection, deploy drift
against real repositories, workspace scanning and repository summaries,
language detection, remote URL parsing, configuration parsing, CLI argument
parsing, license generation, own-commit analytics and the tree-sitter code
intelligence. The integration tests in `tests/cli/` run the compiled binary
against temporary git repositories and local bare remotes with `HOME` pointed
at a scratch directory, and check `--json` output and exit codes for listing,
status, sync (pull, push, commit + push, secret blocking, dry run, protected
repositories, partial failure), releases, todos, projects, repository
utilities, license generation, cached PR and issue lists, the digest, and the
error paths of provider commands. They need `git` on `PATH` and run on Unix.

## Project layout

```
src/
  main.rs, app.rs, cli.rs   entry point, dispatch, clap definitions
  cli_cmds/                 non-interactive commands (workspace, items, remote, digest)
  hub/                      TUI hub: state, tabs, key handling, modals, background jobs
  detail/                   repository detail view
  tui/                      terminal setup and Ctrl+Z handling
  status/                   workspace scan and per-repository summary
  providers/                GitHub, GitLab and Gitea API clients
  auth/                     accounts and token storage
  config/                   settings file and settings UI
  codeintel/                tree-sitter function listing, usages and renames
  stats/                    own-commit analytics, size history, release cadence
  deploy.rs                 deploy drift
  secrets.rs                staged-diff secret scan
  digest.rs                 weekly digest
  license/                  LICENSE templates
  activity/, cache.rs, todos/, projects/, labels/, workspace/, profile/
tests/cli/                  integration tests
install/                    installer scripts
docs/                       CLI reference and user guides
```

## Limitations

- Prebuilt binaries exist only for Linux x86_64 and Windows x86_64; other
  platforms build from source.
- PR and issue lists, CI status, Dependabot alerts and digest search counts
  are implemented for GitHub only. The CLI `pr list` and `issue list` read the
  cache written by `vulp refresh`.
- Ahead/behind uses the local `origin/<branch>` ref, so it is only as fresh
  as the last fetch or refresh.
- Deploy drift assumes the deploy host mirrors the workspace layout and that
  `deploy_ssh_cmd` runs its last argument as a shell command there.
- Digest delivery supports Discord webhooks only.
- Code intelligence covers Rust, Python, JavaScript, TypeScript and Go.

## License

MIT, see [LICENSE](LICENSE).
