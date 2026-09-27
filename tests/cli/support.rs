use serde_json::Value;
use std::path::{Path, PathBuf};
use std::process::Command;
use tempfile::TempDir;

pub struct Run {
    pub code: i32,
    pub stdout: String,
    pub stderr: String,
}

impl Run {
    pub fn success(self) -> Self {
        assert_eq!(self.code, 0, "expected success\nstdout:\n{}\nstderr:\n{}", self.stdout, self.stderr);
        self
    }

    pub fn failure(self, code: i32) -> Self {
        assert_eq!(self.code, code, "unexpected exit code\nstdout:\n{}\nstderr:\n{}", self.stdout, self.stderr);
        self
    }

    pub fn json(&self) -> Value {
        serde_json::from_str(&self.stdout)
            .unwrap_or_else(|e| panic!("stdout is not JSON ({e}):\n{}", self.stdout))
    }
}

pub struct Sandbox {
    _dir: TempDir,
    pub home: PathBuf,
    pub workspace: PathBuf,
    pub outside: PathBuf,
}

impl Sandbox {
    pub fn new() -> Self {
        Self::with_config("")
    }

    pub fn with_config(extra: &str) -> Self {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().canonicalize().unwrap();
        let sandbox = Self {
            home: root.join("home"),
            workspace: root.join("workspace"),
            outside: root.join("outside"),
            _dir: dir,
        };
        for d in [&sandbox.home, &sandbox.workspace, &sandbox.outside] {
            std::fs::create_dir_all(d).unwrap();
        }
        std::fs::write(sandbox.home.join(".gitconfig"), concat!(
            "[user]\n\tname = Test User\n\temail = test@example.com\n",
            "[init]\n\tdefaultBranch = main\n",
            "[commit]\n\tgpgsign = false\n",
            "[tag]\n\tgpgsign = false\n",
            "[pull]\n\trebase = false\n",
            "[advice]\n\tdetachedHead = false\n",
        )).unwrap();
        sandbox.write_config(extra);
        sandbox
    }

    pub fn write_config(&self, extra: &str) {
        let data_dir = self.home.join(".vulp");
        let config = format!(
            "workspace_root = \"{ws}\"\n\
             data_dir = \"{data}\"\n\
             default_provider = \"github\"\n\
             default_branch = \"main\"\n\
             default_visibility = \"private\"\n\
             editor = \"true\"\n\
             username = \"tester\"\n\
             email = \"test@example.com\"\n\
             gitignore_template = \"blank\"\n\
             readme_template = \"default\"\n\
             share_stats = false\n\
             timezone = \"UTC\"\n\
             vsc_data_dir = \"{data}/vsc-data\"\n\
             clipboard_cmd = \"\"\n\
             {extra}\n",
            ws = self.workspace.display(),
            data = data_dir.display(),
        );
        self.write_data("config.toml", &config);
    }

    fn env(&self, cmd: &mut Command) {
        cmd.env("HOME", &self.home)
            .env("GIT_CONFIG_GLOBAL", self.home.join(".gitconfig"))
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("GIT_TERMINAL_PROMPT", "0")
            .env_remove("VULP_AGENT")
            .env_remove("VULP_REPO")
            .env_remove("GIT_DIR")
            .env_remove("GIT_WORK_TREE")
            .env_remove("XDG_CONFIG_HOME");
    }

    pub fn vulp(&self, args: &[&str]) -> Run {
        self.vulp_with_env(args, &[])
    }

    pub fn vulp_with_env(&self, args: &[&str], env: &[(&str, &str)]) -> Run {
        let mut cmd = Command::new(env!("CARGO_BIN_EXE_vulp"));
        cmd.args(args).current_dir(&self.outside);
        self.env(&mut cmd);
        cmd.envs(env.iter().copied());
        let out = cmd.output().expect("vulp binary runs");
        Run {
            code: out.status.code().expect("vulp exited normally"),
            stdout: String::from_utf8_lossy(&out.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&out.stderr).into_owned(),
        }
    }

    pub fn git(&self, dir: &Path, args: &[&str]) -> String {
        let mut cmd = Command::new("git");
        cmd.arg("-C").arg(dir).args(args);
        self.env(&mut cmd);
        let out = cmd.output().expect("git binary must be on PATH");
        assert!(
            out.status.success(),
            "git {args:?} in {} failed: {}",
            dir.display(),
            String::from_utf8_lossy(&out.stderr)
        );
        String::from_utf8_lossy(&out.stdout).trim().to_string()
    }

    pub fn commit(&self, repo: &Path, file: &str, content: &str, message: &str) -> String {
        let target = repo.join(file);
        std::fs::create_dir_all(target.parent().unwrap()).unwrap();
        std::fs::write(target, content).unwrap();
        self.git(repo, &["add", "-A"]);
        self.git(repo, &["commit", "-q", "-m", message]);
        self.git(repo, &["rev-parse", "HEAD"])
    }

    pub fn repo(&self, name: &str) -> PathBuf {
        let path = self.workspace.join(name);
        std::fs::create_dir_all(&path).unwrap();
        self.git(&path, &["init", "-q"]);
        self.commit(&path, "README.md", &format!("# {name}\n"), "initial commit");
        path
    }

    pub fn repo_with_remote(&self, name: &str) -> (PathBuf, PathBuf) {
        let bare = self.outside.join(format!("{}.git", name.replace('/', "-")));
        std::fs::create_dir_all(&bare).unwrap();
        self.git(&bare, &["init", "-q", "--bare"]);
        let repo = self.repo(name);
        self.git(&repo, &["remote", "add", "origin", bare.to_str().unwrap()]);
        self.git(&repo, &["push", "-q", "-u", "origin", "main"]);
        (repo, bare)
    }

    pub fn clone_outside(&self, bare: &Path, dir_name: &str) -> PathBuf {
        let target = self.outside.join(dir_name);
        self.git(&self.outside, &["clone", "-q", bare.to_str().unwrap(), target.to_str().unwrap()]);
        target
    }

    pub fn write_data(&self, file: &str, content: &str) {
        let path = self.home.join(".vulp").join(file);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, content).unwrap();
    }

    pub fn read_data(&self, file: &str) -> String {
        std::fs::read_to_string(self.home.join(".vulp").join(file)).unwrap_or_default()
    }
}

pub fn names(list: &Value) -> Vec<String> {
    list.as_array()
        .expect("JSON array")
        .iter()
        .map(|r| r["name"].as_str().expect("name field").to_string())
        .collect()
}

pub fn find<'a>(list: &'a Value, name: &str) -> &'a Value {
    list.as_array()
        .expect("JSON array")
        .iter()
        .find(|r| r["name"] == name)
        .unwrap_or_else(|| panic!("{name} missing from {list}"))
}
