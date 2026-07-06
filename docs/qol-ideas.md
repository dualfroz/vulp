# vulp — pomysły na QoL (Quality of Life)

> Dokument roboczy. Każdy pomysł ma tytuł, opis, przykład użycia i szacunkową trudność implementacji.

---

## Wysoki priorytet

### 1. `vulp clone` — pobierz wszystkie repozytoria jedną komendą

**Problem:** Na nowym laptopie masz 50 repo na GitHubie. Musisz je klonować jedno po drugim. Żmudne.

**Rozwiązanie:** Komenda pobierająca listę repo z API i klonująca je do `workspace_root`.

```bash
vulp clone --mine            # repo których jesteś właścicielem
vulp clone --contributed     # repo w których robiłeś commity (contributor)
vulp clone --org mojaFirma   # wszystkie repo organizacji
vulp clone --starred         # repo które oznaczyłeś gwiazdką

# z filtrowaniem
vulp clone --mine --filter "frontend"    # tylko repo z "frontend" w nazwie
vulp clone --mine --limit 20            # max 20 repo
```

**Implementacja:** Wywołać `provider.list_repos()` (nowa metoda w trait `Provider`), przefiltrować, wywołać `git clone` dla każdego. Wyświetlić progress w TUI podobny do `sync`.

**Trudność:** średnia — wymaga nowej metody w każdym providerze (GitHub/GitLab/Gitea API wszystkie to obsługują).

---

### 2. Tagi jako organizacja folderów lokalnych

**Problem:** Masz 50 repo i wszystkie są w jednym `workspace_root/`. Chaos.

**Rozwiązanie:** Tag przypisany do repo = podfolder w `workspace_root`. repo z tagiem `dev` ląduje w `workspace_root/dev/`, repo z tagiem `client-acme` ląduje w `workspace_root/client-acme/`.

```bash
# przy tworzeniu repo
vulp init moj-projekt --tags dev,frontend

# retroaktywnie
vulp tag moj-projekt add client-acme
vulp tag moj-projekt remove dev

# reorganizacja lokalna — przenieś foldery według tagów
vulp workspace organize        # pyta o potwierdzenie przed każdym przeniesieniem
vulp workspace organize --dry-run   # pokaż co by się przeniosło, bez wykonania
```

**Wynikowa struktura:**
```
~/projekty/
├── dev/
│   ├── api-backend/
│   └── shared-utils/
├── client-acme/
│   ├── acme-website/
│   └── acme-mobile/
└── archived/
    └── stary-projekt/
```

**Implementacja:** Przechowywać tagi per-repo w `workspaces.toml` lub osobnym `tags.toml`. Komenda `organize` tworzy podfoldery i przenosi repo (lub tworzy symlinki — symlinki są bezpieczniejsze, bo oryginał zostaje w miejscu).

**Trudność:** średnia — logika przenoszenia + aktualizacja `workspace_root` per-repo.

---

### 3. Domyślna paczka plików przy `vulp init`

**Problem:** Przy tworzeniu repo trzeba pamiętać o zaznaczeniu README, CHANGELOG, .env.example w wizardzie. Zawsze się o czymś zapomni.

**Rozwiązanie:** Konfigurowalna lista plików tworzonych domyślnie (bez pytania), z możliwością wyłączenia.

```toml
# ~/.vulp/config.toml
default_init_files = ["README.md", "CHANGELOG.md", ".env.example", ".gitignore"]
```

```bash
vulp init moj-projekt                    # tworzy domyślny zestaw
vulp init moj-projekt --no-defaults     # tworzy puste repo
vulp init moj-projekt --files readme,license  # tylko te dwa
```

Dodatkowo: **szablony zawartości pliku README** konfigurowalne w `config.toml` (`readme_template = "heartbyte-default"` już istnieje w konfigu, ale nie jest zaimplementowane).

**Trudność:** niska — już mamy `InitConfig` z polami `readme`, `changelog` itd., wystarczy ustawić domyślne wartości z configa zamiast false.

---

### 4. `vulp sync branch cleanup` — sprzątanie zmergowanych gałęzi

**Problem:** Po miesiącach pracy masz 30 lokalnych gałęzi, z których 25 jest już zmergowanych do main. Zajmują miejsce, mylą.

