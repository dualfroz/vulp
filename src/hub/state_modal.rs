#[derive(Clone)]
pub enum Modal {
    None,
    Text    { title: String, prompt: String, value: String, tag: TextTag, max_len: Option<usize> },
    Select  { title: String, options: Vec<String>, cursor: usize, tag: SelectTag },
    Multi   { title: String, options: Vec<String>, checks: Vec<bool>, cursor: usize, tag: MultiTag },
    Confirm { message: String, tag: ConfirmTag },
    Info    { title: String, lines: Vec<String>, scroll: usize },
    RenamePreview { title: String, lines: Vec<String>, scroll: usize, plan: crate::codeintel::RenamePlan },
}

impl Modal {
    pub fn is_some(&self) -> bool { !matches!(self, Modal::None) }
}

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
    SyncMsg,
    SetEditor,
    SetClipboard,
    SetWorkspace,
    ImportDir,
    NewTemplateName,
    NewTemplateCmds(String),
    ProfileSaveName,
    ProfileImportJson,
    PrRepo,
    PrTitle(String),
    PrHead(String, String),
    PrBase(String, String, String),
    IssueRepo,
    IssueTitle(String),
    CodeIntelFindUsages { repo: std::path::PathBuf, name: String },
    CodeIntelRenameOld  { repo: std::path::PathBuf, name: String },
    CodeIntelRenameNew  { repo: std::path::PathBuf, name: String, old: String },
}

#[derive(Clone)]
pub enum MenuAction {
    Key(crossterm::event::KeyEvent),
    NewPr,
    NewIssue,
    OpenAccountProfile,
    CodeIntelPicker,
    StashSave(usize),
    StashPop(usize),
    AddLicense,
}

#[derive(Clone)]
pub enum SelectTag {
    ProjMenu,
    ProjForAction(ProjAction),
    TodoStatus { title: String, desc: String },
    TodoProject { title: String, desc: String, status_idx: usize },
    SelectByCondition,
    SortFilter,
    NewRepo,
    ProjColorCreate(String),
    ProjColorRename(usize, String),
    SyncAction,
    PullAllAccount,
    InitOwner(String),
    InitTemplate { name: String, owner: Option<String> },
    TemplateMenu(usize),
    CopyUrl(std::path::PathBuf),
    LinkExisting { src: std::path::PathBuf, name: String },
    SetEditor,
    SetClipboard,
    ProfileMenu { profile_count: usize },
    ProfileScope(ProfileScopeAction),
    ActionMenu(Vec<MenuAction>),
    LicenseKind { targets: Vec<usize> },
    CodeIntelRepoPick,
    CodeIntelAction { repo: std::path::PathBuf, name: String },
    CodeIntelConvention { repo: std::path::PathBuf, name: String },
}

#[derive(Clone)]
pub enum ProfileScopeAction {
    SaveProfile(String),
    CopyJson,
    PushRepo,
}

#[derive(Clone, Copy, PartialEq)]
pub enum ProjAction { Assign, Rename, Delete }

#[derive(Clone)]
pub enum MultiTag {
    ProjAssign(usize),
    BulkImportAffiliation(usize),
}

#[derive(Clone)]
pub enum ConfirmTag {
    DeleteProject(usize),
    DeleteTodo(usize),
    RemoveGitAll,
    DeleteTemplate(usize),
    WriteLicense { kind: crate::license::LicenseKind, targets: Vec<usize> },
}

pub enum ModalAction { Pending, Confirm, Cancel }
