# fxg — strategia cross-platform

> Cel: jedna binarka, trzy systemy, instalacja w 30 sekund, zero wiedzy technicznej wymaganej.

---

## Stan obecny

Rust kompiluje się do natywnych binarek bez zewnętrznych zależności runtime. To dobra podstawa — nie trzeba instalować Pythona, Node.js ani JVM. Ale dziś jedynym sposobem instalacji jest `cargo install --path .`, co wymaga zainstalowanego Rusta i środowiska build. Dla grafika — nierealne.

Obecny problem z `keyring`: na Linuksie wymaga `libdbus` lub `libsecret`. Do rozwiązania przy pakowaniu.

---

## Cel UX instalacji

**Windows:** Pobierasz plik `.exe`. Klikasz go dwukrotnie. Otwiera się terminal z konfiguratorem. Po 2 minutach piszesz `fxg status` i to działa.

**Linux:** Wklejasz jedną linię w terminal. Po 10 sekundach piszesz `fxg status` i to działa.

**macOS:** `brew install`. Jedna komenda.

---

## Strategia dystrybucji — GitHub Releases + CI

### GitHub Actions — automatyczne buildowanie binarek

Przy każdym tagu (`v*.*.*`) CI buduje binarkę na 4 targety:

```yaml
# .github/workflows/release.yml
strategy:
  matrix:
    include:
      - os: ubuntu-latest
        target: x86_64-unknown-linux-musl   # statyczna binarka — działa na każdym Linuksie
        artifact: fxg-linux-x64
      - os: ubuntu-latest
        target: aarch64-unknown-linux-musl  # ARM (np. Raspberry Pi, nowe serwery)
        artifact: fxg-linux-arm64
      - os: windows-latest
        target: x86_64-pc-windows-msvc
        artifact: fxg-windows-x64.exe
      - os: macos-latest
        target: x86_64-apple-darwin
        artifact: fxg-macos-x64
      - os: macos-latest
        target: aarch64-apple-darwin         # Apple Silicon (M1/M2/M3)
        artifact: fxg-macos-arm64
```

> Używamy `x86_64-unknown-linux-musl` zamiast `gnu` — binarka statyczna, nie wymaga żadnych systemowych `.so`. Keyring na Linuksie fallback do pliku szyfrowanego (jeśli brak dbus).

Pliki lądują w GitHub Releases jako Assets. Użytkownik pobiera odpowiedni plik dla swojego systemu.

---

## Linux — instalacja

### Skrypt jednolinijkowy (docelowy UX)

```bash
curl -fsSL https://github.com/twoj-user/fxg/releases/latest/download/install.sh | bash
```

Skrypt robi:
1. Wykrywa architekturę (`uname -m`)
2. Pobiera odpowiednią binarkę z GitHub Releases
3. Umieszcza ją w `~/.local/bin/fxg`
4. Dodaje `~/.local/bin` do `$PATH` w `.bashrc`/`.zshrc` jeśli jeszcze nie ma
5. Jeśli brak konfiguracji → uruchamia `fxg setup` (first-run wizard)

```bash
#!/bin/bash
set -e
ARCH=$(uname -m)
OS=linux
case $ARCH in
  x86_64)  FILE="fxg-linux-x64" ;;
  aarch64) FILE="fxg-linux-arm64" ;;
  *) echo "Nieobsługiwana architektura: $ARCH"; exit 1 ;;
esac

LATEST=$(curl -s https://api.github.com/repos/twoj-user/fxg/releases/latest | grep tag_name | cut -d '"' -f 4)
URL="https://github.com/twoj-user/fxg/releases/download/${LATEST}/${FILE}"

mkdir -p ~/.local/bin
curl -fsSL "$URL" -o ~/.local/bin/fxg
chmod +x ~/.local/bin/fxg

# Dodaj do PATH jeśli brak
if ! echo "$PATH" | grep -q "$HOME/.local/bin"; then
  echo 'export PATH="$HOME/.local/bin:$PATH"' >> ~/.bashrc
  export PATH="$HOME/.local/bin:$PATH"
fi

echo "fxg zainstalowany. Uruchamianie konfiguracji..."
fxg setup
```

### Opcjonalnie: pakiety .deb i .rpm

Generowane przez `cargo-deb` i `cargo-rpm` w CI — przydatne dla systemów zarządzanych przez IT.

```bash
# Ubuntu/Debian
sudo dpkg -i fxg_0.1.0_amd64.deb

# Fedora/RHEL/CentOS
sudo rpm -i fxg-0.1.0.x86_64.rpm
```

---

## Windows — instalacja

### Opcja A: Kliknięcie `.exe` (docelowy UX dla grafika)

Gdy użytkownik dwukrotnie kliknie `fxg-windows-x64.exe` bez żadnych argumentów i brak konfiguracji, zamiast wyświetlić help i zamknąć okno — **otwiera PowerShell z konfiguratorem**.

Implementacja w `src/main.rs`:
```rust
#[cfg(windows)]
fn ensure_console() {
    // Jeśli uruchomiono przez dwuklik (nie z terminala), otwórz nowe okno konsoli
    // i uruchom fxg setup w nim
    use std::os::windows::process::CommandExt;
    if !is_launched_from_terminal() {
        std::process::Command::new("cmd")
            .args(["/k", "fxg.exe", "setup"])
            .creation_flags(0x00000010) // CREATE_NEW_CONSOLE
            .spawn().ok();
        std::process::exit(0);
    }
}
```

