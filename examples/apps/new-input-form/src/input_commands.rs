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
    Insert(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PickerItem {
    pub label: String,
    pub action: PickerAction,
}

impl PickerItem {
    pub fn label(&self) -> &str {
        &self.label
    }

    pub fn value(&self) -> Option<&str> {
        match &self.action {
            PickerAction::Insert(value) => Some(value.as_str()),
            PickerAction::EnterFiles | PickerAction::EnterFolders => None,
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
        Self {
            kind: CommandKind::Actions,
            stage: PickerStage::Root,
            root: PathBuf::new(),
            filter: String::new(),
            selected: 0,
            items: vec![
                PickerItem {
                    label: "/plan".to_string(),
                    action: PickerAction::Insert("/plan".to_string()),
                },
                PickerItem {
                    label: "/run".to_string(),
                    action: PickerAction::Insert("/run".to_string()),
                },
                PickerItem {
                    label: "/review".to_string(),
                    action: PickerAction::Insert("/review".to_string()),
                },
            ],
        }
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

    pub fn selected_item(&self) -> Option<&str> {
        self.visible_items()
            .get(self.selected)
            .and_then(|item| item.value())
    }

    pub fn prompt(&self) -> &str {
        &self.filter
    }

    pub fn push_filter_char(&mut self, ch: char) {
        self.filter.push(ch);
        self.selected = 0;
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
            return self
                .items
                .iter()
                .filter(|item| self.kind != CommandKind::Files || self.stage == PickerStage::Root || true)
                .collect();
        }
        let needle = self.filter.to_lowercase();
        self.items
            .iter()
            .filter(|item| item.label.to_lowercase().contains(&needle))
            .collect()
    }

    pub fn activate(&mut self, input: &mut Input) -> PickerOutcome {
        let Some(item) = self.visible_items().get(self.selected) else {
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
            PickerAction::Insert(value) => {
                for ch in value.chars() {
                    input.handle(InputRequest::InsertChar(ch));
                }
                if matches!(self.kind, CommandKind::Actions) {
                    input.handle(InputRequest::InsertChar(' '));
                }
                PickerOutcome::Close
            })
        }
    }

    pub fn accept(&self, input: &mut Input) {
        let mut cloned = self.clone();
        let _ = cloned.activate(input);
    }

    fn rebuild_items(&mut self) {
        self.items = match (self.kind, self.stage) {
            (CommandKind::Files, PickerStage::Root) => vec![
                PickerItem {
                    label: "files".to_string(),
                    action: PickerAction::EnterFiles,
                },
                PickerItem {
                    label: "文件夹".to_string(),
                    action: PickerAction::EnterFolders,
                },
            ],
            (CommandKind::Files, PickerStage::Files) => file_picker_items(&self.root),
            (CommandKind::Files, PickerStage::Folders) => folder_picker_items(&self.root),
            (CommandKind::Actions, PickerStage::Root)
            | (CommandKind::Actions, PickerStage::Files)
            | (CommandKind::Actions, PickerStage::Folders) => vec![
                PickerItem {
                    label: "/plan".to_string(),
                    action: PickerAction::Insert("/plan".to_string()),
                },
                PickerItem {
                    label: "/run".to_string(),
                    action: PickerAction::Insert("/run".to_string()),
                },
                PickerItem {
                    label: "/review".to_string(),
                    action: PickerAction::Insert("/review".to_string()),
                },
            ],
        };
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PickerOutcome {
    StayOpen,
    Close,
}

fn file_picker_items(root: &Path) -> Vec<PickerItem> {
    let files = list_files(root);
    if files.is_empty() {
        return vec![PickerItem {
            label: format!("@{}", root.display()),
            action: PickerAction::Insert(format!("@{}", root.display())),
        }];
    }

    files
        .into_iter()
        .map(|path| {
            let rel = relative_path(root, &path);
            let value = format!("@{}", rel.display());
            PickerItem {
                label: value.clone(),
                action: PickerAction::Insert(value),
            }
        })
        .collect()
}

fn folder_picker_items(root: &Path) -> Vec<PickerItem> {
    let folders = list_directories(root);
    if folders.is_empty() {
        return vec![PickerItem {
            label: format!("@{}", root.display()),
            action: PickerAction::Insert(format!("@{}", root.display())),
        }];
    }

    folders
        .into_iter()
        .map(|path| {
            let rel = relative_path(root, &path);
            let value = format!("@{}", rel.display());
            PickerItem {
                label: value.clone(),
                action: PickerAction::Insert(value),
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
                folders.push(path.clone());
                collect_directories(&path, folders);
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
    fn inserts_selected_file_path_into_input() {
        let picker = CommandPicker {
            kind: CommandKind::Files,
            filter: String::new(),
            selected: 1,
            items: vec![
                PickerItem::Entry {
                    label: "@alpha.rs".to_string(),
                    value: "@alpha.rs".to_string(),
                },
                PickerItem::Entry {
                    label: "@beta.rs".to_string(),
                    value: "@beta.rs".to_string(),
                },
            ],
        };
        let mut input = Input::default();

        picker.accept(&mut input);

        assert_eq!(input.value(), "@beta.rs");
    }

    #[test]
    fn list_files_recursively_finds_nested_files() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock before unix epoch")
            .as_nanos();
        let root = std::env::temp_dir().join(format!("new-input-form-{unique}"));
        let nested = root.join("nested");
        fs::create_dir_all(&nested).expect("create nested dir");
        fs::write(root.join("alpha.rs"), "alpha").expect("write alpha");
        fs::write(nested.join("beta.rs"), "beta").expect("write beta");

        let files = list_files(&root);

        assert!(files.iter().any(|path| path.ends_with("alpha.rs")));
        assert!(files
            .iter()
            .any(|path| path.ends_with("nested\\beta.rs") || path.ends_with("nested/beta.rs")));
    }

    #[test]
    fn inserts_selected_action_into_input() {
        let picker = CommandPicker {
            kind: CommandKind::Actions,
            filter: String::new(),
            selected: 1,
            items: vec![
                PickerItem::Entry {
                    label: "/plan".to_string(),
                    value: "/plan".to_string(),
                },
                PickerItem::Entry {
                    label: "/run".to_string(),
                    value: "/run".to_string(),
                },
                PickerItem::Entry {
                    label: "/review".to_string(),
                    value: "/review".to_string(),
                },
            ],
        };
        let mut input = Input::default();

        picker.accept(&mut input);

        assert_eq!(input.value(), "/run ");
    }

    #[test]
    fn filters_visible_items_by_query() {
        let mut picker = CommandPicker {
            kind: CommandKind::Files,
            filter: String::new(),
            selected: 0,
            items: vec![
                PickerItem::Entry {
                    label: "@alpha.rs".to_string(),
                    value: "@alpha.rs".to_string(),
                },
                PickerItem::Section("src".to_string()),
                PickerItem::Entry {
                    label: "@src/main.rs".to_string(),
                    value: "@src/main.rs".to_string(),
                },
                PickerItem::Entry {
                    label: "@src/ui.rs".to_string(),
                    value: "@src/ui.rs".to_string(),
                },
            ],
        };

        picker.push_filter_char('s');
        picker.push_filter_char('r');
        picker.push_filter_char('c');

        assert_eq!(picker.visible_items(), vec!["@src/main.rs", "@src/ui.rs"]);
    }

    #[test]
    fn empty_filter_results_in_empty_visible_items() {
        let mut picker = CommandPicker {
            kind: CommandKind::Actions,
            filter: String::new(),
            selected: 0,
            items: vec![
                PickerItem::Entry {
                    label: "/plan".to_string(),
                    value: "/plan".to_string(),
                },
                PickerItem::Entry {
                    label: "/run".to_string(),
                    value: "/run".to_string(),
                },
            ],
        };

        picker.push_filter_char('z');

        assert!(picker.visible_items().is_empty());
        assert_eq!(picker.selected_item(), None);
    }

    #[test]
    fn files_use_relative_paths_when_possible() {
        let root = PathBuf::from("/workspace");
        let path = PathBuf::from("/workspace/src/main.rs");

        let rel = relative_path(&root, &path);

        assert_eq!(rel, PathBuf::from("src/main.rs"));
    }

    #[test]
    fn file_picker_items_include_recent_and_group_sections() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock before unix epoch")
            .as_nanos();
        let root = std::env::temp_dir().join(format!("new-input-form-picker-{unique}"));
        let src = root.join("src");
        let docs = root.join("docs");
        fs::create_dir_all(&src).expect("create src dir");
        fs::create_dir_all(&docs).expect("create docs dir");
        fs::write(root.join("alpha.rs"), "alpha").expect("write alpha");
        fs::write(src.join("beta.rs"), "beta").expect("write beta");
        fs::write(docs.join("gamma.md"), "gamma").expect("write gamma");

        let items = file_picker_items(&root);

        assert!(items.iter().any(|item| matches!(item, PickerItem::Section(name) if name == "recent")));
        assert!(items.iter().any(|item| matches!(item, PickerItem::Section(name) if name == "all files")));
        assert!(items.iter().any(|item| matches!(item, PickerItem::Section(name) if name == "src")));
        assert!(items.iter().any(|item| matches!(item, PickerItem::Section(name) if name == "docs")));
        assert!(items.iter().any(|item| matches!(item, PickerItem::Entry { value, .. } if value == "@alpha.rs")));
    }
}
