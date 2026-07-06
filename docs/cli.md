# vulp CLI — reference

Complete non-interactive surface. Written for machine consumers (AI agents,
scripts) as much as humans: every read command supports `--json`, nothing
ever prompts, and exit codes are meaningful.

```
vulp                    # no args → interactive TUI (humans)
vulp <command> [--json] # non-interactive (scripts/AI)
```

## Contract

- **`--json`** (global flag): machine-readable JSON on stdout. Available on:
  `list`, `status`, `scan`, `stats`, `activity`, `todo list`, `pr list`,
  `issue list`, `project list`, `accounts`, `refresh`, `repo license`.
- **No prompts, ever.** Missing input = error + non-zero exit.
- **stdout** = results only. **stderr** = progress and errors.
- **Exit codes:** `0` ok · `1` execution error · `2` bad usage ·
  `3` auth missing/invalid · `4` not found (repo/todo/project).
- Repo arguments match the display label **or** the directory basename,
  case-insensitively (e.g. `vulp status vulp` matches "Vulp - Open Source!").
- Mutating commands write to the same `~/.vulp/activity.log` as the TUI,
  so CLI actions appear in the TUI's Overview feed.

## Guardrails (for AI agents especially)

- **`--dry-run`** (global flag): mutating commands (`sync`, `release`,
  `init`, `repo collab`, `repo license`) print `DRY RUN: would ...` lines
  and exit 0 without changing anything. Read commands ignore it.
- **`protected_repos`** (config, comma-separated in `vulp config`): mutating
  commands refuse to touch listed repos (exit 1) unless **`--force`** is
  passed. Matching is by display label or dir basename, case-insensitive.
  The protection check runs before `--dry-run`, so a dry run also reveals it.
- **Secret scan**: `sync --action commit-push` refuses to commit staged
  changes that look like credentials (see Git operations); override with
  `--allow-secrets`.
- **`VULP_AGENT`** (env var, any non-empty value): every activity-log entry
  written while it's set gets a `· via agent` marker in its detail, so human
  and agent actions are distinguishable in `vulp activity` and the TUI feed.
  AI agents driving vulp should always set it.

## Workspace (local, no network)

```
vulp scan                                # repo count + dirty/behind/ahead summary
vulp list [--dirty] [--behind] [--ahead] [--lang ts] [--project NAME]
vulp status <repo>                       # branch, dirty, ahead/behind, last commit...
vulp stats [--period 1d|7d|30d|90d|1y|all]   # commits per repo in period
vulp activity [--limit N] [--kind push|pull|release|"new repo"|todo|pr|issue|commit|license]
```

`vulp list --json` returns an array of repo objects:
```json
[{ "name": "...", "path": "...", "branch": "main", "ahead": 0, "behind": 0,
   "is_dirty": true, "last_commit_msg": "...", "last_commit_age": "2h ago",
   "last_commit_ts": 1780000000, "last_tag": null, "visibility": null,
   "owner": "dualfroz", "has_license": false, "lang": "TypeScript 96%",
   "bookmarked": false, "stars": null, "open_issues": null, "open_prs": null }]
```

## Git operations

```
vulp sync <repo>... [--action pull|push|commit-push] [-m MSG]
vulp sync --all --action pull
vulp sync --project NAME --action push
vulp release <repo> --tag v1.2.0 [--notes "..."]   # git tag + push + provider release
```

`commit-push` requires `-m/--message`. `sync` exits 1 if any target failed
(per-repo progress on stderr).

Before every `commit-push` commit, the staged diff is scanned for secret
shapes (GitHub/GitLab tokens, AWS access keys, Discord token assignments,
private key blocks). A hit blocks that repo's commit and unstages the
changes; pass `--allow-secrets` to override. The same scan guards the TUI
sync flow. Matched values are never echoed back.

## Todos

```
vulp todo list [--status open|in-progress|done|cancelled] [--json]
vulp todo add "Title" [--desc "..."] [--project NAME]
vulp todo start|done|cancel|rm <id>
```

## Projects (repo groupings)

```
vulp project list [--json]
vulp project create NAME [--color "#e74c3c"]
vulp project assign NAME <repo>...
vulp project rm NAME
```

## Provider API (needs `vulp auth import` first)

```
vulp pr list [--repo R] [--json]        # cached — run `vulp refresh` for fresh
vulp pr create --repo R --title T --head BRANCH [--base main]
vulp issue list [--repo R] [--json]
vulp issue create --repo R --title T [--body "..."]
vulp refresh                            # re-fetch meta + PRs + issues into cache
vulp init NAME [--private] [--description "..."]   # local init + create remote + push
vulp accounts [--json]
```

`pr/issue list` read the on-disk cache (`~/.vulp/`) — the same one the TUI
uses; run `vulp refresh` first when freshness matters.

## Per-repo utilities

```
vulp repo url <repo>                    # print origin URL
vulp repo path <repo>                   # print absolute path
vulp repo open <repo>                   # open provider page in browser
vulp repo label <repo> "Display Name"   # empty name clears the label
vulp repo bookmark <repo>               # toggle
vulp repo pin <repo>                    # toggle
vulp repo collab <repo> <user> [--role pull|push|admin]
vulp repo license <repo>... [--kind mit|apache-2.0] [--holder NAME]
vulp repo license --all                 # every repo missing a license
```

`repo license` writes a `LICENSE` file into each target repo. It never
overwrites an existing license (reported as skipped) and never commits —
review and commit the file yourself. Defaults come from config:
`default_license` (mit) and `license_holder` (falls back to
`git config user.name`, then the account username).

## Recipes for AI agents

```bash
# What needs attention right now?
vulp list --dirty --json

# Commit and push everything in one project
vulp sync --project client-panel --action commit-push -m "chore: batch update"

# Fresh cross-repo picture (meta + PRs + issues), then read it
vulp refresh --json && vulp pr list --json

# Weekly activity summary
vulp stats --period 7d --json
```