### Opcja B: Skrypt PowerShell (instalacja systemowa)

```powershell
# Wklej w PowerShell jako Administrator:
iwr https://github.com/twoj-user/fxg/releases/latest/download/install.ps1 | iex
```

Skrypt:
1. Pobiera `fxg-windows-x64.exe`
2. Umieszcza w `%USERPROFILE%\AppData\Local\fxg\`
3. Dodaje ten folder do `$PATH` (przez `[Environment]::SetEnvironmentVariable`)
4. Uruchamia `fxg setup` w nowym oknie terminalu

### Opcja C: WinGet (Microsoft Store CLI) — przyszłość

```
winget install fxg
```

Wymaga zgłoszenia pakietu do repozytorium `winget-pkgs`.

---

## macOS — instalacja

### Homebrew (docelowy UX)

```bash
brew install twoj-user/tap/fxg
```

Wymaga stworzenia własnego "tap" (repozytorium z formularzem Homebrew):

```ruby
# Formula/fxg.rb
class Fxg < Formula
  desc "Multi-repository git workflow manager"
  homepage "https://github.com/twoj-user/fxg"
  version "0.1.0"

  on_macos do
    if Hardware::CPU.arm?
      url "https://github.com/twoj-user/fxg/releases/download/v0.1.0/fxg-macos-arm64"
      sha256 "..."
    else
      url "https://github.com/twoj-user/fxg/releases/download/v0.1.0/fxg-macos-x64"
      sha256 "..."
    end
  end

  def install
    bin.install "fxg-macos-arm64" => "fxg"
  end
end
```

---

## First-run wizard

Kluczowy element UX. Gdy użytkownik uruchamia **dowolną komendę** po raz pierwszy (brak `~/.fxg/config.toml`), zamiast error'a dostaje automatyczny setup.

Implementacja w `src/app.rs`, przed każdym `match cli.command`:

```rust
// src/app.rs
pub async fn run(cli: Cli) -> Result<()> {
    // Jeśli brak konfiguracji → uruchom first-run setup
    if !config::config_path().exists() {
        println!("Wygląda na to, że uruchamiasz fxg po raz pierwszy.");
        println!("Przeprowadźmy krótką konfigurację (zajmie ~2 minuty).\n");
        first_run_setup().await?;
    }

    let cfg = config::load_config()?;
    // ... reszta
}

async fn first_run_setup() -> Result<()> {
    // 1. Uruchom config TUI
    let cfg = config::run_first_time().await?;
    // 2. Zapytaj o dodanie konta
    if inquire::Confirm::new("Czy chcesz teraz podłączyć konto GitHub/GitLab?")
        .with_default(true).prompt()? {
        // Uruchom auth add wizard
        auth::run_add_wizard(&cfg).await?;
    }
    println!("\nGotowe! Wpisz 'fxg status' żeby zobaczyć swoje projekty.");
    Ok(())
}
```

---

## Systemowe zależności — problemy i rozwiązania

| Zależność | Problem | Rozwiązanie |
|---|---|---|
| `libgit2` | Wymagana na Linuksie przy dynamicznym linkowaniu | Używamy `git2` z feature `vendored` — kompiluje libgit2 statycznie |
| `keyring` / `libsecret` | Wymaga `dbus` na Linuksie headless | Fallback do zaszyfrowanego pliku `~/.fxg/credentials.enc` gdy brak dbus |
| `openssl` | Różne wersje na różnych systemach | Feature `native-tls` → `rustls` — TLS w całości wbudowany w Rust |

```toml
# Cargo.toml — docelowa konfiguracja dla statycznych binarek
[dependencies]
git2     = { version = "0.19", features = ["vendored"] }
reqwest  = { version = "0.12", features = ["json", "rustls-tls"], default-features = false }
keyring  = { version = "3", features = ["sync-secret-service"] }
```

---

## O nazwie

Aktualna nazwa `fxg` ("finally executable git") to placeholder — dobra jako codename, ale przed publicznym wydaniem warto rozważyć zmianę.

Kryteria dobrej nazwy narzędzia CLI:
- Krótka (2–4 znaki idealne, max 6)
- Łatwa do wymówienia i zapamiętania
- Nie koliduje z istniejącymi komendami systemowymi
- Nawiązuje do funkcji (opcjonalnie)

Kilka luźnych kierunków (bez rekomendacji — ostateczna decyzja należy do Ciebie):
- Coś od "workspace" / "repos" / "git"
- Coś sugerującego "wielość" lub "hub"
- Akronim który znaczy coś po polsku, a po angielsku brzmi jak komenda

Niezależnie od wyboru nazwy — warto go podjąć przed stworzeniem Homebrew tap i pierwszym publicznym Release, bo zmiana nazwy binarki po publikacji jest uciążliwa.

---

## Roadmap dystrybucji

```
v0.1 (teraz)    cargo install — tylko dla developerów
v0.2            GitHub Releases z binaries (Linux + Windows + macOS)
                Skrypt instalacyjny curl | bash
v0.3            First-run wizard (bez potrzeby wpisywania komend setup)
                PowerShell install script dla Windows
v0.4            Homebrew tap dla macOS
v1.0            Stabilny API, binarka klikalna na Windows
                WinGet (opcjonalnie)
                Pakiety .deb/.rpm (opcjonalnie)
```
