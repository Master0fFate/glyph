#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

mod assets;
mod buffer;
mod document;
mod editor;
mod export;
mod files;
mod highlight;
mod markdown;
mod preview;
mod settings;
mod theme;
mod workspace;

use gpui::{App, Bounds, KeyBinding, SharedString, TitlebarOptions, WindowBounds, WindowDecorations, WindowOptions, point, prelude::*, px, size};
use settings::Settings;
use std::path::PathBuf;
use theme::Theme;

fn pick_font(available: &[String], candidates: &[&str], fallback: &str) -> SharedString {
    candidates
        .iter()
        .find(|name| available.iter().any(|a| a.eq_ignore_ascii_case(name)))
        .map(|name| SharedString::from(name.to_string()))
        .unwrap_or_else(|| SharedString::from(fallback.to_string()))
}

fn bind_keys(cx: &mut App) {
    use editor::*;
    use workspace::*;
    let editor = Some("Editor");
    cx.bind_keys([
        KeyBinding::new("left", MoveLeft, editor),
        KeyBinding::new("right", MoveRight, editor),
        KeyBinding::new("up", MoveUp, editor),
        KeyBinding::new("down", MoveDown, editor),
        KeyBinding::new("shift-left", SelectLeft, editor),
        KeyBinding::new("shift-right", SelectRight, editor),
        KeyBinding::new("shift-up", SelectUp, editor),
        KeyBinding::new("shift-down", SelectDown, editor),
        KeyBinding::new("ctrl-left", MoveWordLeft, editor),
        KeyBinding::new("ctrl-right", MoveWordRight, editor),
        KeyBinding::new("ctrl-shift-left", SelectWordLeft, editor),
        KeyBinding::new("ctrl-shift-right", SelectWordRight, editor),
        KeyBinding::new("home", LineStart, editor),
        KeyBinding::new("end", LineEnd, editor),
        KeyBinding::new("shift-home", SelectLineStart, editor),
        KeyBinding::new("shift-end", SelectLineEnd, editor),
        KeyBinding::new("ctrl-home", DocumentStart, editor),
        KeyBinding::new("ctrl-end", DocumentEnd, editor),
        KeyBinding::new("ctrl-shift-home", SelectDocumentStart, editor),
        KeyBinding::new("ctrl-shift-end", SelectDocumentEnd, editor),
        KeyBinding::new("pageup", PageUp, editor),
        KeyBinding::new("pagedown", PageDown, editor),
        KeyBinding::new("shift-pageup", SelectPageUp, editor),
        KeyBinding::new("shift-pagedown", SelectPageDown, editor),
        KeyBinding::new("backspace", Backspace, editor),
        KeyBinding::new("shift-backspace", Backspace, editor),
        KeyBinding::new("delete", Delete, editor),
        KeyBinding::new("ctrl-backspace", DeleteWordLeft, editor),
        KeyBinding::new("enter", Newline, editor),
        KeyBinding::new("shift-enter", ShiftNewline, editor),
        KeyBinding::new("tab", Indent, editor),
        KeyBinding::new("shift-tab", Outdent, editor),
        KeyBinding::new("ctrl-a", SelectAll, editor),
        KeyBinding::new("ctrl-c", Copy, editor),
        KeyBinding::new("ctrl-x", Cut, editor),
        KeyBinding::new("ctrl-v", Paste, editor),
        KeyBinding::new("ctrl-z", Undo, editor),
        KeyBinding::new("ctrl-y", Redo, editor),
        KeyBinding::new("ctrl-shift-z", Redo, editor),
        KeyBinding::new("ctrl-d", DuplicateLine, editor),
        KeyBinding::new("ctrl-shift-k", DeleteLine, editor),
        KeyBinding::new("alt-up", MoveLineUp, editor),
        KeyBinding::new("alt-down", MoveLineDown, editor),
        KeyBinding::new("ctrl-b", ToggleBold, editor),
        KeyBinding::new("ctrl-i", ToggleItalic, editor),
        KeyBinding::new("ctrl-e", ToggleInlineCode, editor),
        KeyBinding::new("ctrl-k", InsertLink, editor),
        KeyBinding::new("ctrl-shift-c", InsertCodeBlock, editor),
        KeyBinding::new("ctrl-shift-m", InsertMermaidBlock, editor),
        KeyBinding::new("escape", Cancel, editor),
        KeyBinding::new("ctrl-n", NewDocument, None),
        KeyBinding::new("ctrl-o", OpenFile, None),
        KeyBinding::new("ctrl-shift-o", OpenFolder, None),
        KeyBinding::new("ctrl-s", Save, None),
        KeyBinding::new("ctrl-shift-s", SaveAs, None),
        KeyBinding::new("ctrl-w", CloseTab, None),
        KeyBinding::new("ctrl-tab", NextTab, None),
        KeyBinding::new("ctrl-shift-tab", PreviousTab, None),
        KeyBinding::new("ctrl-\\", ToggleSidebar, None),
        KeyBinding::new("ctrl-shift-l", ToggleTheme, None),
        KeyBinding::new("ctrl-1", ViewWrite, None),
        KeyBinding::new("ctrl-2", ViewSplit, None),
        KeyBinding::new("ctrl-3", ViewRead, None),
        KeyBinding::new("ctrl-f", Find, None),
        KeyBinding::new("ctrl-h", Replace, None),
        KeyBinding::new("ctrl-g", GoToLine, None),
        KeyBinding::new("ctrl-shift-p", CommandPalette, None),
        KeyBinding::new("f11", ToggleFocusMode, None),
        KeyBinding::new("alt-z", ToggleWrap, None),
        KeyBinding::new("ctrl-=", ZoomIn, None),
        KeyBinding::new("ctrl-+", ZoomIn, None),
        KeyBinding::new("ctrl--", ZoomOut, None),
        KeyBinding::new("ctrl-0", ZoomReset, None),
        KeyBinding::new("ctrl-shift-e", ExportHtml, None),
        KeyBinding::new("ctrl-q", Quit, None),
        KeyBinding::new("f3", FindNext, None),
        KeyBinding::new("shift-f3", FindPrevious, None),
        KeyBinding::new("escape", CloseOverlay, None),
    ]);
}

