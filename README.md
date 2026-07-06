# Vulp

Multi-repository git workflow manager. Jeden terminal, cały workspace:
stan wszystkich repozytoriów, batch sync, projekty, todos, PR-y/issues,
statystyki — plus pełne, bezpromptowe CLI, którego mogą używać skrypty
i asystenci AI.

- **TUI** (`vulp` bez argumentów) — interfejs dla ludzi
- **CLI** (`vulp <komenda>`) — dla ludzi, skryptów i AI; `--json`, kody
  wyjścia, zero interaktywnych pytań → pełna referencja: [docs/cli.md](docs/cli.md)

## Instalacja

Gotowa binarka (Linux/Windows), bez Rusta:

```bash
# Linux
curl -fsSL https://raw.githubusercontent.com/dualfroz/fxg/main/install/vulp-installer.sh | bash
```

```powershell
# Windows
irm https://raw.githubusercontent.com/dualfroz/fxg/main/install/vulp-installer.ps1 | iex
```

Albo ze źródeł: `cargo install --path .`

## Pierwsze uruchomienie

```bash
vulp config        # ustaw workspace_root, username, email
vulp auth import   # zaimportuj konto z gh CLI (najprościej)
# albo: vulp auth add --provider github --token ghp_xxx
vulp               # odpal TUI
```

## TUI

`vulp` otwiera hub z zakładkami (przełączanie: `Tab`/`←→` albo `1-9`):

| # | Zakładka | Co robi |
|---|----------|---------|
| 1 | **Overview** | bento-dashboard: statystyki workspace, projekty, top repo po commitach, scalony feed aktywności (commity + akcje vulpa), todos/PRs/issues |
| 2 | **Repos** | tabela wszystkich rep: branch, dirty/ahead/behind, język, gwiazdki; multi-select do operacji hurtowych |
| 3 | **Projects** | grupy repozytoriów (kolor + przypisania) |
| 4 | **Todos** | zadania ręczne + zaimportowane skanem TODO/FIXME z kodu |
| 5/6 | **PRs / Issues** | otwarte PR-y i issues ze wszystkich rep (cache, odświeżane `[R]`) |
| 7 | **Stats** | statystyki commitów per okres (1d–all) |
| 8 | **Activity** | feed: commity + akcje wykonane vulpem |
| 9 | **Settings** | edytor, schowek, workspace, konta, szablony repo, bulk import |

Najważniejsze klawisze (pełna lista: `?` w aplikacji):

- **`m` — kontekstowe menu akcji.** Na każdej zakładce pokazuje wszystkie
  dostępne akcje z ich skrótami — nie musisz nic pamiętać.
- `Enter` — szczegóły repo/projektu (historia, branch'e, pliki, diff,
  ustawienia zdalne, akcje: sync/release/collaboratorzy…)
- `Space` zaznacz do batcha · `s` sync (pull/commit+push) · `r` release
- `/` filtr · `S` sort i filtry · `d/b/f` tylko dirty/behind/ahead
- `N` nowe repo (clone/create/link) · `R` odśwież metadane z API
- `o` otwórz w edytorze · `W` w przeglądarce · `y`/`u` kopiuj ścieżkę/URL

## CLI (skrypty + AI)

Każda operacja TUI ma odpowiednik w CLI. Przykłady:

```bash
vulp list --dirty --json          # co wymaga uwagi (JSON dla AI)
vulp status <repo>                # szczegóły jednego repo
vulp sync --project X --action commit-push -m "chore: update"
vulp release <repo> --tag v1.2.0
vulp todo add "tytuł" && vulp todo done 3
vulp pr create --repo R --title T --head branch
vulp repo collab <repo> <user> --role push
vulp init nazwa --private         # git init + repo na providerze + push
vulp stats --period 7d --json
vulp refresh                      # dociągnij meta/PR/issues z API
```

Kontrakt: `--json` na komendach odczytu, stdout = wynik / stderr = postęp,
kody wyjścia `0` ok · `1` błąd · `2` złe użycie · `3` auth · `4` nie znaleziono.
**Szczegóły i przepisy: [docs/cli.md](docs/cli.md).**

## Konfiguracja

`vulp config` (interaktywnie) albo edycja `~/.vulp/config.toml`:

| Pole | Domyślnie | Opis |
|---|---|---|
| `workspace_root` | `~/projects` | katalog skanowany w poszukiwaniu rep |
| `workspace_groups` | `[]` | foldery-grupy, których dzieci są repami (np. `["clients", "opensource"]`) |
| `data_dir` | `~/.vulp` | dane vulpa |
| `default_provider` | `github` | provider bez `--provider` |
| `default_branch` / `default_visibility` | `main` / `private` | dla nowych rep |
| `editor` | `$EDITOR`/`vim` | do `o` / otwierania plików |
| `clipboard_cmd` | auto | komenda kopiowania (`wl-copy`, `xclip …`) |
| `username` / `email` | — | tożsamość gita |

## Pliki danych

```
~/.vulp/
├── config.toml      # ustawienia
├── accounts.toml    # konta (tokeny w keyringu / .tokens 0600)
├── projects.toml    # projekty (grupy rep)
├── todos.toml       # zadania
├── labels.toml      # etykiety wyświetlania rep
├── bookmarks.toml / pins.toml
├── templates.toml   # szablony nowych rep
├── meta_cache.toml  # cache metadanych/PR/issues z API
└── activity.log     # log akcji vulpa (JSONL)
```

## Providery

| Provider | `--provider` | Zakres wsparcia |
|---|---|---|
| GitHub | `github` | pełny (repo, PR, issues, release, collaboratorzy, search) |
| GitLab | `gitlab` | podstawowy (tworzenie rep, collaboratorzy) |
| Gitea/Forgejo | `gitea` | podstawowy (tworzenie rep, collaboratorzy; wymaga `--url`) |

Listy PR/issues są dziś GitHub-only — na innych providerach zakładki 5/6
będą puste (known limitation).

## Licencja

MIT — patrz [LICENSE](LICENSE).
