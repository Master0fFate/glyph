use crate::document::{self, Snapshot, is_text_document, read_document, save_document, save_new};
use crate::editor::{Editor, EditorEvent, EditorMode};
use crate::files::FileTree;
use crate::markdown::{self, Document};
use crate::preview::PreviewContext;
use crate::settings::{Layout, Settings};
use crate::theme::Theme;
use gpui::{
    AnyElement, App, Context, Entity, FocusHandle, Focusable, FontWeight, MouseButton, PathPromptOptions, PromptLevel,
    ScrollHandle, ScrollStrategy, Subscription, Task, UniformListScrollHandle, Window, WindowBounds,
    WindowControlArea, actions, div, prelude::*, px, svg, uniform_list,
};
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::time::Duration;

actions!(
    workspace,
    [
        NewDocument,
        OpenFile,
        OpenFolder,
        Save,
        SaveAs,
        CloseTab,
        NextTab,
        PreviousTab,
        ToggleSidebar,
        ToggleTheme,
        ViewWrite,
        ViewSplit,
        ViewRead,
        Find,
        Replace,
        GoToLine,
        CommandPalette,
        ToggleFocusMode,
        ToggleWrap,
        ToggleLineNumbers,
        ToggleStatusBar,
        ZoomIn,
        ZoomOut,
        ZoomReset,
        ExportHtml,
        Quit,
        FindNext,
        FindPrevious,
        CloseOverlay,
    ]
);

const SIDEBAR_WIDTH: f32 = 260.0;
const TITLEBAR_HEIGHT: f32 = 44.0;

struct Tab {
    id: usize,
    editor: Entity<Editor>,
    snapshot: Option<Snapshot>,
    title: String,
    document: Rc<Document>,
    parsed_version: Option<u64>,
    word_count: usize,
    word_count_version: Option<u64>,
    _subscription: Subscription,
}

