use crate::document::is_text_document;
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Clone, Debug)]
pub struct Entry {
    pub path: PathBuf,
    pub name: String,
    pub depth: usize,
    pub is_dir: bool,
    pub expanded: bool,
}

#[derive(Default)]
pub struct FileTree {
    root: Option<PathBuf>,
    entries: Vec<Entry>,
    expanded: HashSet<PathBuf>,
}

const SKIPPED_DIRS: &[&str] = &["node_modules", "target", ".git", ".venv", "__pycache__", ".idea", ".vs"];

impl FileTree {
    pub fn root(&self) -> Option<&Path> {
        self.root.as_deref()
    }

    pub fn entries(&self) -> &[Entry] {
        &self.entries
    }

    pub fn set_root(&mut self, root: PathBuf) {
        self.root = Some(root);
        self.expanded.clear();
        self.rebuild();
    }

    pub fn refresh(&mut self) {
        self.rebuild();
    }

    pub fn toggle(&mut self, index: usize) {
        let Some(entry) = self.entries.get(index) else {
            return;
        };
        if !entry.is_dir {
            return;
        }
        if !self.expanded.remove(&entry.path) {
            self.expanded.insert(entry.path.clone());
        }
        self.rebuild();
    }

    pub fn reveal(&mut self, path: &Path) -> Option<usize> {
        let root = self.root.clone()?;
        let mut ancestor = path.parent();
        let mut changed = false;
        while let Some(dir) = ancestor {
            if !dir.starts_with(&root) || dir == root {
                break;
            }
            changed |= self.expanded.insert(dir.to_path_buf());
            ancestor = dir.parent();
        }
        if changed {
            self.rebuild();
        }
        self.entries.iter().position(|e| e.path == path)
    }

    fn rebuild(&mut self) {
        self.entries.clear();
        if let Some(root) = self.root.clone() {
            self.walk(&root, 0);
        }
    }

    fn walk(&mut self, dir: &Path, depth: usize) {
        let Ok(read) = fs::read_dir(dir) else {
            return;
        };
        let mut children: Vec<(bool, String, PathBuf)> = read
            .flatten()
            .filter_map(|entry| {
                let path = entry.path();
                let name = entry.file_name().to_string_lossy().into_owned();
                if name.starts_with('.') {
                    return None;
                }
                let is_dir = entry.file_type().map(|t| t.is_dir()).unwrap_or(false);
                if is_dir && SKIPPED_DIRS.contains(&name.as_str()) {
                    return None;
                }
                if !is_dir && !is_text_document(&path) {
                    return None;
                }
                Some((is_dir, name, path))
            })
            .collect();
        children.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.to_lowercase().cmp(&b.1.to_lowercase())));
        for (is_dir, name, path) in children {
            let expanded = is_dir && self.expanded.contains(&path);
            self.entries.push(Entry { path: path.clone(), name, depth, is_dir, expanded });
            if expanded {
                self.walk(&path, depth + 1);
            }
        }
    }
}