**Rozwiązanie:**
```bash
vulp sync branch cleanup           # interaktywny wybór (TUI lista)
vulp sync branch cleanup --all     # usuń wszystkie zmergowane bez pytania
vulp sync branch cleanup --dry-run # pokaż co by się usunęło
```

Usuwa gałęzie które są już zmergowane do `main`/`master` — lokalnie i opcjonalnie zdalnie.

**Implementacja:** `git2` — `repo.branches()`, sprawdzenie czy jest zmergowana przez `repo.merge_analysis()`. Już mamy `delete_branch()` w `sync/mod.rs`.

**Trudność:** niska — wszystkie potrzebne funkcje `git2` już są w kodzie.

---

## Średni priorytet

### 5. Shell completions (bash / zsh / fish / PowerShell)

Wpisanie `vulp <Tab>` podpowiada:
- subkomendy (`status`, `sync`, `init`...)
- nazwy repo dla `vulp status <Tab>` → lista repo z workspace_root
- nazwy workspace dla `--workspace <Tab>`
- aliasy kont dla `vulp auth switch <Tab>`

```bash
vulp completions bash   >> ~/.bashrc
vulp completions zsh    >> ~/.zshrc
vulp completions fish   > ~/.config/fish/completions/vulp.fish
vulp completions powershell >> $PROFILE
```

**Implementacja:** `clap` ma wbudowany generator completion przez crate `clap_complete`. Dynamiczne uzupełnianie nazw repo wymaga osobnego skryptu który wywołuje `vulp --list-repos` (nowa ukryta flaga).

**Trudność:** niska (statyczne) / średnia (dynamiczne nazwy repo).

---

### 6. `vulp status --watch` — live monitoring workspace

```bash
vulp status --watch          # odświeża TUI co 30 sekund
vulp status --watch --interval 10   # co 10 sekund
```

TUI pokazuje kiedy ostatnio odświeżono. Przy zmianie statusu (np. nowy commit na serwerze) wiersz miga przez chwilę na żółto.

**Implementacja:** Tokio timer w pętli TUI, wywołuje `scan_workspace()` co N sekund. Detekcja zmian przez porównanie `ahead/behind` z poprzednim stanem.

**Trudność:** niska — już mamy `scan_workspace()` i pętlę TUI.

---

### 7. Szablony projektów (`vulp init --template`)

```bash
vulp init moj-projekt --template react-app
vulp init moj-projekt --template rust-cli
vulp init moj-projekt --template python-fastapi
vulp init moj-projekt --template node-express
```

Każdy szablon tworzy kompletną strukturę katalogów i pliki startowe:
- `react-app` → `src/`, `public/`, `package.json` z React, `.eslintrc`, Dockerfile
- `rust-cli` → `src/main.rs`, `Cargo.toml` z clap+anyhow, `README.md`
- `python-fastapi` → `main.py`, `requirements.txt`, `.env.example`, `Dockerfile`

Szablony przechowywane w `~/.vulp/templates/` jako foldery — użytkownik może dodać własne.

**Trudność:** średnia — logika tworzenia plików jest już w `init/mod.rs`, potrzebny nowy system szablonów.

---

### 8. `vulp pr list` — przegląd otwartych PR

```bash
vulp pr list                     # wszystkie PR we wszystkich repo workspace
vulp pr list --repo moj-projekt  # tylko jedno repo
vulp pr list --mine              # tylko PR gdzie jesteś autorem lub reviewerem
```

Wyświetla TUI tabelę: repo | numer | tytuł | autor | data | etykiety.

Enter na PR otwiera URL w przeglądarce.

**Implementacja:** Nowa metoda `list_pull_requests()` w trait `Provider`, wywołanie API. GitHub/GitLab/Gitea wszystkie to mają.

**Trudność:** średnia — nowa metoda w każdym providerze.

---

### 9. Globalny `.vulpignore` — repo do pominięcia

```
# ~/.vulp/ignore
archived-*        # wszystkie repo zaczynające się od "archived-"
old-monorepo      # konkretne repo po nazwie
```

`scan_workspace()` pomija pasujące foldery. Przydatne gdy `workspace_root` zawiera też foldery inne niż repo git.