impl Tab {
    fn dirty(&self, cx: &App) -> bool {
        self.editor.read(cx).buffer.is_dirty()
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Overlay {
    None,
    Find,
    GoToLine,
    Palette,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum SidebarTab {
    Files,
    Outline,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum NoticeKind {
    Info,
    Error,
}

struct Command {
    label: &'static str,
    keys: &'static str,
    run: fn(&mut Workspace, &mut Window, &mut Context<Workspace>),
}

const COMMANDS: &[Command] = &[
    Command { label: "New document", keys: "Ctrl+N", run: |w, window, cx| w.new_document(&NewDocument, window, cx) },
    Command { label: "Open file", keys: "Ctrl+O", run: |w, window, cx| w.open_file(&OpenFile, window, cx) },
    Command { label: "Open folder", keys: "Ctrl+Shift+O", run: |w, window, cx| w.open_folder(&OpenFolder, window, cx) },
    Command { label: "Save", keys: "Ctrl+S", run: |w, window, cx| w.save(&Save, window, cx) },
    Command { label: "Save as", keys: "Ctrl+Shift+S", run: |w, window, cx| w.save_as(&SaveAs, window, cx) },
    Command { label: "Close document", keys: "Ctrl+W", run: |w, window, cx| w.close_tab(&CloseTab, window, cx) },
    Command { label: "Export HTML", keys: "Ctrl+Shift+E", run: |w, window, cx| w.export_html(&ExportHtml, window, cx) },
    Command { label: "View: Write", keys: "Ctrl+1", run: |w, window, cx| w.set_layout(Layout::Write, window, cx) },
    Command { label: "View: Split", keys: "Ctrl+2", run: |w, window, cx| w.set_layout(Layout::Split, window, cx) },
    Command { label: "View: Read", keys: "Ctrl+3", run: |w, window, cx| w.set_layout(Layout::Read, window, cx) },
    Command { label: "Toggle sidebar", keys: "Ctrl+\\", run: |w, window, cx| w.toggle_sidebar(&ToggleSidebar, window, cx) },
    Command { label: "Toggle light / dark theme", keys: "Ctrl+Shift+L", run: |w, window, cx| w.toggle_theme(&ToggleTheme, window, cx) },
    Command { label: "Toggle focus mode", keys: "F11", run: |w, window, cx| w.toggle_focus(&ToggleFocusMode, window, cx) },
    Command { label: "Toggle word wrap", keys: "Alt+Z", run: |w, window, cx| w.toggle_wrap(&ToggleWrap, window, cx) },
    Command { label: "Toggle line numbers", keys: "", run: |w, window, cx| w.toggle_line_numbers(&ToggleLineNumbers, window, cx) },
    Command { label: "Toggle status bar", keys: "", run: |w, window, cx| w.toggle_status(&ToggleStatusBar, window, cx) },
    Command { label: "Find", keys: "Ctrl+F", run: |w, window, cx| w.find(&Find, window, cx) },
    Command { label: "Find and replace", keys: "Ctrl+H", run: |w, window, cx| w.replace(&Replace, window, cx) },
    Command { label: "Go to line", keys: "Ctrl+G", run: |w, window, cx| w.go_to_line(&GoToLine, window, cx) },
    Command { label: "Insert code block", keys: "Ctrl+Shift+C", run: |w, window, cx| w.with_editor(window, cx, |e, cx| e.insert_block("", cx)) },
    Command { label: "Insert Mermaid diagram", keys: "Ctrl+Shift+M", run: |w, window, cx| w.with_editor(window, cx, |e, cx| e.insert_block("mermaid", cx)) },
    Command { label: "Bold", keys: "Ctrl+B", run: |w, window, cx| w.with_editor(window, cx, |e, cx| e.wrap_selection("**", "**", "bold", cx)) },
    Command { label: "Italic", keys: "Ctrl+I", run: |w, window, cx| w.with_editor(window, cx, |e, cx| e.wrap_selection("*", "*", "italic", cx)) },
    Command { label: "Inline code", keys: "Ctrl+E", run: |w, window, cx| w.with_editor(window, cx, |e, cx| e.wrap_selection("`", "`", "code", cx)) },
    Command { label: "Zoom in", keys: "Ctrl+=", run: |w, window, cx| w.zoom_in(&ZoomIn, window, cx) },
    Command { label: "Zoom out", keys: "Ctrl+-", run: |w, window, cx| w.zoom_out(&ZoomOut, window, cx) },
    Command { label: "Reset zoom", keys: "Ctrl+0", run: |w, window, cx| w.zoom_reset(&ZoomReset, window, cx) },
    Command { label: "Quit", keys: "Ctrl+Q", run: |w, window, cx| w.quit(&Quit, window, cx) },
];

pub struct Workspace {
    tabs: Vec<Tab>,
    active: usize,
    next_tab_id: usize,
    layout: Layout,
    sidebar_open: bool,
    sidebar_tab: SidebarTab,
    status_visible: bool,
    focus_mode: bool,
    line_numbers: bool,
    wrap: bool,
    font_size: f32,
    files: FileTree,
    file_scroll: UniformListScrollHandle,
    outline_scroll: UniformListScrollHandle,
    preview_scroll: ScrollHandle,
    last_sync_row: Option<usize>,
    overlay: Overlay,
    replace_visible: bool,
    find_input: Entity<Editor>,
    replace_input: Entity<Editor>,
    goto_input: Entity<Editor>,
    palette_input: Entity<Editor>,
    palette_index: usize,
    notice: Option<(String, NoticeKind)>,
    notice_task: Option<Task<()>>,
    parse_task: Option<Task<()>>,
    focus_handle: FocusHandle,
    closing: bool,
    _input_subscriptions: Vec<Subscription>,
}

impl Workspace {
    pub fn new(settings: &Settings, paths: Vec<PathBuf>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let find_input = cx.new(|cx| Editor::single_line("Find", cx));
        let replace_input = cx.new(|cx| Editor::single_line("Replace with", cx));
        let goto_input = cx.new(|cx| Editor::single_line("Line number", cx));
        let palette_input = cx.new(|cx| Editor::single_line("Type a command", cx));
        let mut subscriptions = Vec::new();
        subscriptions.push(cx.subscribe_in(&find_input, window, |this, _, event, window, cx| match event {
            EditorEvent::Edited => this.update_search(cx),
            EditorEvent::Submit => this.find_next(&FindNext, window, cx),
            EditorEvent::SubmitReverse => this.find_previous(&FindPrevious, window, cx),
            EditorEvent::Cancel => this.close_overlay(&CloseOverlay, window, cx),
            _ => {}
        }));
        subscriptions.push(cx.subscribe_in(&replace_input, window, |this, _, event, window, cx| match event {
            EditorEvent::Submit => this.replace_current(window, cx),
            EditorEvent::Cancel => this.close_overlay(&CloseOverlay, window, cx),
            _ => {}
        }));
        subscriptions.push(cx.subscribe_in(&goto_input, window, |this, _, event, window, cx| match event {
            EditorEvent::Submit => this.submit_go_to_line(window, cx),
            EditorEvent::Cancel => this.close_overlay(&CloseOverlay, window, cx),
            _ => {}
        }));
        subscriptions.push(cx.subscribe_in(&palette_input, window, |this, _, event, window, cx| match event {
            EditorEvent::Edited => {
                this.palette_index = 0;
                cx.notify();
            }
            EditorEvent::Navigate(delta) => this.palette_move(*delta, cx),
            EditorEvent::Submit => this.palette_run(window, cx),
            EditorEvent::Cancel => this.close_overlay(&CloseOverlay, window, cx),
            _ => {}
        }));
        let mut workspace = Self {
            tabs: Vec::new(),
            active: 0,
            next_tab_id: 1,
            layout: settings.layout,
            sidebar_open: settings.sidebar,
            sidebar_tab: SidebarTab::Files,
            status_visible: settings.status_bar,
            focus_mode: false,
            line_numbers: settings.line_numbers,
            wrap: settings.wrap,
            font_size: settings.font_size.clamp(10., 28.),
            files: FileTree::default(),
            file_scroll: UniformListScrollHandle::new(),
            outline_scroll: UniformListScrollHandle::new(),
            preview_scroll: ScrollHandle::new(),
            last_sync_row: None,
            overlay: Overlay::None,
            replace_visible: false,
            find_input,
            replace_input,
            goto_input,
            palette_input,
            palette_index: 0,
            notice: None,
            notice_task: None,
            parse_task: None,
            focus_handle: cx.focus_handle(),
            closing: false,
            _input_subscriptions: subscriptions,
        };
        let folder = settings.workspace.clone().filter(|p| p.is_dir());
        if let Some(folder) = folder {
            workspace.files.set_root(folder);
        }
        let restore = paths.is_empty();
        let mut initial: Vec<PathBuf> = paths;
        if restore {
            initial = settings.open_files.iter().filter(|p| p.is_file()).cloned().collect();
        }
        for path in initial {
            if path.is_dir() {
                workspace.files.set_root(path);
                workspace.sidebar_open = true;
                continue;
            }
            workspace.open_path(&path, window, cx);
        }
        if workspace.tabs.is_empty() {
            workspace.add_tab("Untitled".to_owned(), "", None, window, cx);
        }
        if workspace.files.root().is_none()
            && let Some(parent) = workspace.tabs.first().and_then(|t| t.snapshot.as_ref()).and_then(|s| s.path.parent())
        {
            workspace.files.set_root(parent.to_path_buf());
        }
        workspace.activate_tab(0, window, cx);
        let weak = cx.weak_entity();
        window.on_window_should_close(cx, move |window, cx| weak.update(cx, |this, cx| this.request_close(window, cx)).unwrap_or(true));
        workspace
    }

    fn active_tab(&self) -> Option<&Tab> {
        self.tabs.get(self.active)
    }

    fn active_editor(&self) -> Option<Entity<Editor>> {
        self.active_tab().map(|t| t.editor.clone())
    }

    fn with_editor(&mut self, window: &mut Window, cx: &mut Context<Self>, f: impl FnOnce(&mut Editor, &mut Context<Editor>)) {
        if let Some(editor) = self.active_editor() {
            editor.update(cx, |editor, cx| {
                editor.focus(window, cx);
                f(editor, cx);
            });
        }
    }

    fn add_tab(&mut self, title: String, text: &str, snapshot: Option<Snapshot>, window: &mut Window, cx: &mut Context<Self>) -> usize {
        let id = self.next_tab_id;
        self.next_tab_id += 1;
        let font_size = self.font_size;
        let line_numbers = self.line_numbers;
        let wrap = self.wrap;
        let editor = cx.new(|cx| {
            let mut editor = Editor::new(text, EditorMode::Document, cx);
            editor.font_size = px(font_size);
            editor.line_numbers = line_numbers;
            editor.wrap = wrap && editor.wrap;
            editor.placeholder = "Start writing…  Ctrl+O opens a file, Ctrl+2 shows the preview.".into();
            editor
        });
        let subscription = cx.subscribe_in(&editor, window, move |this, _, event, _, cx| match event {
            EditorEvent::Edited => this.schedule_parse(cx),
            EditorEvent::SelectionChanged => cx.notify(),
            _ => {}
        });
        let document = Rc::new(markdown::parse(text));
        self.tabs.push(Tab { id, editor, snapshot, title, document, parsed_version: Some(0), word_count: 0, word_count_version: None, _subscription: subscription });
        self.tabs.len() - 1
    }

    fn activate_tab(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        if self.tabs.is_empty() {
            return;
        }
        self.active = index.min(self.tabs.len() - 1);
        self.last_sync_row = None;
        let tab = &self.tabs[self.active];
        let title = format!("{} — Glyph", tab.title);
        window.set_window_title(&title);
        tab.editor.update(cx, |editor, cx| editor.focus(window, cx));
        self.ensure_parsed(cx);
        cx.notify();
    }

    pub fn open_path(&mut self, path: &Path, window: &mut Window, cx: &mut Context<Self>) {
        let canonical = document::canonical(path);
        if let Some(ix) = self.tabs.iter().position(|t| t.snapshot.as_ref().is_some_and(|s| s.path == canonical)) {
            self.activate_tab(ix, window, cx);
            return;
        }
        match read_document(&canonical) {
            Ok((text, snapshot)) => {
                let title = snapshot.path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| "Untitled".into());
                let pristine_untitled = self.tabs.len() == 1 && self.tabs[0].snapshot.is_none() && !self.tabs[0].dirty(cx) && self.tabs[0].editor.read(cx).buffer.line_count() == 1 && self.tabs[0].editor.read(cx).buffer.line(0).is_empty();
                if pristine_untitled {
                    self.tabs.clear();
                }
                let ix = self.add_tab(title, &text, Some(snapshot), window, cx);
                self.activate_tab(ix, window, cx);
                if let Some(entry) = self.files.reveal(&canonical) {
                    self.file_scroll.scroll_to_item(entry, ScrollStrategy::Center);
                }
            }
            Err(error) => self.show_notice(format!("{error:#}"), NoticeKind::Error, cx),
        }
    }

    fn schedule_parse(&mut self, cx: &mut Context<Self>) {
        cx.notify();
        self.parse_task = Some(cx.spawn(async move |this, cx| {
            cx.background_executor().timer(Duration::from_millis(140)).await;
            this.update(cx, |this, cx| this.ensure_parsed(cx)).ok();
        }));
    }

    fn ensure_parsed(&mut self, cx: &mut Context<Self>) {
        let Some(tab) = self.tabs.get_mut(self.active) else {
            return;
        };
        let version = tab.editor.read(cx).buffer.version();
        if tab.parsed_version == Some(version) {
            return;
        }
        let text = tab.editor.read(cx).text();
        tab.document = Rc::new(markdown::parse(&text));
        tab.parsed_version = Some(version);
        cx.notify();
    }

    fn show_notice(&mut self, text: String, kind: NoticeKind, cx: &mut Context<Self>) {
        self.notice = Some((text, kind));
        cx.notify();
        self.notice_task = Some(cx.spawn(async move |this, cx| {
            cx.background_executor().timer(Duration::from_millis(4200)).await;
            this.update(cx, |this, cx| {
                this.notice = None;
                cx.notify();
            })
            .ok();
        }));
    }

    fn persist(&self, window: &Window, cx: &App) -> Settings {
        let bounds = match window.window_bounds() {
            WindowBounds::Windowed(bounds) => Some([bounds.origin.x.as_f32(), bounds.origin.y.as_f32(), bounds.size.width.as_f32(), bounds.size.height.as_f32()]),
            _ => None,
        };
        Settings {
            theme: cx.global::<Theme>().appearance,
            layout: self.layout,
            sidebar: self.sidebar_open,
            status_bar: self.status_visible,
            line_numbers: self.line_numbers,
            wrap: self.wrap,
            font_size: self.font_size,
            workspace: self.files.root().map(Path::to_path_buf),
            open_files: self.tabs.iter().filter_map(|t| t.snapshot.as_ref().map(|s| s.path.clone())).collect(),
            window: bounds,
        }
    }

    fn save_settings(&self, window: &Window, cx: &App) {
        self.persist(window, cx).save();
    }

    fn request_close(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        if self.closing || !self.tabs.iter().any(|t| t.dirty(cx)) {
            self.save_settings(window, cx);
            return true;
        }
        let count = self.tabs.iter().filter(|t| t.dirty(cx)).count();
        let message = if count == 1 { "One document has unsaved changes.".to_owned() } else { format!("{count} documents have unsaved changes.") };
        let answer = window.prompt(PromptLevel::Warning, &message, Some("Closing now discards those changes."), &["Discard and close", "Keep editing"], cx);
        cx.spawn_in(window, async move |this, cx| {
            if answer.await == Ok(0) {
                this.update_in(cx, |this, window, cx| {
                    this.closing = true;
                    this.save_settings(window, cx);
                    window.remove_window();
                })
                .ok();
            }
        })
        .detach();
        false
    }

    fn new_document(&mut self, _: &NewDocument, window: &mut Window, cx: &mut Context<Self>) {
        let ix = self.add_tab("Untitled".to_owned(), "", None, window, cx);
        self.activate_tab(ix, window, cx);
    }

    fn open_file(&mut self, _: &OpenFile, window: &mut Window, cx: &mut Context<Self>) {
        let receiver = cx.prompt_for_paths(PathPromptOptions { files: true, directories: false, multiple: true, prompt: None });
        cx.spawn_in(window, async move |this, cx| {
            if let Ok(Ok(Some(paths))) = receiver.await {
                this.update_in(cx, |this, window, cx| {
                    for path in paths {
                        this.open_path(&path, window, cx);
                    }
                })
                .ok();
            }
        })
        .detach();
    }

    fn open_folder(&mut self, _: &OpenFolder, window: &mut Window, cx: &mut Context<Self>) {
        let receiver = cx.prompt_for_paths(PathPromptOptions { files: false, directories: true, multiple: false, prompt: None });
        cx.spawn_in(window, async move |this, cx| {
            if let Ok(Ok(Some(paths))) = receiver.await
                && let Some(folder) = paths.into_iter().next()
            {
                this.update_in(cx, |this, _, cx| {
                    this.files.set_root(folder);
                    this.sidebar_open = true;
                    this.sidebar_tab = SidebarTab::Files;
                    cx.notify();
                })
                .ok();
            }
        })
        .detach();
    }

    fn save(&mut self, _: &Save, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(tab) = self.active_tab() {
            self.save_tab(tab.id, false, window, cx);
        }
    }

    fn save_tab(&mut self, id: usize, force: bool, window: &mut Window, cx: &mut Context<Self>) {
        let Some(ix) = self.tabs.iter().position(|t| t.id == id) else {
            return;
        };
        let Some(snapshot) = self.tabs[ix].snapshot.clone() else {
            self.save_tab_as(id, window, cx);
            return;
        };
        let text = self.tabs[ix].editor.read(cx).text();
        match save_document(&snapshot, &text, force) {
            Ok(snapshot) => {
                self.tabs[ix].snapshot = Some(snapshot);
                self.tabs[ix].editor.update(cx, |editor, cx| {
                    editor.buffer.mark_saved();
                    cx.notify();
                });
                self.files.refresh();
                self.show_notice(format!("Saved {}", self.tabs[ix].title), NoticeKind::Info, cx);
            }
            Err(error) => {
                let message = format!("{error:#}");
                if message.contains("outside Glyph") {
                    let answer = window.prompt(PromptLevel::Warning, &message, Some("Overwrite the file on disk with this document?"), &["Overwrite", "Cancel"], cx);
                    cx.spawn_in(window, async move |this, cx| {
                        if answer.await == Ok(0) {
                            this.update_in(cx, |this, window, cx| this.save_tab(id, true, window, cx)).ok();
                        }
                    })
                    .detach();
                } else {
                    self.show_notice(message, NoticeKind::Error, cx);
                }
            }
        }
    }

    fn save_as(&mut self, _: &SaveAs, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(tab) = self.active_tab() {
            self.save_tab_as(tab.id, window, cx);
        }
    }

    fn save_tab_as(&mut self, id: usize, window: &mut Window, cx: &mut Context<Self>) {
        let Some(tab) = self.tabs.iter().find(|t| t.id == id) else {
            return;
        };
        let directory = tab
            .snapshot
            .as_ref()
            .and_then(|s| s.path.parent().map(Path::to_path_buf))
            .or_else(|| self.files.root().map(Path::to_path_buf))
            .or_else(|| directories::UserDirs::new().and_then(|d| d.document_dir().map(Path::to_path_buf)))
            .unwrap_or_else(|| PathBuf::from("."));
        let suggested = if tab.snapshot.is_some() { tab.title.clone() } else { format!("{}.md", tab.title.to_lowercase().replace(' ', "-")) };
        let receiver = cx.prompt_for_new_path(&directory, Some(&suggested));
        cx.spawn_in(window, async move |this, cx| {
            if let Ok(Ok(Some(path))) = receiver.await {
                this.update_in(cx, |this, _, cx| this.finish_save_as(id, path, cx)).ok();
            }
        })
        .detach();
    }

    fn finish_save_as(&mut self, id: usize, path: PathBuf, cx: &mut Context<Self>) {
        let Some(ix) = self.tabs.iter().position(|t| t.id == id) else {
            return;
        };
        let path = if path.extension().is_none() { path.with_extension("md") } else { path };
        let text = self.tabs[ix].editor.read(cx).text();
        match save_new(&path, &text) {
            Ok(snapshot) => {
                self.tabs[ix].title = snapshot.path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
                self.tabs[ix].snapshot = Some(snapshot);
                self.tabs[ix].editor.update(cx, |editor, cx| {
                    editor.buffer.mark_saved();
                    cx.notify();
                });
                if self.files.root().is_none()
                    && let Some(parent) = path.parent()
                {
                    self.files.set_root(parent.to_path_buf());
                }
                self.files.refresh();
                self.show_notice(format!("Saved {}", self.tabs[ix].title), NoticeKind::Info, cx);
            }
            Err(error) => self.show_notice(format!("{error:#}"), NoticeKind::Error, cx),
        }
        cx.notify();
    }

    fn close_tab(&mut self, _: &CloseTab, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(tab) = self.active_tab() {
            self.close_tab_by_id(tab.id, false, window, cx);
        }
    }

    fn close_tab_by_id(&mut self, id: usize, force: bool, window: &mut Window, cx: &mut Context<Self>) {
        let Some(ix) = self.tabs.iter().position(|t| t.id == id) else {
            return;
        };
        if !force && self.tabs[ix].dirty(cx) {
            let title = self.tabs[ix].title.clone();
            let answer = window.prompt(PromptLevel::Warning, &format!("Save changes to {title}?"), Some("Your changes will be lost if you don't save them."), &["Save", "Don't save", "Cancel"], cx);
            cx.spawn_in(window, async move |this, cx| match answer.await {
                Ok(0) => {
                    this.update_in(cx, |this, window, cx| {
                        this.save_tab(id, false, window, cx);
                        if this.tabs.iter().find(|t| t.id == id).is_some_and(|t| !t.dirty(cx)) {
                            this.close_tab_by_id(id, true, window, cx);
                        }
                    })
                    .ok();
                }
                Ok(1) => {
                    this.update_in(cx, |this, window, cx| this.close_tab_by_id(id, true, window, cx)).ok();
                }
                _ => {}
            })
            .detach();
            return;
        }
        self.tabs.remove(ix);
        if self.tabs.is_empty() {
            self.add_tab("Untitled".to_owned(), "", None, window, cx);
        }
        let next = if ix <= self.active { self.active.saturating_sub(if ix < self.active { 1 } else { 0 }) } else { self.active };
        self.activate_tab(next.min(self.tabs.len() - 1), window, cx);
    }

    fn next_tab(&mut self, _: &NextTab, window: &mut Window, cx: &mut Context<Self>) {
        let next = (self.active + 1) % self.tabs.len().max(1);
        self.activate_tab(next, window, cx);
    }

    fn previous_tab(&mut self, _: &PreviousTab, window: &mut Window, cx: &mut Context<Self>) {
        let count = self.tabs.len().max(1);
        self.activate_tab((self.active + count - 1) % count, window, cx);
    }

    fn toggle_sidebar(&mut self, _: &ToggleSidebar, _: &mut Window, cx: &mut Context<Self>) {
        self.sidebar_open = !self.sidebar_open;
        self.focus_mode = false;
        cx.notify();
    }

    fn toggle_theme(&mut self, _: &ToggleTheme, _: &mut Window, cx: &mut Context<Self>) {
        let current = cx.global::<Theme>().clone();
        cx.set_global(Theme::new(current.appearance.toggled(), current.sans, current.mono));
        self.invalidate_editors(cx);
    }

    fn invalidate_editors(&mut self, cx: &mut Context<Self>) {
        let line_numbers = self.line_numbers;
        let wrap = self.wrap;
        let font_size = self.font_size;
        for tab in &self.tabs {
            tab.editor.update(cx, |editor, cx| {
                editor.line_numbers = line_numbers;
                editor.wrap = wrap;
                editor.font_size = px(font_size);
                editor.invalidate_layout(cx);
            });
        }
        for input in [&self.find_input, &self.replace_input, &self.goto_input, &self.palette_input] {
            input.update(cx, |editor, cx| editor.invalidate_layout(cx));
        }
        cx.notify();
    }

    fn set_layout(&mut self, layout: Layout, window: &mut Window, cx: &mut Context<Self>) {
        self.layout = layout;
        self.last_sync_row = None;
        self.ensure_parsed(cx);
        if layout != Layout::Read {
            self.with_editor(window, cx, |_, _| {});
        }
        cx.notify();
    }

    fn view_write(&mut self, _: &ViewWrite, window: &mut Window, cx: &mut Context<Self>) {
        self.set_layout(Layout::Write, window, cx);
    }

    fn view_split(&mut self, _: &ViewSplit, window: &mut Window, cx: &mut Context<Self>) {
        self.set_layout(Layout::Split, window, cx);
    }

    fn view_read(&mut self, _: &ViewRead, window: &mut Window, cx: &mut Context<Self>) {
        self.set_layout(Layout::Read, window, cx);
    }

    fn open_overlay(&mut self, overlay: Overlay, window: &mut Window, cx: &mut Context<Self>) {
        self.overlay = overlay;
        let input = match overlay {
            Overlay::Find => Some(&self.find_input),
            Overlay::GoToLine => Some(&self.goto_input),
            Overlay::Palette => Some(&self.palette_input),
            Overlay::None => None,
        };
        if let Some(input) = input {
            input.update(cx, |editor, cx| {
                let end = editor.buffer.end();
                editor.select_range(Default::default()..end, cx);
                editor.focus(window, cx);
            });
        }
        cx.notify();
    }

    fn find(&mut self, _: &Find, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(editor) = self.active_editor() {
            let selected = editor.read(cx).selected_text();
            if !selected.is_empty() && !selected.contains('\n') {
                self.find_input.update(cx, |input, cx| input.replace_all_text(&selected, cx));
            }
        }
        self.open_overlay(Overlay::Find, window, cx);
        self.update_search(cx);
    }

    fn replace(&mut self, _: &Replace, window: &mut Window, cx: &mut Context<Self>) {
        self.replace_visible = true;
        self.find(&Find, window, cx);
    }

    fn update_search(&mut self, cx: &mut Context<Self>) {
        let query = self.find_input.read(cx).text();
        let query = if self.overlay == Overlay::Find { query } else { String::new() };
        if let Some(editor) = self.active_editor() {
            editor.update(cx, |editor, cx| editor.set_search(&query, cx));
        }
        cx.notify();
    }

    fn find_next(&mut self, _: &FindNext, _: &mut Window, cx: &mut Context<Self>) {
        if let Some(editor) = self.active_editor() {
            editor.update(cx, |editor, cx| editor.search_step(true, cx));
        }
    }

    fn find_previous(&mut self, _: &FindPrevious, _: &mut Window, cx: &mut Context<Self>) {
        if let Some(editor) = self.active_editor() {
            editor.update(cx, |editor, cx| editor.search_step(false, cx));
        }
    }

    fn replace_current(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        let replacement = self.replace_input.read(cx).text();
        if let Some(editor) = self.active_editor() {
            editor.update(cx, |editor, cx| editor.replace_current(&replacement, cx));
        }
    }

    fn replace_everything(&mut self, cx: &mut Context<Self>) {
        let replacement = self.replace_input.read(cx).text();
        if let Some(editor) = self.active_editor() {
            let count = editor.update(cx, |editor, cx| editor.replace_all(&replacement, cx));
            self.show_notice(format!("Replaced {count} occurrence{}", if count == 1 { "" } else { "s" }), NoticeKind::Info, cx);
        }
    }

    fn go_to_line(&mut self, _: &GoToLine, window: &mut Window, cx: &mut Context<Self>) {
        self.goto_input.update(cx, |input, cx| input.replace_all_text("", cx));
        self.open_overlay(Overlay::GoToLine, window, cx);
    }

    fn submit_go_to_line(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let text = self.goto_input.read(cx).text();
        if let Ok(line) = text.trim().parse::<usize>()
            && let Some(editor) = self.active_editor()
        {
            editor.update(cx, |editor, cx| editor.go_to_line(line, cx));
        }
        self.close_overlay(&CloseOverlay, window, cx);
    }

    fn command_palette(&mut self, _: &CommandPalette, window: &mut Window, cx: &mut Context<Self>) {
        self.palette_input.update(cx, |input, cx| input.replace_all_text("", cx));
        self.palette_index = 0;
        self.open_overlay(Overlay::Palette, window, cx);
    }

    fn filtered_commands(&self, cx: &App) -> Vec<&'static Command> {
        let query = self.palette_input.read(cx).text().to_lowercase();
        COMMANDS
            .iter()
            .filter(|command| {
                if query.is_empty() {
                    return true;
                }
                let label = command.label.to_lowercase();
                let mut chars = query.chars().filter(|c| !c.is_whitespace());
                let mut needle = chars.next();
                for c in label.chars() {
                    if Some(c) == needle {
                        needle = chars.next();
                    }
                }
                needle.is_none()
            })
            .collect()
    }

    fn palette_move(&mut self, delta: isize, cx: &mut Context<Self>) {
        let count = self.filtered_commands(cx).len();
        if count == 0 {
            return;
        }
        self.palette_index = ((self.palette_index as isize + delta).rem_euclid(count as isize)) as usize;
        cx.notify();
    }

    fn palette_run(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let commands = self.filtered_commands(cx);
        let Some(command) = commands.get(self.palette_index).copied() else {
            return;
        };
        self.close_overlay(&CloseOverlay, window, cx);
        (command.run)(self, window, cx);
    }

    fn close_overlay(&mut self, _: &CloseOverlay, window: &mut Window, cx: &mut Context<Self>) {
        if self.overlay == Overlay::None {
            return;
        }
        self.overlay = Overlay::None;
        self.replace_visible = false;
        self.update_search(cx);
        self.with_editor(window, cx, |_, _| {});
        cx.notify();
    }

    fn toggle_focus(&mut self, _: &ToggleFocusMode, _: &mut Window, cx: &mut Context<Self>) {
        self.focus_mode = !self.focus_mode;
        let centered = self.focus_mode;
        for tab in &self.tabs {
            tab.editor.update(cx, |editor, cx| {
                editor.centered = centered;
                editor.invalidate_layout(cx);
            });
        }
        cx.notify();
    }

    fn toggle_wrap(&mut self, _: &ToggleWrap, _: &mut Window, cx: &mut Context<Self>) {
        self.wrap = !self.wrap;
        self.invalidate_editors(cx);
    }

    fn toggle_line_numbers(&mut self, _: &ToggleLineNumbers, _: &mut Window, cx: &mut Context<Self>) {
        self.line_numbers = !self.line_numbers;
        self.invalidate_editors(cx);
    }

    fn toggle_status(&mut self, _: &ToggleStatusBar, _: &mut Window, cx: &mut Context<Self>) {
        self.status_visible = !self.status_visible;
        cx.notify();
    }

    fn zoom_in(&mut self, _: &ZoomIn, _: &mut Window, cx: &mut Context<Self>) {
        self.font_size = (self.font_size + 1.).min(28.);
        self.invalidate_editors(cx);
    }

    fn zoom_out(&mut self, _: &ZoomOut, _: &mut Window, cx: &mut Context<Self>) {
        self.font_size = (self.font_size - 1.).max(10.);
        self.invalidate_editors(cx);
    }

    fn zoom_reset(&mut self, _: &ZoomReset, _: &mut Window, cx: &mut Context<Self>) {
        self.font_size = 14.5;
        self.invalidate_editors(cx);
    }

    fn export_html(&mut self, _: &ExportHtml, window: &mut Window, cx: &mut Context<Self>) {
        let Some(tab) = self.active_tab() else {
            return;
        };
        let id = tab.id;
        let directory = tab.snapshot.as_ref().and_then(|s| s.path.parent().map(Path::to_path_buf)).unwrap_or_else(|| PathBuf::from("."));
        let suggested = format!("{}.html", Path::new(&tab.title).file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_else(|| "document".into()));
        let receiver = cx.prompt_for_new_path(&directory, Some(&suggested));
        cx.spawn_in(window, async move |this, cx| {
            if let Ok(Ok(Some(path))) = receiver.await {
                this.update_in(cx, |this, _, cx| {
                    let Some(tab) = this.tabs.iter().find(|t| t.id == id) else {
                        return;
                    };
                    let text = tab.editor.read(cx).text();
                    let title = tab.title.clone();
                    let dark = cx.global::<Theme>().appearance == crate::theme::Appearance::Dark;
                    let html = crate::export::standalone_html(&title, &text, dark);
                    match std::fs::write(&path, html) {
                        Ok(()) => this.show_notice(format!("Exported {}", path.display()), NoticeKind::Info, cx),
                        Err(error) => this.show_notice(format!("Export failed: {error}"), NoticeKind::Error, cx),
                    }
                })
                .ok();
            }
        })
        .detach();
    }

    fn quit(&mut self, _: &Quit, window: &mut Window, cx: &mut Context<Self>) {
        if self.request_close(window, cx) {
            cx.quit();
        }
    }

    fn file_clicked(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        let Some(entry) = self.files.entries().get(index).cloned() else {
            return;
        };
        if entry.is_dir {
            self.files.toggle(index);
            cx.notify();
        } else {
            self.open_path(&entry.path, window, cx);
        }
    }

    fn outline_clicked(&mut self, line: usize, window: &mut Window, cx: &mut Context<Self>) {
        self.with_editor(window, cx, |editor, cx| editor.go_to_line(line + 1, cx));
        if self.layout == Layout::Read
            && let Some(tab) = self.active_tab()
        {
            let ix = tab.document.blocks.partition_point(|b| b.line() < line);
            self.preview_scroll.scroll_to_item(ix.min(tab.document.blocks.len().saturating_sub(1)));
        }
    }

    fn jump_to_line(&mut self, line: usize, window: &mut Window, cx: &mut Context<Self>) {
        if self.layout == Layout::Read {
            self.layout = Layout::Split;
        }
        self.with_editor(window, cx, |editor, cx| editor.go_to_line(line + 1, cx));
        cx.notify();
    }

    fn open_link(&mut self, link: &str, window: &mut Window, cx: &mut Context<Self>) {
        if link.starts_with("http://") || link.starts_with("https://") || link.starts_with("mailto:") {
            cx.open_url(link);
            return;
        }
        let base = self.active_tab().and_then(|t| t.snapshot.as_ref()).and_then(|s| s.path.parent().map(Path::to_path_buf));
        let target = link.split('#').next().unwrap_or("");
        if target.is_empty() {
            return;
        }
        let path = match base {
            Some(base) => base.join(target),
            None => PathBuf::from(target),
        };
        if path.is_file() && is_text_document(&path) {
            self.open_path(&path, window, cx);
        } else {
            self.show_notice(format!("Only local Markdown links open in Glyph: {link}"), NoticeKind::Info, cx);
        }
    }

    fn sync_preview(&mut self, cx: &App) {
        if self.layout != Layout::Split {
            return;
        }
        let Some(tab) = self.active_tab() else {
            return;
        };
        let row = tab.editor.read(cx).first_visible_row();
        if self.last_sync_row == Some(row) {
            return;
        }
        let blocks = tab.document.blocks.len();
        let ix = tab.document.blocks.partition_point(|b| b.line() < row);
        self.last_sync_row = Some(row);
        if blocks == 0 {
            return;
        }
        let ix = ix.min(blocks - 1);
        if row == 0 {
            self.preview_scroll.set_offset(gpui::point(px(0.), px(0.)));
            return;
        }
        if let Some(item) = self.preview_scroll.bounds_for_item(ix) {
            let container = self.preview_scroll.bounds();
            let offset = self.preview_scroll.offset();
            let delta = item.top() - container.top() - px(28.);
            self.preview_scroll.set_offset(gpui::point(offset.x, offset.y - delta));
        } else {
            self.preview_scroll.scroll_to_item(ix);
        }
    }
}

impl Focusable for Workspace {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Workspace {
    fn icon_button(&self, id: &'static str, icon: &'static str, tooltip: &'static str, active: bool, theme: &Theme) -> gpui::Stateful<gpui::Div> {
        let _ = tooltip;
        div()
            .id(id)
            .size(px(30.))
            .rounded(px(8.))
            .flex()
            .items_center()
            .justify_center()
            .cursor_pointer()
            .when(active, |this| this.bg(theme.active))
            .hover(|this| this.bg(theme.hover))
            .child(svg().path(format!("icons/{icon}.svg")).size(px(16.)).text_color(if active { theme.accent } else { theme.muted }))
    }

    fn render_titlebar(&self, window: &Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.global::<Theme>().clone();
        let tab = self.active_tab();
        let title = tab.map(|t| t.title.clone()).unwrap_or_default();
        let dirty = tab.is_some_and(|t| t.dirty(cx));
        let maximized = window.is_maximized();
        let dark = theme.appearance == crate::theme::Appearance::Dark;
        let segment = |label: &'static str, layout: Layout, icon: &'static str| {
            let selected = self.layout == layout;
            div()
                .id(label)
                .h(px(26.))
                .px(px(11.))
                .rounded(px(6.))
                .flex()
                .items_center()
                .gap(px(6.))
                .cursor_pointer()
                .text_size(px(12.))
                .font_weight(if selected { FontWeight::SEMIBOLD } else { FontWeight::MEDIUM })
                .text_color(if selected { theme.text } else { theme.muted })
                .when(selected, |this| this.bg(theme.background).shadow_sm())
                .hover(move |this| if selected { this } else { this.text_color(theme.text) })
                .on_click(cx.listener(move |this, _, window, cx| this.set_layout(layout, window, cx)))
                .child(svg().path(format!("icons/{icon}.svg")).size(px(13.)).text_color(if selected { theme.accent } else { theme.faint }))
                .child(label)
        };
        let control = |id: &'static str, icon: &'static str, area: WindowControlArea, danger: bool| {
            div()
                .id(id)
                .group(id)
                .w(px(44.))
                .h(px(TITLEBAR_HEIGHT))
                .flex()
                .items_center()
                .justify_center()
                .window_control_area(area)
                .hover(move |this| if danger { this.bg(gpui::rgb(0xe5484d)).text_color(gpui::white()) } else { this.bg(theme.hover) })
                .text_color(theme.muted)
                .when(cfg!(target_os = "linux"), |this| {
                    this.on_click(move |_, window, _| match area {
                        WindowControlArea::Close => window.remove_window(),
                        WindowControlArea::Max => window.zoom_window(),
                        WindowControlArea::Min => window.minimize_window(),
                        WindowControlArea::Drag => {}
                    })
                })
                .child(svg().path(format!("icons/{icon}.svg")).size(px(14.)).text_color(theme.muted).group_hover(id, |this| this.text_color(if danger { gpui::white() } else { theme.text })))
        };
        div()
            .h(px(TITLEBAR_HEIGHT))
            .w_full()
            .flex()
            .items_center()
            .bg(theme.shell)
            .border_b_1()
            .border_color(theme.border)
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(6.))
                    .pl(px(10.))
                    .child(self.icon_button("toggle-sidebar", "sidebar", "Sidebar", self.sidebar_open && !self.focus_mode, &theme).on_click(cx.listener(|this, _, window, cx| this.toggle_sidebar(&ToggleSidebar, window, cx))))
                    .child(
                        div()
                            .h(px(TITLEBAR_HEIGHT))
                            .pl(px(6.))
                            .flex()
                            .items_center()
                            .gap(px(6.))
                            .window_control_area(WindowControlArea::Drag)
                            .child(div().text_size(px(13.)).font_weight(FontWeight::SEMIBOLD).text_color(theme.text).child(title))
                            .when(dirty, |this| this.child(div().size(px(7.)).rounded_full().bg(theme.accent)))
                            .when(!dirty, |this| this.child(div().size(px(7.)))),
                    ),
            )
            .child(div().flex_1().h_full().window_control_area(WindowControlArea::Drag))
            .child(
                div()
                    .flex()
                    .items_center()
                    .p(px(3.))
                    .rounded(px(9.))
                    .bg(theme.raised)
                    .border_1()
                    .border_color(theme.border)
                    .child(segment("Write", Layout::Write, "pencil"))
                    .child(segment("Split", Layout::Split, "columns"))
                    .child(segment("Read", Layout::Read, "eye")),
            )
            .child(div().flex_1().h_full().window_control_area(WindowControlArea::Drag))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(2.))
                    .pr(px(8.))
                    .child(self.icon_button("palette", "command", "Command palette", false, &theme).on_click(cx.listener(|this, _, window, cx| this.command_palette(&CommandPalette, window, cx))))
                    .child(self.icon_button("theme", if dark { "sun" } else { "moon" }, "Theme", false, &theme).on_click(cx.listener(|this, _, window, cx| this.toggle_theme(&ToggleTheme, window, cx)))),
            )
            .child(div().w(px(1.)).h(px(18.)).bg(theme.border))
            .child(control("win-min", "minus", WindowControlArea::Min, false))
            .child(control("win-max", if maximized { "restore" } else { "square" }, WindowControlArea::Max, false))
            .child(control("win-close", "x", WindowControlArea::Close, true))
    }

    fn render_tabs(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.global::<Theme>().clone();
        div()
            .h(px(36.))
            .w_full()
            .flex()
            .items_center()
            .gap(px(4.))
            .px(px(8.))
            .bg(theme.shell)
            .border_b_1()
            .border_color(theme.border)
            .children(self.tabs.iter().enumerate().map(|(ix, tab)| {
                let selected = ix == self.active;
                let id = tab.id;
                let dirty = tab.dirty(cx);
                div()
                    .id(("tab", id))
                    .group("tab")
                    .h(px(28.))
                    .pl(px(12.))
                    .pr(px(6.))
                    .rounded(px(7.))
                    .flex()
                    .items_center()
                    .gap(px(6.))
                    .cursor_pointer()
                    .text_size(px(12.5))
                    .text_color(if selected { theme.text } else { theme.muted })
                    .when(selected, |this| this.bg(theme.raised).border_1().border_color(theme.border))
                    .hover(|this| this.bg(theme.hover))
                    .on_click(cx.listener(move |this, _, window, cx| {
                        if let Some(ix) = this.tabs.iter().position(|t| t.id == id) {
                            this.activate_tab(ix, window, cx);
                        }
                    }))
                    .child(div().max_w(px(180.)).truncate().child(tab.title.clone()))
                    .child(
                        div()
                            .id(("tab-close", id))
                            .size(px(18.))
                            .rounded(px(5.))
                            .flex()
                            .items_center()
                            .justify_center()
                            .hover(|this| this.bg(theme.hover))
                            .on_click(cx.listener(move |this, _, window, cx| this.close_tab_by_id(id, false, window, cx)))
                            .child(if dirty {
                                div().size(px(7.)).rounded_full().bg(theme.accent).into_any_element()
                            } else {
                                svg().path("icons/x.svg").size(px(12.)).text_color(theme.faint).into_any_element()
                            }),
                    )
            }))
            .child(
                div()
                    .id("tab-new")
                    .size(px(26.))
                    .rounded(px(7.))
                    .flex()
                    .items_center()
                    .justify_center()
                    .cursor_pointer()
                    .hover(|this| this.bg(theme.hover))
                    .on_click(cx.listener(|this, _, window, cx| this.new_document(&NewDocument, window, cx)))
                    .child(svg().path("icons/plus.svg").size(px(13.)).text_color(theme.faint)),
            )
    }

    fn render_sidebar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.global::<Theme>().clone();
        let root_name = self.files.root().and_then(|r| r.file_name()).map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| "No folder".to_owned());
        let this = cx.entity();
        let tab_button = |label: &'static str, tab: SidebarTab, icon: &'static str| {
            let selected = self.sidebar_tab == tab;
            div()
                .id(label)
                .flex_1()
                .h(px(26.))
                .rounded(px(6.))
                .flex()
                .items_center()
                .justify_center()
                .gap(px(5.))
                .cursor_pointer()
                .text_size(px(12.))
                .font_weight(FontWeight::MEDIUM)
                .text_color(if selected { theme.text } else { theme.muted })
                .when(selected, |this| this.bg(theme.background).shadow_sm())
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.sidebar_tab = tab;
                    cx.notify();
                }))
                .child(svg().path(format!("icons/{icon}.svg")).size(px(12.)).text_color(if selected { theme.accent } else { theme.faint }))
                .child(label)
        };
        let list: AnyElement = match self.sidebar_tab {
            SidebarTab::Files => {
                let count = self.files.entries().len();
                if count == 0 {
                    div()
                        .p(px(14.))
                        .text_size(px(12.5))
                        .text_color(theme.faint)
                        .child(if self.files.root().is_some() { "No Markdown files here." } else { "Open a folder to browse its Markdown files." })
                        .into_any_element()
                } else {
                    uniform_list("files", count, move |range, _, cx| {
                        let workspace = this.read(cx);
                        range
                            .filter_map(|ix| workspace.files.entries().get(ix).map(|e| (ix, e.clone())))
                            .map(|(ix, entry)| {
                                let open = workspace.active_tab().and_then(|t| t.snapshot.as_ref()).is_some_and(|s| s.path == entry.path);
                                let handle = this.clone();
                                let icon = if entry.is_dir {
                                    if entry.expanded { "chevron-down" } else { "chevron-right" }
                                } else {
                                    "file"
                                };
                                div()
                                    .id(("file", ix))
                                    .h(px(28.))
                                    .mx(px(6.))
                                    .pl(px(8. + entry.depth as f32 * 14.))
                                    .pr(px(8.))
                                    .rounded(px(6.))
                                    .flex()
                                    .items_center()
                                    .gap(px(6.))
                                    .cursor_pointer()
                                    .text_size(px(12.5))
                                    .text_color(if open { theme.text } else { theme.muted })
                                    .when(open, |this| this.bg(theme.active))
                                    .hover(|this| this.bg(theme.hover))
                                    .on_click(move |_, window, cx| handle.update(cx, |ws, cx| ws.file_clicked(ix, window, cx)))
                                    .child(svg().path(format!("icons/{icon}.svg")).size(px(13.)).text_color(if entry.is_dir { theme.faint } else { theme.accent.alpha(0.8) }))
                                    .child(div().truncate().child(entry.name))
                            })
                            .collect()
                    })
                    .track_scroll(&self.file_scroll)
                    .size_full()
                    .into_any_element()
                }
            }
            SidebarTab::Outline => {
                let document: Rc<Document> = self.active_tab().map(|t| t.document.clone()).unwrap_or_default();
                if document.headings.is_empty() {
                    div().p(px(14.)).text_size(px(12.5)).text_color(theme.faint).child("Headings will appear here.").into_any_element()
                } else {
                    let count = document.headings.len();
                    uniform_list("outline", count, move |range, _, _| {
                        range
                            .filter_map(|ix| document.headings.get(ix).map(|h| (ix, h.clone())))
                            .map(|(ix, heading)| {
                                let handle = this.clone();
                                let line = heading.line;
                                div()
                                    .id(("heading", ix))
                                    .h(px(28.))
                                    .mx(px(6.))
                                    .pl(px(10. + (heading.level.saturating_sub(1)) as f32 * 12.))
                                    .pr(px(8.))
                                    .rounded(px(6.))
                                    .flex()
                                    .items_center()
                                    .gap(px(6.))
                                    .cursor_pointer()
                                    .text_size(if heading.level <= 2 { px(12.5) } else { px(12.) })
                                    .font_weight(if heading.level == 1 { FontWeight::SEMIBOLD } else { FontWeight::NORMAL })
                                    .text_color(if heading.level <= 2 { theme.text } else { theme.muted })
                                    .hover(|this| this.bg(theme.hover))
                                    .on_click(move |_, window, cx| handle.update(cx, |ws, cx| ws.outline_clicked(line, window, cx)))
                                    .child(div().truncate().child(heading.text))
                            })
                            .collect()
                    })
                    .track_scroll(&self.outline_scroll)
                    .size_full()
                    .into_any_element()
                }
            }
        };
        div()
            .w(px(SIDEBAR_WIDTH))
            .h_full()
            .flex()
            .flex_col()
            .bg(theme.shell)
            .border_r_1()
            .border_color(theme.border)
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .px(px(12.))
                    .pt(px(12.))
                    .pb(px(6.))
                    .child(div().text_size(px(12.)).font_weight(FontWeight::SEMIBOLD).text_color(theme.muted).truncate().child(root_name))
                    .child(self.icon_button("open-folder", "folder", "Open folder", false, &theme).size(px(26.)).on_click(cx.listener(|this, _, window, cx| this.open_folder(&OpenFolder, window, cx)))),
            )
            .child(
                div()
                    .mx(px(10.))
                    .mb(px(8.))
                    .p(px(3.))
                    .rounded(px(8.))
                    .bg(theme.raised)
                    .flex()
                    .child(tab_button("Files", SidebarTab::Files, "folder"))
                    .child(tab_button("Outline", SidebarTab::Outline, "list")),
            )
            .child(div().flex_1().min_h_0().child(list))
    }

    fn render_find_bar(&self, window: &Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.global::<Theme>().clone();
        let (count, current) = self
            .active_editor()
            .map(|editor| {
                let editor = editor.read(cx);
                (editor.search.matches.len(), editor.search.current)
            })
            .unwrap_or((0, None));
        let status = if self.find_input.read(cx).text().is_empty() {
            String::new()
        } else if count == 0 {
            "No matches".to_owned()
        } else {
            format!("{} of {count}", current.map(|c| c + 1).unwrap_or(0))
        };
        let input_box = |editor: &Entity<Editor>, icon: &'static str| {
            div()
                .h(px(30.))
                .w(px(300.))
                .px(px(10.))
                .rounded(px(8.))
                .bg(theme.background)
                .border_1()
                .border_color(if editor.read(cx).is_focused(window) { theme.accent } else { theme.border })
                .flex()
                .items_center()
                .gap(px(8.))
                .child(svg().path(format!("icons/{icon}.svg")).size(px(14.)).text_color(theme.faint))
                .child(div().flex_1().min_w_0().child(editor.clone()))
        };
        let small_button = |id: &'static str, label: &'static str| {
            div()
                .id(id)
                .h(px(28.))
                .px(px(10.))
                .rounded(px(7.))
                .flex()
                .items_center()
                .cursor_pointer()
                .text_size(px(12.))
                .text_color(theme.muted)
                .border_1()
                .border_color(theme.border)
                .hover(|this| this.bg(theme.hover).text_color(theme.text))
                .child(label)
        };
        div()
            .w_full()
            .flex()
            .flex_col()
            .gap(px(6.))
            .px(px(12.))
            .py(px(8.))
            .bg(theme.shell)
            .border_b_1()
            .border_color(theme.border)
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(8.))
                    .child(input_box(&self.find_input, "search"))
                    .child(div().w(px(70.)).text_size(px(12.)).text_color(theme.faint).child(status))
                    .child(self.icon_button("find-prev", "arrow-up", "Previous", false, &theme).on_click(cx.listener(|this, _, window, cx| this.find_previous(&FindPrevious, window, cx))))
                    .child(self.icon_button("find-next", "arrow-down", "Next", false, &theme).on_click(cx.listener(|this, _, window, cx| this.find_next(&FindNext, window, cx))))
                    .child(self.icon_button("find-replace-toggle", "replace", "Replace", self.replace_visible, &theme).on_click(cx.listener(|this, _, window, cx| {
                        this.replace_visible = !this.replace_visible;
                        if this.replace_visible {
                            this.replace_input.update(cx, |input, cx| input.focus(window, cx));
                        }
                        cx.notify();
                    })))
                    .child(div().flex_1())
                    .child(self.icon_button("find-close", "x", "Close", false, &theme).on_click(cx.listener(|this, _, window, cx| this.close_overlay(&CloseOverlay, window, cx)))),
            )
            .when(self.replace_visible, |this| {
                this.child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(8.))
                        .child(input_box(&self.replace_input, "replace"))
                        .child(small_button("replace-one", "Replace").on_click(cx.listener(|this, _, window, cx| this.replace_current(window, cx))))
                        .child(small_button("replace-all", "Replace all").on_click(cx.listener(|this, _, _, cx| this.replace_everything(cx)))),
                )
            })
    }

    fn render_preview(&self, cx: &mut Context<Self>) -> AnyElement {
        let theme = cx.global::<Theme>().clone();
        let Some(tab) = self.active_tab() else {
            return div().into_any_element();
        };
        let this = cx.entity();
        let jump = this.clone();
        let context = PreviewContext::new(
            theme.clone(),
            tab.snapshot.as_ref().and_then(|s| s.path.parent().map(Path::to_path_buf)),
            tab.id,
            Rc::new(move |link, window, cx| {
                let link = link.to_owned();
                this.update(cx, |ws, cx| ws.open_link(&link, window, cx))
            }),
            Rc::new(move |line, window, cx| jump.update(cx, |ws, cx| ws.jump_to_line(line, window, cx))),
        );
        let blocks = context.render(&tab.document);
        let empty = blocks.is_empty();
        div()
            .id(("preview", tab.id))
            .size_full()
            .overflow_y_scroll()
            .track_scroll(&self.preview_scroll)
            .flex()
            .flex_col()
            .items_center()
            .px(px(40.))
            .pt(px(28.))
            .pb(px(160.))
            .font_family(theme.sans.clone())
            .text_size(px(15.5))
            .line_height(px(26.))
            .text_color(theme.text)
            .children(blocks.into_iter().map(|block| div().w_full().max_w(px(760.)).child(block)))
            .when(empty, |this| this.child(div().w_full().max_w(px(760.)).text_color(theme.faint).child("Nothing to preview yet.")))
            .into_any_element()
    }

    fn active_word_count(&mut self, cx: &App) -> usize {
        let Some(tab) = self.tabs.get_mut(self.active) else {
            return 0;
        };
        let editor = tab.editor.read(cx);
        let version = editor.buffer.version();
        if tab.word_count_version != Some(version) {
            tab.word_count = editor.buffer.lines().iter().map(|l| l.split_whitespace().count()).sum();
            tab.word_count_version = Some(version);
        }
        tab.word_count
    }

    fn render_status(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.global::<Theme>().clone();
        let words = self.active_word_count(cx);
        let (position, lines) = self
            .active_editor()
            .map(|editor| {
                let editor = editor.read(cx);
                let cursor = editor.cursor();
                let line = editor.buffer.line(cursor.row);
                let column = line[..cursor.col.min(line.len())].chars().count() + 1;
                (format!("Ln {}, Col {}", cursor.row + 1, column), editor.buffer.line_count())
            })
            .unwrap_or_default();
        let path = self
            .active_tab()
            .and_then(|t| t.snapshot.as_ref())
            .map(|s| s.path.to_string_lossy().into_owned())
            .unwrap_or_else(|| "Unsaved document".to_owned());
        let newline = self.active_tab().and_then(|t| t.snapshot.as_ref()).map(|s| match s.newline {
            crate::document::Newline::CrLf => "CRLF",
            crate::document::Newline::Cr => "CR",
            crate::document::Newline::Lf => "LF",
        });
        let item = |text: String| div().child(text);
        div()
            .h(px(26.))
            .w_full()
            .flex()
            .items_center()
            .gap(px(16.))
            .px(px(14.))
            .bg(theme.shell)
            .border_t_1()
            .border_color(theme.border)
            .text_size(px(11.5))
            .text_color(theme.faint)
            .child(item(position))
            .child(item(format!("{words} words")))
            .child(item(format!("{lines} lines")))
            .child(div().flex_1().truncate().text_center().child(path))
            .child(item(format!("UTF-8{}", newline.map(|n| format!(" · {n}")).unwrap_or_default())))
            .child(item(if self.wrap { "Wrap".to_owned() } else { "No wrap".to_owned() }))
    }

    fn render_palette(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.global::<Theme>().clone();
        let commands = self.filtered_commands(cx);
        let selected = self.palette_index.min(commands.len().saturating_sub(1));
        div()
            .id("palette-backdrop")
            .absolute()
            .inset_0()
            .bg(gpui::black().alpha(if theme.appearance == crate::theme::Appearance::Dark { 0.45 } else { 0.2 }))
            .on_mouse_down(MouseButton::Left, cx.listener(|this, _, window, cx| this.close_overlay(&CloseOverlay, window, cx)))
            .flex()
            .justify_center()
            .pt(px(110.))
            .child(
                div()
                    .id("palette-card")
                    .w(px(560.))
                    .max_h(px(420.))
                    .rounded(px(12.))
                    .bg(theme.card)
                    .border_1()
                    .border_color(theme.border_strong)
                    .shadow_lg()
                    .flex()
                    .flex_col()
                    .overflow_hidden()
                    .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(10.))
                            .px(px(14.))
                            .h(px(46.))
                            .border_b_1()
                            .border_color(theme.border)
                            .child(svg().path("icons/search.svg").size(px(15.)).text_color(theme.faint))
                            .child(div().flex_1().min_w_0().text_size(px(14.)).child(self.palette_input.clone())),
                    )
                    .child(
                        div().id("palette-list").flex_1().overflow_y_scroll().py(px(6.)).children(commands.iter().enumerate().map(|(ix, command)| {
                            let active = ix == selected;
                            div()
                                .id(("command", ix))
                                .mx(px(6.))
                                .h(px(34.))
                                .px(px(10.))
                                .rounded(px(7.))
                                .flex()
                                .items_center()
                                .justify_between()
                                .cursor_pointer()
                                .text_size(px(13.))
                                .text_color(if active { theme.text } else { theme.muted })
                                .when(active, |this| this.bg(theme.active))
                                .hover(|this| this.bg(theme.hover))
                                .on_click(cx.listener(move |this, _, window, cx| {
                                    this.palette_index = ix;
                                    this.palette_run(window, cx);
                                }))
                                .child(command.label)
                                .child(div().text_size(px(11.)).font_family(theme.mono.clone()).text_color(theme.faint).child(command.keys))
                        })),
                    )
                    .when(commands.is_empty(), |this| this.child(div().p(px(14.)).text_size(px(13.)).text_color(theme.faint).child("No matching commands"))),
            )
    }

    fn render_goto(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.global::<Theme>().clone();
        let lines = self.active_editor().map(|e| e.read(cx).buffer.line_count()).unwrap_or(1);
        div()
            .id("goto-backdrop")
            .absolute()
            .inset_0()
            .on_mouse_down(MouseButton::Left, cx.listener(|this, _, window, cx| this.close_overlay(&CloseOverlay, window, cx)))
            .flex()
            .justify_center()
            .pt(px(110.))
            .child(
                div()
                    .id("goto-card")
                    .w(px(320.))
                    .rounded(px(12.))
                    .bg(theme.card)
                    .border_1()
                    .border_color(theme.border_strong)
                    .shadow_lg()
                    .p(px(12.))
                    .flex()
                    .flex_col()
                    .gap(px(8.))
                    .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                    .child(div().text_size(px(12.)).text_color(theme.muted).child(format!("Go to line (1 – {lines})")))
                    .child(
                        div()
                            .h(px(32.))
                            .px(px(10.))
                            .rounded(px(8.))
                            .bg(theme.background)
                            .border_1()
                            .border_color(theme.accent)
                            .flex()
                            .items_center()
                            .child(div().flex_1().min_w_0().child(self.goto_input.clone())),
                    ),
            )
    }

    fn render_notice(&self, cx: &mut Context<Self>) -> Option<impl IntoElement> {
        let theme = cx.global::<Theme>().clone();
        let (text, kind) = self.notice.clone()?;
        let error = kind == NoticeKind::Error;
        Some(
            div().absolute().bottom(px(40.)).left_0().right_0().flex().justify_center().child(
                div()
                    .max_w(px(520.))
                    .px(px(14.))
                    .py(px(9.))
                    .rounded(px(10.))
                    .bg(theme.card)
                    .border_1()
                    .border_color(if error { theme.danger.alpha(0.5) } else { theme.border_strong })
                    .shadow_md()
                    .flex()
                    .items_center()
                    .gap(px(8.))
                    .text_size(px(12.5))
                    .text_color(theme.text)
                    .child(svg().path(if error { "icons/alert.svg" } else { "icons/check.svg" }).size(px(14.)).text_color(if error { theme.danger } else { theme.success }))
                    .child(div().truncate().child(text)),
            ),
        )
    }
}

