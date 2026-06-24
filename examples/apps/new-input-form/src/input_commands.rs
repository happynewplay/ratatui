use std::fs;
use std::path::{Path, PathBuf};
use tui_input::{Input, InputRequest};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CommandMode {
    None,
    Files,
    Actions,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CommandKind {
    Files,
    Actions,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PickerStage {
    Root,
    Files,
    Folders,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PickerAction {
    EnterFiles,
    EnterFolders,
    EnterDirectory(PathBuf),
    Insert(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PickerItem {
    pub label: String,
    pub secondary: Option<String>,
    pub action: PickerAction,
}

impl PickerItem {
    pub fn value(&self) -> Option<&str> {
        match &self.action {
            PickerAction::Insert(value) => Some(value.as_str()),
            PickerAction::EnterFiles | PickerAction::EnterFolders | PickerAction::EnterDirectory(_) => None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct CommandPicker {
    pub kind: CommandKind,
    pub stage: PickerStage,
    pub root: PathBuf,
    pub filter: String,
    pub selected: usize,
    pub items: Vec<PickerItem>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PickerOutcome {
    StayOpen,
    Close,
}

impl CommandPicker {
    pub fn files(root: impl Into<PathBuf>) -> Self {
        let root = root.into();
        let mut picker = Self {
            kind: CommandKind::Files,
            stage: PickerStage::Root,
            root,
            filter: String::new(),
            selected: 0,
            items: Vec::new(),
        };
        picker.rebuild_items();
        picker
    }

    pub fn actions() -> Self {
        let mut picker = Self {
            kind: CommandKind::Actions,
            stage: PickerStage::Root,
            root: PathBuf::new(),
            filter: String::new(),
            selected: 0,
            items: Vec::new(),
        };
        picker.rebuild_items();
        picker
    }

    pub fn move_up(&mut self) {
        self.selected = self.selected.saturating_sub(1);
    }

    pub fn move_down(&mut self) {
        let visible = self.visible_items();
        if !visible.is_empty() {
            self.selected = (self.selected + 1).min(visible.len().saturating_sub(1));
        }
    }

    pub fn prompt(&self) -> &str {
        &self.filter
    }

    pub fn push_filter_char(&mut self, ch: char) {
        self.filter.push(ch);
        self.selected = 0;
        if matches!(self.kind, CommandKind::Files) && matches!(self.stage, PickerStage::Root) {
            self.stage = PickerStage::Files;
            self.rebuild_items();
        }
    }

    pub fn pop_filter_char(&mut self) {
        self.filter.pop();
        self.selected = 0;
    }

    pub fn clear_filter(&mut self) {
        self.filter.clear();
        self.selected = 0;
    }

    pub fn visible_items(&self) -> Vec<&PickerItem> {
        if self.filter.is_empty() {
            return self.items.iter().collect();
        }

        let needle = self.filter.to_lowercase();
        let mut matched: Vec<&PickerItem> = self
            .items
            .iter()
            .filter(|item| {
                item.label.to_lowercase().contains(&needle)
                    || item
                        .secondary
                        .as_ref()
                        .map(|secondary| secondary.to_lowercase().contains(&needle))
                        .unwrap_or(false)
            })
            .collect();
        matched.sort_by(|left, right| {
            let left_name = left.label.to_lowercase().contains(&needle);
            let right_name = right.label.to_lowercase().contains(&needle);
            right_name.cmp(&left_name).then_with(|| left.label.cmp(&right.label))
        });
        matched
    }

    pub fn selected_item(&self) -> Option<&str> {
        let visible_items = self.visible_items();
        visible_items
            .get(self.selected)
            .and_then(|item| item.value())
    }

    pub fn activate(&mut self, input: &mut Input) -> PickerOutcome {
        let visible_items = self.visible_items();
        let Some(item) = visible_items.get(self.selected) else {
            return PickerOutcome::StayOpen;
        };

        match &item.action {
            PickerAction::EnterFiles => {
                self.stage = PickerStage::Files;
                self.clear_filter();
                self.rebuild_items();
                PickerOutcome::StayOpen
            }
            PickerAction::EnterFolders => {
                self.stage = PickerStage::Folders;
                self.clear_filter();
                self.rebuild_items();
                PickerOutcome::StayOpen
            }
            PickerAction::EnterDirectory(path) => {
                self.root = path.clone();
                self.stage = PickerStage::Folders;
                self.clear_filter();
                self.rebuild_items();
                PickerOutcome::StayOpen
            }
            PickerAction::Insert(value) => {
                for ch in value.chars() {
                    input.handle(InputRequest::InsertChar(ch));
                }
                if matches!(self.kind, CommandKind::Actions) {
                    input.handle(InputRequest::InsertChar(' '));
                }
                PickerOutcome::Close
            }
        }
    }

    fn rebuild_items(&mut self) {
        self.items = match (self.kind, self.stage) {
            (CommandKind::Files, PickerStage::Root) => vec![
            PickerItem {
                label: "files".to_string(),
                secondary: None,
                action: PickerAction::EnterFiles,
            },
            PickerItem {
                label: "文件夹".to_string(),
                secondary: None,
                action: PickerAction::EnterFolders,
            },
            ],
            (CommandKind::Files, PickerStage::Files) => file_picker_items(&self.root),
            (CommandKind::Files, PickerStage::Folders) => folder_picker_items(&self.root),
            (CommandKind::Actions, _) => vec![
                PickerItem {
                    label: "/plan".to_string(),
                    secondary: None,
                    action: PickerAction::Insert("/plan".to_string()),
                },
                PickerItem {
                    label: "/run".to_string(),
                    secondary: None,
                    action: PickerAction::Insert("/run".to_string()),
                },
                PickerItem {
                    label: "/review".to_string(),
                    secondary: None,
                    action: PickerAction::Insert("/review".to_string()),
                },
            ],
        };
    }
}

fn file_picker_items(root: &Path) -> Vec<PickerItem> {
    list_files(root)
        .into_iter()
        .map(|path| {
            let rel = relative_path(root, &path);
            let file_name = path
                .file_name()
                .map(|name| name.to_string_lossy().to_string())
                .unwrap_or_else(|| rel.display().to_string());
            let value = format!("@{}", rel.display());
            PickerItem {
                label: file_name,
                secondary: Some(rel.display().to_string()),
                action: PickerAction::Insert(value),
            }
        })
        .collect()
}

fn folder_picker_items(root: &Path) -> Vec<PickerItem> {
    list_directories(root)
        .into_iter()
        .map(|path| {
            let rel = relative_path(root, &path);
            let value = format!("@{}/", rel.display());
            PickerItem {
                label: value.clone(),
                secondary: None,
                action: PickerAction::EnterDirectory(path),
            }
        })
        .collect()
}

fn list_directories(root: impl AsRef<Path>) -> Vec<PathBuf> {
    let mut folders = Vec::new();
    collect_directories(root.as_ref(), &mut folders);
    folders.sort();
    folders
}

pub fn list_files(root: impl AsRef<Path>) -> Vec<PathBuf> {
    let mut files = Vec::new();
    collect_files(root.as_ref(), &mut files);
    files.sort();
    files
}

fn collect_files(root: &Path, files: &mut Vec<PathBuf>) {
    if let Ok(entries) = fs::read_dir(root) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                collect_files(&path, files);
            } else if path.is_file() {
                files.push(path);
            }
        }
    }
}

fn collect_directories(root: &Path, folders: &mut Vec<PathBuf>) {
    if let Ok(entries) = fs::read_dir(root) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                folders.push(path);
            }
        }
    }
}

fn relative_path(root: &Path, path: &Path) -> PathBuf {
    path.strip_prefix(root)
        .map(Path::to_path_buf)
        .unwrap_or_else(|_| path.to_path_buf())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};
    use tui_input::Input;

    #[test]
    fn root_files_picker_offers_two_choices() {
        let picker = CommandPicker::files("/workspace");

        assert_eq!(picker.kind, CommandKind::Files);
        assert_eq!(picker.stage, PickerStage::Root);
        assert_eq!(picker.items.len(), 2);
        assert_eq!(picker.items[0].label, "files");
        assert_eq!(picker.items[1].label, "文件夹");
    }

    #[test]
    fn selecting_files_stage_lists_current_path_files() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock before unix epoch")
            .as_nanos();
        let root = std::env::temp_dir().join(format!("new-input-form-stage-{unique}"));
        fs::create_dir_all(&root).expect("create root dir");
        fs::write(root.join("alpha.rs"), "alpha").expect("write alpha");
        fs::write(root.join("beta.rs"), "beta").expect("write beta");

        let mut picker = CommandPicker::files(&root);
        let mut input = Input::default();

        let outcome = picker.activate(&mut input);
        assert_eq!(outcome, PickerOutcome::StayOpen);
        assert_eq!(picker.stage, PickerStage::Files);
        assert!(picker.items.iter().all(|item| item.label == "alpha.rs" || item.label == "beta.rs"));
        assert!(picker
            .items
            .iter()
            .any(|item| item.label == "alpha.rs" && item.secondary.as_deref() == Some("alpha.rs")));
        assert!(picker
            .items
            .iter()
            .any(|item| item.label == "beta.rs" && item.secondary.as_deref() == Some("beta.rs")));
    }

    #[test]
    fn typing_into_root_files_picker_enters_files_stage() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock before unix epoch")
            .as_nanos();
        let root = std::env::temp_dir().join(format!("new-input-form-filter-{unique}"));
        fs::create_dir_all(&root).expect("create root dir");
        fs::write(root.join("l1.rs"), "l1").expect("write l1");
        fs::write(root.join("x.rs"), "x").expect("write x");

        let mut picker = CommandPicker::files(&root);
        picker.push_filter_char('l');

        assert_eq!(picker.stage, PickerStage::Files);
        assert!(picker.visible_items().iter().any(|item| item.label == "l1.rs"));
        assert!(picker
            .visible_items()
            .iter()
            .all(|item| item.label == "l1.rs" || item.label == "x.rs"));
    }

    #[test]
    fn file_filter_matches_secondary_path_fragments() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock before unix epoch")
            .as_nanos();
        let root = std::env::temp_dir().join(format!("new-input-form-path-filter-{unique}"));
        let nested = root.join("docs");
        fs::create_dir_all(&nested).expect("create nested dir");
        fs::write(nested.join("alpha.rs"), "alpha").expect("write alpha");
        fs::write(root.join("beta.rs"), "beta").expect("write beta");

        let mut picker = CommandPicker::files(&root);
        picker.push_filter_char('d');

        assert_eq!(picker.stage, PickerStage::Files);
        let expected_secondary = PathBuf::from("docs").join("alpha.rs").display().to_string();
        assert!(picker
            .visible_items()
            .iter()
            .any(|item| item.label == "alpha.rs" && item.secondary.as_deref() == Some(expected_secondary.as_str())));
        assert!(picker
            .visible_items()
            .iter()
            .all(|item| item.label == "alpha.rs"));
    }

    #[test]
    fn file_matches_from_filename_are_ranked_before_path_only_matches() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock before unix epoch")
            .as_nanos();
        let root = std::env::temp_dir().join(format!("new-input-form-rank-{unique}"));
        let nested = root.join("docs");
        fs::create_dir_all(&nested).expect("create nested dir");
        fs::write(root.join("omega.rs"), "omega").expect("write omega");
        fs::write(nested.join("z.rs"), "z").expect("write z");

        let mut picker = CommandPicker::files(&root);
        picker.push_filter_char('o');

        let visible = picker.visible_items();
        assert_eq!(visible.len(), 2);
        assert_eq!(visible[0].label, "omega.rs");
        assert_eq!(visible[1].label, "z.rs");
        let expected_secondary = PathBuf::from("docs").join("z.rs").display().to_string();
        assert_eq!(visible[1].secondary.as_deref(), Some(expected_secondary.as_str()));
    }

    #[test]
    fn selecting_folder_stage_enters_nested_directory() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock before unix epoch")
            .as_nanos();
        let root = std::env::temp_dir().join(format!("new-input-form-folder-{unique}"));
        let nested = root.join("nested");
        let child = nested.join("child");
        fs::create_dir_all(&child).expect("create child dir");

        let mut picker = CommandPicker::files(&root);
        let mut input = Input::default();

        assert!(matches!(picker.activate(&mut input), PickerOutcome::StayOpen));
        assert_eq!(picker.stage, PickerStage::Files);

        picker = CommandPicker {
            kind: CommandKind::Files,
            stage: PickerStage::Folders,
            root: root.clone(),
            filter: String::new(),
            selected: 0,
            items: folder_picker_items(&root),
        };

        let outcome = picker.activate(&mut input);
        assert_eq!(outcome, PickerOutcome::StayOpen);
        assert_eq!(picker.stage, PickerStage::Folders);
        assert_eq!(picker.root, nested);
        assert!(picker.items.iter().any(|item| item.label == "@child/"));
    }

    #[test]
    fn entering_files_stage_lists_matching_files() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock before unix epoch")
            .as_nanos();
        let root = std::env::temp_dir().join(format!("new-input-form-{unique}"));
        let nested = root.join("nested");
        fs::create_dir_all(&nested).expect("create nested dir");
        fs::write(root.join("alpha.rs"), "alpha").expect("write alpha");
        fs::write(nested.join("beta.rs"), "beta").expect("write beta");

        let mut picker = CommandPicker::files(&root);
        let mut input = Input::default();

        assert!(matches!(picker.items[0].action, PickerAction::EnterFiles));
        picker.activate(&mut input);

        assert_eq!(picker.stage, PickerStage::Files);
        assert!(picker
            .items
            .iter()
            .any(|item| item.label == "alpha.rs" || item.label == "beta.rs"));
    }

    #[test]
    fn files_are_inserted_into_input() {
        let picker = CommandPicker {
            kind: CommandKind::Files,
            stage: PickerStage::Files,
            root: PathBuf::new(),
            filter: String::new(),
            selected: 0,
            items: vec![PickerItem {
                label: "alpha.rs".to_string(),
                secondary: Some("src/alpha.rs".to_string()),
                action: PickerAction::Insert("@alpha.rs".to_string()),
            }],
        };
        let mut input = Input::default();

        let mut picker = picker;
        assert!(matches!(picker.activate(&mut input), PickerOutcome::Close));
        assert_eq!(input.value(), "@alpha.rs");
    }
}