**Implementacja:** Dodać `ignore_patterns: Vec<String>` do `AppConfig`, sprawdzać przy `scan_workspace()`.

**Trudność:** niska.

---

### 10. `vulp stats --trend` — wykres aktywności w czasie

```bash
vulp stats --trend             # ostatnie 30 dni
vulp stats --trend --days 90
vulp stats --trend --repo api-backend
```

ASCII wykres słupkowy commitów per dzień:

```
Aktywność (ostatnie 30 dni) — api-backend

May 01  ████████░░░░░░░░░░░░░  8
May 08  ████████████░░░░░░░░░  12
May 15  ██████░░░░░░░░░░░░░░░  6
May 22  ██████████████████░░░  18
```

**Implementacja:** `revwalk` po historii commitów + grupowanie po dniach. Już mamy `git2` w projekcie.

**Trudność:** niska.

---

## Przyszłość / ambitne

### 11. VSCode Extension — automatyczne time tracking

Aktualnie `vulp stats time` czyta pliki JSON z `~/.vulp/vsc-data/`. Trzeba je tworzyć ręcznie lub osobnym narzędziem.

**Rozwiązanie:** Towarzyszące rozszerzenie VS Code które automatycznie zapisuje czas spędzony na plikach w formacie rozumianym przez `vulp`.

```
vulp stats time            # pokazuje automatycznie zbierane dane
```

---

### 12. Webhook przy każdym sync

Opcjonalny URL w `config.toml`:
```toml
sync_webhook = "https://discord.com/api/webhooks/..."
```

Po każdym udanym `push` wysyła embed z: repo, branch, commit message, autor.
Identyczny mechanizm jak audit log w innych projektach z tego zespołu.

**Trudność:** niska — prosty `reqwest::post` po każdym `push_repo()`.

---

### 13. AI-assisted commit messages

```bash
vulp sync commit --ai        # generuje wiadomość z diff za pomocą AI API
vulp sync commit --ai --edit # generuje, ale otwiera edytor do poprawki przed commitem
```

Wysyła `git diff --staged` do API (np. Claude lub OpenAI), dostaje propozycję wiadomości w stylu conventional commits.

Wymaga klucza API w konfiguracji.

---

### 14. `vulp release` multi-platform publish

Po stworzeniu release tagu — automatyczne wyzwolenie publishu na:
- `crates.io` (dla Rust)
- `npm` (dla Node.js)
- `PyPI` (dla Python)

Wykrywa typ projektu przez `detect_project_type()` (już istnieje w `summary/mod.rs`).

---

### 15. Interaktywny diff viewer

W widoku szczegółowym repo (`vulp status <repo>`) — klawisz `d` otwiera inlinowy diff zmienionych plików z kolorowym podświetleniem:

```
  M  src/components/Nav.tsx    [d] diff  [s] stage  [r] revert
─────────────────────────────────────────
  - const navItems = ["Home", "About"];
  + const navItems = ["Home", "About", "Contact"];
```

**Trudność:** wysoka — wymaga parsowania diff output i renderowania w ratatui.

---

## Szybkie podsumowanie priorytetów

| # | Pomysł | Priorytet | Trudność |
|---|---|---|---|
| 1 | `vulp clone --mine/--contributed` | Wysoki | Średnia |
| 2 | Tagi → organizacja folderów | Wysoki | Średnia |
| 3 | Domyślna paczka plików przy init | Wysoki | Niska |
| 4 | `branch cleanup` | Wysoki | Niska |
| 5 | Shell completions | Średni | Niska–Średnia |
| 6 | `status --watch` | Średni | Niska |
| 7 | Szablony projektów | Średni | Średnia |
| 8 | `pr list` | Średni | Średnia |
| 9 | `.vulpignore` | Średni | Niska |
| 10 | `stats --trend` | Średni | Niska |
| 11 | VSCode extension | Przyszłość | Wysoka |
| 12 | Webhook przy sync | Przyszłość | Niska |
| 13 | AI commit messages | Przyszłość | Średnia |
| 14 | Multi-platform publish | Przyszłość | Wysoka |
| 15 | Diff viewer w TUI | Przyszłość | Wysoka |