impl Render for Workspace {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.global::<Theme>().clone();
        self.sync_preview(cx);
        let show_chrome = !self.focus_mode;
        let show_editor = self.layout != Layout::Read;
        let show_preview = self.layout != Layout::Write;
        let editor = self.active_editor();
        let mut panes = div().flex_1().min_h_0().w_full().flex();
        if show_editor && let Some(editor) = editor.clone() {
            panes = panes.child(div().flex_1().min_w_0().h_full().bg(theme.background).child(editor));
        }
        if show_editor && show_preview {
            panes = panes.child(div().w(px(1.)).h_full().bg(theme.border));
        }
        if show_preview {
            panes = panes.child(div().flex_1().min_w_0().h_full().bg(if show_editor { theme.shell } else { theme.background }).child(self.render_preview(cx)));
        }
        let mut main = div().flex_1().min_w_0().h_full().flex().flex_col();
        if self.overlay == Overlay::Find {
            main = main.child(self.render_find_bar(window, cx));
        }
        main = main.child(panes);
        let mut body = div().flex_1().min_h_0().w_full().flex();
        if self.sidebar_open && show_chrome {
            body = body.child(self.render_sidebar(cx));
        }
        body = body.child(main);
        let mut root = div()
            .id("workspace")
            .key_context("Workspace")
            .track_focus(&self.focus_handle)
            .relative()
            .size_full()
            .flex()
            .flex_col()
            .bg(theme.background)
            .text_color(theme.text)
            .font_family(theme.sans.clone())
            .text_size(px(13.))
            .on_action(cx.listener(Self::new_document))
            .on_action(cx.listener(Self::open_file))
            .on_action(cx.listener(Self::open_folder))
            .on_action(cx.listener(Self::save))
            .on_action(cx.listener(Self::save_as))
            .on_action(cx.listener(Self::close_tab))
            .on_action(cx.listener(Self::next_tab))
            .on_action(cx.listener(Self::previous_tab))
            .on_action(cx.listener(Self::toggle_sidebar))
            .on_action(cx.listener(Self::toggle_theme))
            .on_action(cx.listener(Self::view_write))
            .on_action(cx.listener(Self::view_split))
            .on_action(cx.listener(Self::view_read))
            .on_action(cx.listener(Self::find))
            .on_action(cx.listener(Self::replace))
            .on_action(cx.listener(Self::go_to_line))
            .on_action(cx.listener(Self::command_palette))
            .on_action(cx.listener(Self::toggle_focus))
            .on_action(cx.listener(Self::toggle_wrap))
            .on_action(cx.listener(Self::toggle_line_numbers))
            .on_action(cx.listener(Self::toggle_status))
            .on_action(cx.listener(Self::zoom_in))
            .on_action(cx.listener(Self::zoom_out))
            .on_action(cx.listener(Self::zoom_reset))
            .on_action(cx.listener(Self::export_html))
            .on_action(cx.listener(Self::quit))
            .on_action(cx.listener(Self::find_next))
            .on_action(cx.listener(Self::find_previous))
            .on_action(cx.listener(Self::close_overlay))
            .child(self.render_titlebar(window, cx));
        if self.tabs.len() > 1 && show_chrome {
            root = root.child(self.render_tabs(cx));
        }
        root = root.child(body);
        if self.status_visible && show_chrome {
            root = root.child(self.render_status(cx));
        }
        match self.overlay {
            Overlay::Palette => root = root.child(self.render_palette(cx)),
            Overlay::GoToLine => root = root.child(self.render_goto(cx)),
            _ => {}
        }
        if let Some(notice) = self.render_notice(cx) {
            root = root.child(notice);
        }
        root
    }
}

