//! Modal types for the hub: what's currently being asked of the user
//! (`Modal`), and the per-modal-kind tag enums that record *what for*, so
//! `process_modal_confirm` (in `modals.rs`) knows which action to run once
//! the user confirms.

/// One modal currently on screen, or `None` if nothing is open. Each variant
/// is a distinct input shape (free text, a picker list, a checklist, a yes/no
/// prompt, a scrollable read-only message, or a rename/conversion preview);
/// the `tag` field on the input variants records which specific flow is using
/// that shape, since e.g. `Text` is reused for dozens of unrelated prompts.
#[derive(Clone)]
pub enum Modal {
    None,
    Text    { title: String, prompt: String, value: String, tag: TextTag, max_len: Option<usize> },
    Select  { title: String, options: Vec<String>, cursor: usize, tag: SelectTag },
    Multi   { title: String, options: Vec<String>, checks: Vec<bool>, cursor: usize, tag: MultiTag },
    Confirm { message: String, tag: ConfirmTag },
    Info    { title: String, lines: Vec<String>, scroll: usize },
    // Dry-run preview for a codeintel rename / naming-convention conversion —
    // Enter/y applies `plan`, Esc/n/q discards it untouched.
    RenamePreview { title: String, lines: Vec<String>, scroll: usize, plan: crate::codeintel::RenamePlan },
}

impl Modal {
    pub fn is_some(&self) -> bool { !matches!(self, Modal::None) }
}

/// What a `Modal::Text` prompt is *for* — read by `process_modal_confirm` to
/// route the typed value to the right place once the user presses Enter.
#[derive(Clone)]
pub enum TextTag {
    Label(usize),
    ProjName,
    ProjRenameName(usize),
    TodoTitle,
    TodoDesc(String),
    TodoEditTitle(usize),
    TodoEditDesc(usize),
    CloneUrl,
    InitName,
    LinkPath,
    // hub sync
    SyncMsg,
    // hub settings editing
    SetEditor,
    SetClipboard,
    SetWorkspace,
    // import directory
    ImportDir,
    // template creation
    NewTemplateName,
    NewTemplateCmds(String), // template name
    // profiles & sync
    ProfileSaveName,
    ProfileImportJson,
    // Quick Actions panel — new PR / new issue, multi-step prompts
    PrRepo,
    PrTitle(String),
    PrHead(String, String),
    PrBase(String, String, String),
    IssueRepo,
    IssueTitle(String),
    // Code intelligence (Stats tab, [m]) — find usages / rename, scoped to one repo
    CodeIntelFindUsages { repo: std::path::PathBuf, name: String },
    CodeIntelRenameOld  { repo: std::path::PathBuf, name: String },
    CodeIntelRenameNew  { repo: std::path::PathBuf, name: String, old: String },
}

/// One entry in the contextual `[m]` action menu. Most actions already have
/// a key binding, so they re-dispatch a synthesized `KeyEvent` through
/// `handle_global_key` — zero logic duplication. The remaining variants cover
/// flows that have no direct key (the old Quick Actions entries) or lost
/// theirs to the menu itself (CodeIntel's `m` on Stats).
#[derive(Clone)]
pub enum MenuAction {
    Key(crossterm::event::KeyEvent),
    NewPr,
    NewIssue,
    OpenAccountProfile,
    CodeIntelPicker,
    /// `git stash` / `git stash pop` on the repo under the cursor.
    StashSave(usize),
    StashPop(usize),
    /// Write a LICENSE file into every `action_targets()` repo missing one.
    AddLicense,
}

/// What a `Modal::Select` list is *for*, mirroring `TextTag`.
#[derive(Clone)]
pub enum SelectTag {
    ProjMenu,
    ProjForAction(ProjAction),
    TodoStatus { title: String, desc: String },
    TodoProject { title: String, desc: String, status_idx: usize },
    SelectByCondition,
    SortFilter,
    NewRepo,
    // color palette for project create / rename
    ProjColorCreate(String),
    ProjColorRename(usize, String),
    // hub sync action picker
    SyncAction,
    // pull-all account picker
    PullAllAccount,
    // repo creation: pick owner (personal account vs an org), then template
    InitOwner(String),
    InitTemplate { name: String, owner: Option<String> },
    // template management
    TemplateMenu(usize),
    // copy URL: SSH vs HTTPS
    CopyUrl(std::path::PathBuf),
    // linking a folder that already has a .git: keep/publish/discard history
    LinkExisting { src: std::path::PathBuf, name: String },
    // editor / clipboard command pickers (last option "Custom..." falls back to Modal::Text)
    SetEditor,
    SetClipboard,
    // settings profiles & sync menu — carries how many profile entries lead
    // the options list, so the handler knows where the fixed actions start
    ProfileMenu { profile_count: usize },
    // "Settings only" vs "Settings + workspace structure" — asked before any
    // snapshot is taken, since the answer is never reachable from a Confirm
    // modal's "no" branch (Cancel never reaches process_modal_confirm).
    ProfileScope(ProfileScopeAction),
    // Contextual `[m]` action menu — options paired 1:1 with the actions
    ActionMenu(Vec<MenuAction>),
    // add-LICENSE flow: MIT vs Apache-2.0, for the captured target repos
    LicenseKind { targets: Vec<usize> },
    // Code intelligence (Stats tab, [m])
    CodeIntelRepoPick,
    CodeIntelAction { repo: std::path::PathBuf, name: String },
    CodeIntelConvention { repo: std::path::PathBuf, name: String },
}

/// Which settings snapshot a `SelectTag::ProfileScope` answer applies to.
#[derive(Clone)]
pub enum ProfileScopeAction {
    SaveProfile(String),
    CopyJson,
    PushRepo,
}

/// Which bulk operation a `SelectTag::ProjForAction` picker is collecting repos for.
#[derive(Clone, Copy, PartialEq)]
pub enum ProjAction { Assign, Rename, Delete }

/// What a `Modal::Multi` checklist is *for*, mirroring `TextTag`/`SelectTag`.
#[derive(Clone)]
pub enum MultiTag {
    ProjAssign(usize),
    // bulk import: which repo categories to pull, carries the account index
    BulkImportAffiliation(usize),
}

/// What a `Modal::Confirm` yes/no prompt is *for*, mirroring `TextTag`/`SelectTag`.
#[derive(Clone)]
pub enum ConfirmTag {
    DeleteProject(usize),
    DeleteTodo(usize),
    RemoveGitAll,
    DeleteTemplate(usize),
    // write a LICENSE (of the chosen kind) into each target repo
    WriteLicense { kind: crate::license::LicenseKind, targets: Vec<usize> },
}

/// Outcome of one key event while a modal is open, returned by
/// `handle_modal_key` (in `modals.rs`) to tell the caller whether to keep
/// editing, run `process_modal_confirm`, or drop the modal untouched.
pub enum ModalAction { Pending, Confirm, Cancel }
