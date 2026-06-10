# Poradnik dla grafika — jak używać fxg

> Ten poradnik zakłada, że nigdy wcześniej nie używałeś terminalu do pracy z kodem. Spokojnie — to jest prostsze niż myślisz.

---

## Co to jest i po co to komuś?

Wyobraź sobie, że każdy projekt (strona, aplikacja, assets) mieszka w osobnym folderze na GitHubie. `fxg` to program, który pozwala ci zarządzać tymi wszystkimi folderami jednocześnie — zobaczyć co się zmieniło, wysłać zmiany na serwer, pobrać najnowszą wersję — wszystko bez klikania po interfejsie GitHuba.

**Dla Ciebie oznacza to:** zamiast wchodzić na stronę GitHuba i klikać w każde repozytorium osobno, wpisujesz jedną komendę i widzisz status wszystkiego.

---

## Krok 1 — Instalacja

### Windows

1. Pobierz plik `fxg.exe` z sekcji [Releases](https://github.com/twoj-team/fxg/releases) (zakładka "Assets")
2. Przenieś plik do folderu `C:\Users\TwojeImie\` (czyli do swojego folderu domowego)
3. Otwórz **Terminal** (wyszukaj "Terminal" w menu Start, lub `Windows + R` → wpisz `cmd` → Enter)
4. Wpisz `fxg --version` i naciśnij Enter — jeśli widzisz numer wersji, instalacja się udała

> Jeśli system pyta "Czy chcesz uruchomić ten plik?" — kliknij "Tak" / "Uruchom mimo to".

### Linux (np. Linux Mint)

Otwórz terminal i wklej:
```
curl -fsSL https://github.com/twoj-team/fxg/install.sh | bash
```
Gotowe.

### macOS

```
brew install twoj-team/tap/fxg
```
(wymaga Homebrew — jeśli nie masz, zapytaj kolegę programistę)

---

## Krok 2 — Pierwsze uruchomienie (robisz to RAZ)

Otwórz terminal i wpisz:

```
fxg config
```

Pojawi się ekran konfiguracji. Ustaw:

| Pole | Co wpisać |
|---|---|
| `workspace_root` | Folder gdzie trzymasz projekty, np. `C:\Users\TwojeImie\projekty` lub `/home/twoje/projekty` |
| `username` | Twoje imię i nazwisko lub nick (to będzie widoczne w commitach) |
| `email` | Twój email z GitHuba |

Poruszasz się strzałkami, edytujesz pole po naciśnięciu Enter, zapisujesz przez `q` lub `Ctrl+S`.

---

## Krok 3 — Podłącz konto GitHub (robisz to RAZ)

Potrzebujesz **tokenu dostępu** z GitHuba. Oto jak go zdobyć:

1. Wejdź na [github.com/settings/tokens](https://github.com/settings/tokens)
2. Kliknij **"Generate new token (classic)"**
3. Wpisz nazwę (np. "fxg na moim laptopie")
4. Zaznacz: `repo` (cały zakres)
5. Kliknij **"Generate token"** na dole
6. **Skopiuj token** — zobaczysz go tylko raz!

Teraz wróć do terminalu i wpisz (zamień `TWOJ_TOKEN` na skopiowany token):

```
fxg auth add --provider github --token TWOJ_TOKEN
```

Gotowe. Token jest bezpiecznie zapisany na Twoim komputerze.

---

## Codzienne użycie

### Sprawdź co się dzieje we wszystkich projektach

```
fxg status
```

Zobaczysz tabelę z projektami. Co oznaczają kolumny:

```
  #   NAZWA           BRANCH     AHEAD  BEHIND  DIRTY  OSTATNI COMMIT
  1   projekt-strona  main           0       2      ✗   2h temu
  2   projekt-logo    develop        1       0      ✓   wczoraj
```

| Kolumna | Co znaczy |
|---|---|
| `AHEAD` | Masz zmiany gotowe do wysłania na serwer |
| `BEHIND` | Na serwerze są nowsze zmiany — ktoś coś dodał |
| `DIRTY ✓` | Masz niezapisane zmiany (jak "niezapisany dokument") |

**Poruszanie się:** strzałki góra/dół, `Enter` żeby wejść w szczegóły projektu, `q` żeby wyjść.

---

### Pobierz najnowszą wersję projektu

Jeśli `BEHIND` pokazuje liczbę większą od 0, ktoś dodał zmiany na serwerze. Pobierz je:

```
fxg sync pull --all
```

To pobierze najnowszą wersję **wszystkich** projektów naraz.

---

### Wyślij swoje zmiany na serwer

Masz nowe pliki (np. nowe grafiki, zaktualizowane assets)? Użyj interaktywnego trybu:

```
fxg sync
```

Pojawi się ekran z listą projektów. Obsługa:

1. **Zaznacz projekt** — naciśnij `Spacja` przy projekcie który chcesz wysłać. Możesz zaznaczyć kilka.
   - `a` — zaznacz wszystkie naraz
2. **Naciśnij Enter** — przejdziesz do wyboru akcji
3. **Wybierz "Commit"** — strzałkami, potem Enter. Wpisz opis zmian (np. "nowe bannery dla kampanii Q2"), Enter.
4. **Wybierz "Push"** — to faktycznie wysyła na serwer.

> **Commit vs Push:** Commit to jak "zapisanie wersji" lokalnie. Push to wysłanie tej wersji na serwer. Zawsze robisz najpierw Commit, potem Push.

---

### Szybki push z linii komend (jeśli wiesz co robisz)

```
fxg sync push --all -m "aktualizacja grafik landing page"
```

To jednocześnie commita (z wiadomością po `-m`) i pushuje wszystkie zmienione projekty.

---

## Słowniczek

| Słowo | Po ludzku |
|---|---|
| **Repozytorium** (repo) | Folder projektu na GitHubie. Jak teczka z plikami projektu. |
| **Commit** | Zapisanie wersji plików z opisem "co zmieniłem". Jak ctrl+S z etykietką. |
| **Push** | Wysłanie swoich zmian na serwer GitHuba. |
| **Pull** | Pobranie zmian z serwera na swój komputer. |
| **Branch** | Wersja projektu. `main` to główna, `develop` to robocza. |
| **Dirty** | Pliki zmienione ale jeszcze nie scommitowane. |
| **Merge** | Połączenie dwóch wersji projektu w jedną. Robi to zwykle programista. |

---

## Coś nie działa?

**Terminal nie zna komendy `fxg`**
- Windows: sprawdź czy plik `fxg.exe` jest w folderze który jest w PATH. Najprostsze rozwiązanie: przenieś plik do `C:\Windows\System32\`
- Linux/Mac: sprawdź czy `~/.cargo/bin` jest w PATH, lub uruchom ręcznie: `./fxg`

**"Permission denied" lub "access denied"**
- Linux/Mac: wpisz `chmod +x fxg` przed pierwszym uruchomieniem

**"Authentication failed" przy push/pull**
- Token prawdopodobnie wygasł lub nie ma uprawnień do `repo`
- Wygeneruj nowy token i dodaj: `fxg auth add --provider github --token NOWY_TOKEN`

**Nie widzę swoich projektów w `fxg status`**
- Sprawdź czy `workspace_root` w `fxg config` wskazuje na właściwy folder
- Folder musi zawierać podfoldery z projektami (każdy projekt to osobny podfolder z `.git` w środku)

**Zmiany się nie wysyłają, `fxg sync push` nic nie robi**
- Najpierw sprawdź `fxg status` — kolumna `DIRTY` musi być `✓` żeby był co wysyłać
- Jeśli `DIRTY` jest `✗`, ale `AHEAD` > 0 — wystarczy sam push bez commita

---

## Szybka ściąga

```
fxg status              # sprawdź co się dzieje
fxg sync pull --all     # pobierz najnowsze wersje
fxg sync                # interaktywny ekran: commit + push
fxg sync push --all -m "opis zmian"   # szybki commit + push
fxg auth add --provider github --token TOKEN   # dodaj konto
fxg config              # zmień ustawienia
```
