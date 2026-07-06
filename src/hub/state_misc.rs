//! Small standalone types that don't fit the other state groupings: the
//! repo-list sort order, and the user-defined repo template format
//! (persisted to `templates.toml`).

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
