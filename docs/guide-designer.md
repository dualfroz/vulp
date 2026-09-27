# Guide for designers: how to use vulp

> This guide assumes you have never used a terminal to work with code. Relax, it is simpler than it looks.

---

## What is it and who is it for?

Imagine that every project (a website, an app, a set of assets) lives in its own folder on GitHub. `vulp` is a program that lets you manage all of those folders at once: see what changed, send your changes to the server, get the latest version, all without clicking through the GitHub website.

**For you this means:** instead of opening every repository on GitHub one by one, you type one command and see the state of everything.

---

## Step 1: Install

### Windows

Open **PowerShell** (search for "PowerShell" in the Start menu) and paste:
```
irm https://raw.githubusercontent.com/dualfroz/vulp/main/install/vulp-installer.ps1 | iex
```
Then close and reopen PowerShell, type `vulp --version` and press Enter. If you see a version number, the installation worked.

You can also download `vulp-windows-x86_64.exe` by hand from the [Releases](https://github.com/dualfroz/vulp/releases) page (the "Assets" section), rename it to `vulp.exe` and move it to a folder that is on your `PATH`.

### Linux (for example Linux Mint)

Open a terminal and paste:
```
curl -fsSL https://raw.githubusercontent.com/dualfroz/vulp/main/install/vulp-installer.sh | bash
```
Done.

### macOS

There is no prebuilt binary for macOS. You need Rust ([rustup.rs](https://rustup.rs)), and then, in the folder with the vulp source code:
```
cargo install --path .
```

---

## Step 2: First run (you do this ONCE)

Open a terminal and type:

```
vulp config
```

A settings screen appears. Set:

| Field | What to enter |
|---|---|
| `workspace_root` | The folder where you keep your projects, e.g. `C:\Users\YourName\projects` or `/home/you/projects` |
| `username` | Your name or nickname (it will be visible in commits) |
| `email` | The email address of your GitHub account |

Move with the arrow keys, press Enter to edit a field and Enter again to accept it. Press `q` or `Esc` to save and leave.

---

## Step 3: Connect your GitHub account (you do this ONCE)

If the GitHub CLI (`gh`) is already installed and logged in on your computer, this is all you need:

```
vulp auth import
```

Otherwise you need an **access token** from GitHub. Here is how to get one:

1. Go to [github.com/settings/tokens](https://github.com/settings/tokens)
2. Click **"Generate new token (classic)"**
3. Enter a name (e.g. "vulp on my laptop")
4. Tick `repo` (the whole scope)
5. Click **"Generate token"** at the bottom
6. **Copy the token**, you will only see it once!

Now go back to the terminal and type (replace `YOUR_TOKEN` with the copied token):

```
vulp auth add --provider github --token YOUR_TOKEN
```

Done. The token is stored on your computer only.

---

## Everyday use

### See what is going on in all projects

```
vulp list
```

You get one line per project: its name, the current branch, status marks and the main language. The marks mean:

| Mark | Meaning |
|---|---|
| warning sign | You have unsaved changes (like an "unsaved document") |
| down arrow | There are newer changes on the server, someone added something |
| up arrow | You have changes ready to send to the server |

For a full picture of a single project, type `vulp status` and the project name, e.g. `vulp status website`.

To see everything on one screen, type just `vulp`. Switch tabs with `Tab` or the arrow keys, move up and down with the arrow keys, press `Enter` to open a project and `q` to leave. `?` lists every key.

---

### Get the latest version of your projects

If a project shows the down arrow, someone added changes on the server. Get them:

```
vulp sync --all
```

This downloads the latest version of **all** projects at once.

---

### Send your changes to the server

Do you have new files (new graphics, updated assets)? Use the interactive screen:

```
vulp
```

1. Go to the **Repos** tab (`Tab` or the arrow keys).
2. **Select a project** by pressing `Space` next to it. You can select several.
   - `Ctrl+A` selects all of them at once
3. **Press `s`** to open the sync menu.
4. **Choose "Commit+Push"** with the arrow keys, then Enter. Replace the suggested message with a short description of your changes (e.g. "new banners for the Q2 campaign") and press Enter.

> **Commit vs push:** a commit is like "saving a version" on your computer. A push sends that version to the server. "Commit+Push" does both, one after the other. "Push only" sends versions you have already committed.

---

### Quick push from the command line (if you know what you are doing)

```
vulp sync --all --action commit-push -m "landing page graphics update"
```

This commits (with the message after `-m`) and pushes every changed project in one go.

---

## Glossary

| Word | In plain words |
|---|---|
| **Repository** (repo) | A project folder on GitHub. Like a binder with the project's files. |
| **Commit** | Saving a version of the files with a note "what I changed". Like Ctrl+S with a label. |
| **Push** | Sending your changes to the GitHub server. |
| **Pull** | Getting changes from the server onto your computer. |
| **Branch** | A version of the project. `main` is the main one, `develop` is the working one. |
| **Dirty** | Files that were changed but not committed yet. |
| **Merge** | Joining two versions of a project into one. Usually done by a developer. |

---

## Something does not work?

**The terminal does not know the `vulp` command**
- Windows: close and reopen PowerShell. If you installed by hand, check that the folder with `vulp.exe` is on your PATH.
- Linux: the installer puts vulp in `~/.local/bin`. Close and reopen the terminal; if it still does not work, add that folder to your PATH.
- macOS: check that `~/.cargo/bin` is on your PATH.

**"Permission denied" or "access denied"**
- Linux/macOS: type `chmod +x vulp` before the first run of a file you downloaded by hand

**"Authentication failed" on push or pull**
- The token has probably expired or does not have the `repo` scope
- Generate a new token and add it: `vulp auth add --provider github --token NEW_TOKEN`

**I do not see my projects in `vulp list`**
- Check that `workspace_root` in `vulp config` points to the right folder
- The folder has to contain project folders (every project is a separate folder with a `.git` folder inside)

**My changes are not sent, push does nothing**
- First run `vulp list --dirty`: a project only has something to commit when it shows up there
- If it does not show up, but `vulp list --ahead` lists it, a plain push is enough: `vulp sync PROJECT --action push`

---

## Cheat sheet

```
vulp                     # everything on one screen
vulp list                # what is going on
vulp sync --all          # get the latest versions
vulp sync --all --action commit-push -m "what changed"   # quick commit + push
vulp auth add --provider github --token TOKEN            # add an account
vulp config              # change settings
```
