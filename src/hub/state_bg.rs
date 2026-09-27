use std::path::PathBuf;

pub struct GlobalStats {
    pub activity:      Vec<(String, usize)>,
    pub lang_dist:     Vec<(String, usize)>,
    pub top_repos:     Vec<(String, usize)>,
    pub repo_commits:  Vec<(String, usize)>,
    pub total_commits: usize,
    pub dirty_count:   usize,
    pub behind_count:  usize,
    pub day_pattern:   [usize; 7],
    pub own_total:     usize,
    pub own_streak:    (u32, u32),
    pub own_heatmap:   Vec<(chrono::NaiveDate, u32)>,
    pub own_hours:     [u32; 24],
}

pub enum RefreshMsg {
    Progress(String),
    Done {
        metas:         Vec<(String, crate::cache::RepoMeta)>,
        prs:           Vec<(String, Vec<crate::providers::PrDetail>)>,
        issues:        Vec<(String, Vec<crate::providers::IssueDetail>)>,
        closed_counts: Option<(u64, u64, u64)>,
    },
}

#[derive(PartialEq)]
pub enum PhaseResult {
    None,
    OpenDetail,
    ExecSync,
    ExecRelease,
    ExecClone(String),
    ExecOpenEditor(PathBuf),
}

pub enum CreateRepoMsg {
    Progress(String),
    Done { success: bool, name: String, detail: String },
}

pub enum ProfilePullMsg {
    Done(Result<crate::profile::SettingsSnapshot, String>),
}

pub enum CodeIntelMsg {
    Info { title: String, lines: Vec<String> },
    Preview { title: String, lines: Vec<String>, plan: crate::codeintel::RenamePlan },
}

pub struct DigestMsg {
    pub lines: Vec<String>,
}
