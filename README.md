# fxg

Multi-repository git workflow manager. Zarządzaj repozytoriami na GitHub, GitLab i Gitea z jednego miejsca.

## Install

```bash
# Linux — opcjonalna zależność systemowa
sudo apt install libgit2-dev   # Ubuntu/Debian
sudo pacman -S libgit2         # Arch

cargo install --path .
fxg --version
```

Binarny plik ląduje w `~/.cargo/bin/fxg` — już jest w PATH po standardowej instalacji Rust. Działa na Windows i Linux.

---

## Pierwsze uruchomienie

```bash
fxg config   # ustaw workspace_root, username, email
fxg auth add --provider github --token ghp_xxxxx
```

Bez tych dwóch kroków większość komend nie zadziała.

---

## `fxg status`

Otwiera TUI — tabelę wszystkich repozytoriów z `workspace_root`:

```
┌ fxg — Workspace Status · 4 repos ──────────────────────────────────────────────┐
│  #   NAME          BRANCH     AHEAD  BEHIND  DIRTY  LAST COMMIT                │
│  1   my-api        main           2       0      ✗   2h ago                     │
│  2   my-frontend   develop        0       3      ✓   yesterday        ← żółty   │
│  3   shared-lib    main           0       0      ✗   5d ago                     │
│  4   infra         feat/k8s       1       0      ✓   30m ago          ← żółty   │
│                                                                                 │
│  [↑/↓] navigate  [Enter] drill-in  [f] filter  [q] quit                        │
└─────────────────────────────────────────────────────────────────────────────────┘
```

Kolory: żółty = dirty (niezcommitowane zmiany), czerwony = behind (trzeba pullować).

**Enter na repozytorium** → widok szczegółowy:

```
┌ my-frontend · develop ─────────────────────────────────────────────────────────┐
│  Branch:  develop                                                               │
│  Status:  +0 ahead  -3 behind  dirty: yes                                      │
│                                                                                 │
│ ┌ Modified files ──────────────────────────┐ ┌ Recent commits ───────────────┐ │
│ │  M  src/components/Nav.tsx               │ │  a3f1c2  fix: nav style  2h   │ │
│ │  M  src/styles/theme.css                 │ │  b89d01  feat: dark mode  1d  │ │
│ │  ?  src/utils/new-helper.ts              │ │  c12345  chore: deps  3d      │ │
│ └──────────────────────────────────────────┘ └───────────────────────────────┘ │
│  [p] pull  [P] push  [c] commit  [b] branches  [q] back                        │
└─────────────────────────────────────────────────────────────────────────────────┘
```

**Filtry (z CLI):**
```bash
fxg status --filter dirty    # tylko repo z niezcommitowanymi zmianami
fxg status --filter behind   # tylko repo, które trzeba pullować
fxg status --filter ahead    # tylko repo gotowe do pusha
fxg status --filter clean    # tylko czyste i zsynchronizowane
fxg status --workspace work  # tylko repo w danym workspace
fxg status my-api            # bezpośrednio widok szczegółowy konkretnego repo
```

---

## `fxg sync`

Bez argumentów otwiera TUI — **3-etapowy flow**:

### Etap 1 — wybór repozytoriów

```
┌ Select repositories (Space = toggle, Enter = confirm) ──────────────────────┐
│  [ ]  my-api          main                                                   │
│  [x]  my-frontend     develop  ✓     ← dirty                                │
│  [ ]  shared-lib      main                                                   │
│  [x]  infra           feat/k8s ✓     ← dirty                                │
│                                                                              │
│  [Space] toggle  [a] select all  [Enter] confirm  [q] quit                  │
└──────────────────────────────────────────────────────────────────────────────┘
```

`Space` = toggle zaznaczenia, `a` = zaznacz wszystkie, `Enter` = przejdź dalej.

### Etap 2 — wybór akcji

```
┌ Action for: my-frontend, infra ────────────────────────────────────────────┐
│  Pull                                                                       │
│  Commit                                                                     │
│  ▶ Push                                                                     │
│  New branch                                                                 │
│  Create PR                                                                  │
│                                                                             │
│  [↑/↓] navigate  [Enter] execute  [Esc] back                               │
└─────────────────────────────────────────────────────────────────────────────┘
```

### Etap 3 — wyniki

```
┌ Progress ──────────────────────────────────────────────────────────────────┐
│  ✓ my-frontend — push — Pushed develop                                     │
│  ✓ infra       — push — Pushed feat/k8s                                    │
│                                                                             │
│  [q] quit                                                                  │
└─────────────────────────────────────────────────────────────────────────────┘
```

### Sync bez TUI (skrypty / CI)

```bash
fxg sync pull --workspace work          # pull wszystkich repo w workspace
fxg sync pull --all                     # pull wszystkich repo w workspace_root

fxg sync commit -m "chore: sync"        # commit wszystkich dirty repo (CWD)
fxg sync commit -m "chore: sync" --all  # commit wszystkich dirty w workspace_root

fxg sync push --all                     # push wszystkich
fxg sync push --all -m "chore: sync"    # najpierw commit dirty, potem push
fxg sync push --all --force-with-lease  # force push (bezpieczny)
```

> **Uwaga:** `commit` używa tej samej wiadomości dla wszystkich repo. Indywidualne wiadomości per-repo możliwe przez TUI (opcja Commit) — TUI pyta o wiadomość przed wykonaniem commita.

---

## `fxg init`

Bez flagi `--provider` uruchamia **wizard 7-krokowy**:

