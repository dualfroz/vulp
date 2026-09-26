#[derive(Clone, Copy, PartialEq, Eq, Default)]
pub enum TodoStatusView {
    #[default]
    Active,
    OpenOnly,
    DoneOnly,
    All,
}

impl TodoStatusView {
    pub fn label(self) -> &'static str {
        match self {
            Self::Active   => "active",
            Self::OpenOnly => "open",
            Self::DoneOnly => "done",
            Self::All      => "all",
        }
    }
    pub fn next(self) -> Self {
        match self {
            Self::Active   => Self::OpenOnly,
            Self::OpenOnly => Self::DoneOnly,
            Self::DoneOnly => Self::All,
            Self::All      => Self::Active,
        }
    }
    pub fn allows(self, status: &crate::todos::TodoStatus) -> bool {
        use crate::todos::TodoStatus as S;
        match self {
            Self::Active   => matches!(status, S::Open | S::InProgress),
            Self::OpenOnly => matches!(status, S::Open),
            Self::DoneOnly => matches!(status, S::Done),
            Self::All      => true,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Default)]
pub enum TodoGroupBy {
    #[default]
    Status,
    Project,
    Repo,
    None,
}

impl TodoGroupBy {
    pub fn label(self) -> &'static str {
        match self {
            Self::Status  => "status",
            Self::Project => "project",
            Self::Repo    => "repo",
            Self::None    => "none",
        }
    }
    pub fn next(self) -> Self {
        match self {
            Self::Status  => Self::Project,
            Self::Project => Self::Repo,
            Self::Repo    => Self::None,
            Self::None    => Self::Status,
        }
    }
}

#[derive(Clone, Copy, PartialEq)]
pub enum SortMode { Default, Name, Age, DirtyFirst, BehindFirst }

impl SortMode {
    pub fn label(self) -> &'static str {
        match self {
            Self::Default    => "Default",
            Self::Name       => "Name",
            Self::Age        => "Recent",
            Self::DirtyFirst => "Dirty↑",
            Self::BehindFirst=> "Behind↑",
        }
    }
}

#[derive(Clone, serde::Serialize, serde::Deserialize)]
pub struct RepoTemplate {
    pub name:        String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub commands:    Vec<String>,
}
