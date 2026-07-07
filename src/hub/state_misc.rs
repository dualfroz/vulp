//! Small standalone types that don't fit the other state groupings: the
//! repo-list sort order, the Todos tab's status-view/grouping modes, and
//! the user-defined repo template format (persisted to `templates.toml`).

/// Which todo statuses the Todos tab currently shows, cycled with `[s]`.
/// Filters the underlying set *before* grouping — e.g. in `Active`, a
/// `Status` grouping only ever produces "In Progress"/"Open" headers.
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
    /// Whether `status` survives this view's filter.
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

/// How the Todos tab groups its (already status-view-filtered) list,
/// cycled with `[g]`.
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

/// Repo-list ordering for the Repos tab, cycled with a single key. Pinned
/// repos always sort to the top regardless of this setting (see
/// `HubState::rebuild_display`), so this only orders the unpinned remainder.
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

/// A user-defined command shortcut shown in the HubSettings tab and runnable
/// against any repo. Persisted verbatim to `~/.vulp/templates.toml` — these
/// field names are part of that on-disk format, so don't rename them without
/// adding a `#[serde(rename = "...")]` alias for backward compatibility.
#[derive(Clone, serde::Serialize, serde::Deserialize)]
pub struct RepoTemplate {
    pub name:        String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub commands:    Vec<String>,
}