```
══════════════════════════════════════════
  fxg — New Repository
══════════════════════════════════════════

Step 1/7 · Repository name: my-new-project
  > walidacja: brak spacji, niepuste

Step 2/7 · Provider & Account:
  > github/personal   ← domyślny (zaznaczony)
    gitlab/work

Step 3/7 · Visibility:
  > private
    public
    internal

Step 4/7 · Tags (comma-separated):
  > rust, cli, tools

Step 5/7 · Initial files:      ← MultiSelect, domyślnie 4 pierwsze zaznaczone
  [x] README.md
  [x] .gitignore
  [x] CHANGELOG.md
  [x] .env.example
  [ ] LICENSE          ← opcja dostępna ale plik NIE jest generowany (TODO)
  [ ] CONTRIBUTING.md  ← j.w.

  Template:
  > node
    rust               ← dla Rusta generuje /target\nCargo.lock\n
    python
    blank

Step 6/7 · Extra branches (comma-separated):
  > dev                ← domyślnie

Step 7/7 · Add collaborators? (y/N)
  > username: alice
    role: write
  > Add another? N

──────────────────────────────────────────
Proceed with creation? (Y/n)
```

**Co się dzieje po potwierdzeniu:**

```
  ✓ Created remote repo my-new-project (github/personal)
  ✓ Applied tags: rust, cli, tools
  ✓ mkdir ~/projects/my-new-project
  ✓ git init
  ✓ Created initial files
  ✓ Initial commit: "chore: init"
  ✓ git remote add origin git@github.com:user/my-new-project.git
  ✓ git push -u origin main
  ✓ Created branch: dev
  ✓ Added collaborator: alice (write)

  Done in 2.3s · https://github.com/user/my-new-project
```

**Tryb bez wizarda** (name + provider podane → pomija wizard):
```bash
fxg init my-project --provider github
fxg init my-project --provider github --status public --tags rust,cli --branch develop
```

**Batch** — wiele repo naraz:
```bash
fxg init --batch "api,frontend,shared-lib"      # lista inline
fxg init --batch repos.txt                       # plik, jedno repo per linia
```
W trybie batch: tworzy README, .gitignore, bez CHANGELOG i .env.example.

---

## `fxg workspace`

Grupy repozytoriów do zbiorczych operacji.

```bash
fxg workspace create backend
fxg workspace add backend api shared-lib
fxg workspace list
fxg workspace remove backend
```

```
Workspaces:
  backend   (2 repos)   api, shared-lib
  frontend  (1 repo)    my-frontend
```

Workspace używasz potem w innych komendach: `--workspace backend`.

---

## `fxg auth`

```bash
fxg auth                                                   # TUI z listą kont
fxg auth add --provider github --token ghp_xxx             # dodaj konto
fxg auth add --provider gitlab --token glpat_xxx
fxg auth add --provider gitea  --token xxx --url https://git.example.com
fxg auth add --provider github --token ghp_xxx --alias work  # nazwany alias
fxg auth remove github/personal
fxg auth switch github/work       # ustaw jako domyślne
fxg auth test                     # sprawdź domyślne konto
fxg auth test github/work
```

ID konta: `<provider>/<alias>`, np. `github/personal`, `gitea/selfhosted`.
Tokeny przechowywane w systemowym keychaine (Windows Credential Manager / macOS Keychain / Linux Secret Service) — nigdy w plikach plaintext.

---

## `fxg config`

Interaktywne TUI do edycji ustawień globalnych. Zapisywane do `~/.fxg/config.toml`.

```bash
fxg config
```

Plik konfiguracyjny (`~/.fxg/config.toml`):

```toml
workspace_root      = "/home/user/projects"
data_dir            = "/home/user/.fxg"
default_provider    = "github"
default_branch      = "main"
default_visibility  = "private"
editor              = "vim"
username            = "your-name"
email               = "you@example.com"
gitignore_template  = "node"
readme_template     = "heartbyte-default"
share_stats         = false
timezone            = "Europe/Warsaw"
vsc_data_dir        = "/home/user/.fxg/vsc-data"
```

| Pole | Domyślnie | Opis |
|---|---|---|
| `workspace_root` | `~/projects` | Gdzie fxg szuka lokalnych repozytoriów |
| `data_dir` | `~/.fxg` | Dane fxg (konta, workspace'y) |
| `default_provider` | `github` | Provider gdy nie podasz `--provider` |
| `default_branch` | `main` | Domyślna gałąź nowych repo |
| `default_visibility` | `private` | Widoczność nowych repo |
| `editor` | `$EDITOR` lub `vim` | Edytor w interaktywnych flowach |
| `username` | _(puste)_ | Git username — używany w commitach |
| `email` | _(puste)_ | Git email — używany w commitach |
| `gitignore_template` | `node` | Template `.gitignore` (`node`, `rust`, `python`) |
| `timezone` | `Europe/Warsaw` | Strefa czasowa w statystykach |
| `vsc_data_dir` | `~/.fxg/vsc-data` | Dane VSCode time tracking |

---

## Pliki danych

```
~/.fxg/
├── config.toml        # ustawienia globalne
├── accounts.toml      # konta (bez tokenów — te są w keychaine)
├── workspaces.toml    # definicje workspace'ów
└── vsc-data/          # dane śledzenia czasu z VSCode (dla fxg stats time)
```

---

## Providery

| Provider | wartość `--provider` | Uwagi |
|---|---|---|
| GitHub | `github` | github.com |
| GitLab | `gitlab` | gitlab.com |
| Gitea | `gitea` | self-hosted, wymaga `--url` |
| Forgejo | `forgejo` | fork Gitea, wymaga `--url` |