fn main() {
    let paths: Vec<PathBuf> = std::env::args_os().skip(1).map(PathBuf::from).filter(|p| p.exists()).collect();
    let settings = Settings::load();
    gpui_platform::application().with_assets(assets::Assets).run(move |cx: &mut App| {
        let fonts = cx.text_system().all_font_names();
        let sans = pick_font(&fonts, &["Inter", "Segoe UI Variable Text", "Segoe UI", "SF Pro Text", "Helvetica Neue", "Cantarell", "Ubuntu", "DejaVu Sans"], ".SystemUIFont");
        let mono = pick_font(
            &fonts,
            &["JetBrains Mono", "JetBrains Mono NL", "Cascadia Code", "Cascadia Mono", "SF Mono", "Fira Code", "Consolas", "Menlo", "DejaVu Sans Mono", "Ubuntu Mono"],
            "monospace",
        );
        cx.set_global(Theme::new(settings.theme, sans, mono));
        bind_keys(cx);
        let bounds = match settings.window {
            Some([x, y, w, h]) if w > 300. && h > 200. => Bounds { origin: point(px(x), px(y)), size: size(px(w), px(h)) },
            _ => Bounds::centered(None, size(px(1180.), px(780.)), cx),
        };
        let options = WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(bounds)),
            titlebar: Some(TitlebarOptions { title: Some("Glyph".into()), appears_transparent: true, traffic_light_position: Some(point(px(12.), px(14.))) }),
            window_min_size: Some(size(px(720.), px(460.))),
            window_decorations: Some(WindowDecorations::Client),
            app_id: Some("glyph".into()),
            ..Default::default()
        };
        let settings = settings.clone();
        cx.open_window(options, move |window, cx| cx.new(|cx| workspace::Workspace::new(&settings, paths, window, cx)))
            .expect("open window");
        cx.activate(true);
    });
}
