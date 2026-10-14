use crate::buffer::{Buffer, Position, Selection};
use crate::highlight::{self, FenceState, Token};
use crate::theme::Theme;
use gpui::{
    App, Bounds, ClipboardItem, Context, CursorStyle, Element, ElementId, ElementInputHandler, Entity, EntityInputHandler,
    EventEmitter, FocusHandle, Focusable, FontStyle, FontWeight, GlobalElementId, InspectorElementId, IntoElement,
    LayoutId, MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent, Pixels, Point, ScrollHandle, ShapedLine,
    SharedString, Style, TextAlign, TextRun, UTF16Selection, Window, WrappedLine, actions, div, fill,
    font, point, prelude::*, px, relative, size,
};
use std::ops::Range;
use unicode_segmentation::UnicodeSegmentation;

actions!(
    editor,
    [
        MoveLeft,
        MoveRight,
        MoveUp,
        MoveDown,
        SelectLeft,
        SelectRight,
        SelectUp,
        SelectDown,
        MoveWordLeft,
        MoveWordRight,
        SelectWordLeft,
        SelectWordRight,
        LineStart,
        LineEnd,
        SelectLineStart,
        SelectLineEnd,
        DocumentStart,
        DocumentEnd,
        SelectDocumentStart,
        SelectDocumentEnd,
        PageUp,
        PageDown,
        SelectPageUp,
        SelectPageDown,
        Backspace,
        Delete,
        DeleteWordLeft,
        Newline,
        ShiftNewline,
        Indent,
        Outdent,
        SelectAll,
        Copy,
        Cut,
        Paste,
        Undo,
        Redo,
        DuplicateLine,
        DeleteLine,
        MoveLineUp,
        MoveLineDown,
        ToggleBold,
        ToggleItalic,
        ToggleInlineCode,
        InsertLink,
        InsertCodeBlock,
        InsertMermaidBlock,
        Cancel,
    ]
);

pub const HIGHLIGHT_LIMIT: usize = 200_000;
const WRAP_LIMIT: usize = 1_500_000;
const PAD_TOP: f32 = 28.0;
const PAD_BOTTOM: f32 = 160.0;
const GUTTER_GAP: f32 = 18.0;
const TEXT_INSET: f32 = 24.0;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum EditorMode {
    Document,
    SingleLine,
}

pub enum EditorEvent {
    Edited,
    Submit,
    SubmitReverse,
    Navigate(isize),
    Cancel,
    SelectionChanged,
}

struct LineLayout {
    wrapped: WrappedLine,
    rows: usize,
}

#[derive(Default)]
pub struct SearchState {
    pub query: String,
    pub matches: Vec<Range<Position>>,
    pub current: Option<usize>,
}

pub struct Editor {
    pub buffer: Buffer,
    pub selection: Selection,
    pub mode: EditorMode,
    pub placeholder: SharedString,
    marked: Option<Range<Position>>,
    goal_x: Option<Pixels>,
    focus_handle: FocusHandle,
    pub scroll_handle: ScrollHandle,
    layouts: Vec<Option<LineLayout>>,
    fences: Vec<FenceState>,
    row_starts: Vec<usize>,
    utf16_lens: Vec<usize>,
    layout_width: Option<Pixels>,
    layout_version: u64,
    pub font_size: Pixels,
    pub wrap: bool,
    pub line_numbers: bool,
    pub centered: bool,
    text_origin: Point<Pixels>,
    last_bounds: Option<Bounds<Pixels>>,
    viewport: Option<Bounds<Pixels>>,
    autoscroll: bool,
    is_selecting: bool,
    pub search: SearchState,
    pub highlighting: bool,
}

impl Editor {
    pub fn new(text: &str, mode: EditorMode, cx: &mut Context<Self>) -> Self {
        let buffer = Buffer::from_text(text);
        let mut editor = Self {
            buffer,
            selection: Selection::default(),
            mode,
            placeholder: SharedString::default(),
            marked: None,
            goal_x: None,
            focus_handle: cx.focus_handle(),
            scroll_handle: ScrollHandle::new(),
            layouts: Vec::new(),
            fences: Vec::new(),
            row_starts: Vec::new(),
            utf16_lens: Vec::new(),
            layout_width: None,
            layout_version: u64::MAX,
            font_size: if mode == EditorMode::SingleLine { px(13.) } else { px(14.5) },
            wrap: true,
            line_numbers: mode == EditorMode::Document,
            centered: false,
            text_origin: Point::default(),
            last_bounds: None,
            viewport: None,
            autoscroll: false,
            is_selecting: false,
            search: SearchState::default(),
            highlighting: mode == EditorMode::Document,
        };
        editor.rebuild_metadata();
        editor
    }

    pub fn single_line(placeholder: impl Into<SharedString>, cx: &mut Context<Self>) -> Self {
        let mut editor = Self::new("", EditorMode::SingleLine, cx);
        editor.placeholder = placeholder.into();
        editor
    }

    pub fn text(&self) -> String {
        self.buffer.text()
    }

    pub fn set_text(&mut self, text: &str, cx: &mut Context<Self>) {
        let before = self.selection;
        let end = self.buffer.end();
        self.buffer.replace(Position::default()..end, text, before, Selection::default(), false);
        self.selection = Selection::default();
        self.rebuild_metadata();
        self.after_edit(cx);
    }

    pub fn line_height(&self) -> Pixels {
        px((self.font_size.as_f32() * 1.65).round())
    }

    pub fn cursor(&self) -> Position {
        self.selection.head
    }

    pub fn is_focused(&self, window: &Window) -> bool {
        self.focus_handle.is_focused(window)
    }

    pub fn focus(&self, window: &mut Window, cx: &mut App) {
        window.focus(&self.focus_handle, cx);
    }

    pub fn first_visible_row(&self) -> usize {
        let line_height = self.line_height();
        let offset = -self.scroll_handle.offset().y;
        let visual = ((offset.as_f32() - PAD_TOP) / line_height.as_f32()).max(0.0) as usize;
        self.line_for_visual_row(visual)
    }

    fn rebuild_metadata(&mut self) {
        let count = self.buffer.line_count();
        let total: usize = self.buffer.lines().iter().map(|l| l.len() + 1).sum();
        self.highlighting = self.mode == EditorMode::Document && total <= HIGHLIGHT_LIMIT;
        if total > WRAP_LIMIT {
            self.wrap = false;
        }
        self.layouts = (0..count).map(|_| None).collect();
        self.utf16_lens = self.buffer.lines().iter().map(|l| l.encode_utf16().count()).collect();
        self.rebuild_fences(0);
        self.row_starts = (0..=count).collect();
        self.layout_version = u64::MAX;
    }

    fn rebuild_fences(&mut self, from: usize) {
        let count = self.buffer.line_count();
        self.fences.resize(count + 1, FenceState::default());
        let mut state = self.fences[from];
        for row in from..count {
            self.fences[row] = state;
            state = highlight::next_fence_state(state, self.buffer.line(row)).0;
        }
        self.fences[count] = state;
    }

    fn after_edit(&mut self, cx: &mut Context<Self>) {
        self.marked = None;
        self.goal_x = None;
        self.autoscroll = true;
        self.refresh_search();
        cx.emit(EditorEvent::Edited);
        cx.notify();
    }

    fn edit(&mut self, range: Range<Position>, text: &str, after: Selection, coalesce: bool, cx: &mut Context<Self>) -> Position {
        let before = self.selection;
        let end = self.buffer.replace(range.clone(), text, before, after, coalesce);
        let new_rows = Buffer::end_of(range.start, text).row - range.start.row + 1;
        self.layouts.splice(range.start.row..=range.end.row, (0..new_rows).map(|_| None));
        self.utf16_lens.splice(
            range.start.row..=range.end.row,
            (range.start.row..range.start.row + new_rows).map(|row| self.buffer.line(row).encode_utf16().count()),
        );
        self.rebuild_fences(range.start.row);
        self.selection = after;
        self.after_edit(cx);
        end
    }

    pub fn insert(&mut self, text: &str, cx: &mut Context<Self>) {
        let range = self.selection.range();
        let end = Buffer::end_of(range.start, text);
        self.edit(range, text, Selection::caret(end), text.chars().count() == 1 && !text.contains('\n'), cx);
    }

    pub fn replace_all_text(&mut self, text: &str, cx: &mut Context<Self>) {
        self.set_text(text, cx);
        self.buffer.break_undo_group();
    }

    fn set_selection(&mut self, selection: Selection, cx: &mut Context<Self>) {
        let selection = Selection { anchor: self.buffer.clamp(selection.anchor), head: self.buffer.clamp(selection.head) };
        if selection != self.selection {
            self.selection = selection;
            cx.emit(EditorEvent::SelectionChanged);
        }
        self.autoscroll = true;
        cx.notify();
    }

    fn move_head(&mut self, head: Position, select: bool, cx: &mut Context<Self>) {
        let anchor = if select { self.selection.anchor } else { head };
        self.set_selection(Selection { anchor, head }, cx);
    }

    pub fn select_range(&mut self, range: Range<Position>, cx: &mut Context<Self>) {
        self.set_selection(Selection { anchor: range.start, head: range.end }, cx);
    }

    pub fn go_to_line(&mut self, line: usize, cx: &mut Context<Self>) {
        let row = line.saturating_sub(1).min(self.buffer.line_count() - 1);
        self.goal_x = None;
        self.set_selection(Selection::caret(Position::new(row, 0)), cx);
    }

    pub fn selected_text(&self) -> String {
        self.buffer.slice(self.selection.range())
    }

    fn previous_grapheme(&self, position: Position) -> Position {
        if position.col == 0 {
            if position.row == 0 {
                return position;
            }
            return Position::new(position.row - 1, self.buffer.line(position.row - 1).len());
        }
        let line = self.buffer.line(position.row);
        let col = line[..position.col].grapheme_indices(true).last().map(|(i, _)| i).unwrap_or(0);
        Position::new(position.row, col)
    }

    fn next_grapheme(&self, position: Position) -> Position {
        let line = self.buffer.line(position.row);
        if position.col >= line.len() {
            if position.row + 1 >= self.buffer.line_count() {
                return position;
            }
            return Position::new(position.row + 1, 0);
        }
        let step = line[position.col..].graphemes(true).next().map(|g| g.len()).unwrap_or(1);
        Position::new(position.row, position.col + step)
    }

    fn previous_word(&self, position: Position) -> Position {
        if position.col == 0 {
            return self.previous_grapheme(position);
        }
        let line = self.buffer.line(position.row);
        let head = &line[..position.col];
        let col = head
            .split_word_bound_indices()
            .rev()
            .find(|(_, word)| !word.trim().is_empty())
            .map(|(i, _)| i)
            .unwrap_or(0);
        Position::new(position.row, col)
    }

    fn next_word(&self, position: Position) -> Position {
        let line = self.buffer.line(position.row);
        if position.col >= line.len() {
            return self.next_grapheme(position);
        }
        let tail = &line[position.col..];
        let mut words = tail.split_word_bound_indices().filter(|(_, word)| !word.trim().is_empty());
        let col = match words.next() {
            Some((start, word)) if start == 0 => position.col + word.len(),
            Some((start, _)) => position.col + start,
            None => line.len(),
        };
        Position::new(position.row, col)
    }

    fn vertical(&mut self, rows: isize, select: bool, cx: &mut Context<Self>) {
        let head = self.selection.head;
        let line_height = self.line_height();
        let Some((visual, x)) = self.visual_position(head) else {
            let row = (head.row as isize + rows).clamp(0, self.buffer.line_count() as isize - 1) as usize;
            let position = self.buffer.clamp(Position::new(row, head.col));
            self.move_head(position, select, cx);
            return;
        };
        let x = self.goal_x.unwrap_or(x);
        self.goal_x = Some(x);
        let total = self.total_rows() as isize;
        let target = visual as isize + rows;
        if target < 0 {
            self.move_head(Position::default(), select, cx);
            return;
        }
        if target >= total {
            let end = self.buffer.end();
            self.move_head(end, select, cx);
            return;
        }
        let position = self.position_for_visual(target as usize, x, line_height);
        let goal = self.goal_x;
        self.move_head(position, select, cx);
        self.goal_x = goal;
    }

    fn visual_position(&self, position: Position) -> Option<(usize, Pixels)> {
        let layout = self.layouts.get(position.row)?.as_ref()?;
        let local = layout.wrapped.position_for_index(position.col, self.line_height())?;
        let sub = (local.y.as_f32() / self.line_height().as_f32()).round() as usize;
        Some((self.row_starts[position.row] + sub, local.x))
    }

    fn position_for_visual(&self, visual: usize, x: Pixels, line_height: Pixels) -> Position {
        let row = self.line_for_visual_row(visual);
        let sub = visual - self.row_starts[row];
        let Some(layout) = self.layouts.get(row).and_then(|l| l.as_ref()) else {
            return Position::new(row, 0);
        };
        let local = point(x, line_height * (sub as f32) + line_height / 2.);
        let col = layout.wrapped.closest_index_for_position(local, line_height).unwrap_or_else(|ix| ix);
        self.buffer.clamp(Position::new(row, col))
    }

    fn line_for_visual_row(&self, visual: usize) -> usize {
        match self.row_starts.binary_search(&visual) {
            Ok(row) => row.min(self.buffer.line_count() - 1),
            Err(next) => next.saturating_sub(1).min(self.buffer.line_count() - 1),
        }
    }

    fn total_rows(&self) -> usize {
        *self.row_starts.last().unwrap_or(&1)
    }

    fn page_rows(&self) -> isize {
        let height = self.viewport.map(|b| b.size.height.as_f32()).unwrap_or(600.0);
        ((height / self.line_height().as_f32()) as isize - 2).max(1)
    }

    fn page(&mut self, direction: isize, select: bool, cx: &mut Context<Self>) {
        let rows = self.page_rows() * direction;
        self.vertical(rows, select, cx);
        let offset = self.scroll_handle.offset();
        self.scroll_handle.set_offset(point(offset.x, offset.y - self.line_height() * rows as f32));
    }

    fn move_left(&mut self, _: &MoveLeft, _: &mut Window, cx: &mut Context<Self>) {
        self.goal_x = None;
        if self.selection.is_empty() {
            self.move_head(self.previous_grapheme(self.selection.head), false, cx);
        } else {
            self.move_head(self.selection.range().start, false, cx);
        }
    }

    fn move_right(&mut self, _: &MoveRight, _: &mut Window, cx: &mut Context<Self>) {
        self.goal_x = None;
        if self.selection.is_empty() {
            self.move_head(self.next_grapheme(self.selection.head), false, cx);
        } else {
            self.move_head(self.selection.range().end, false, cx);
        }
    }

    fn move_up(&mut self, _: &MoveUp, _: &mut Window, cx: &mut Context<Self>) {
        if self.mode == EditorMode::SingleLine {
            cx.emit(EditorEvent::Navigate(-1));
            return;
        }
        self.vertical(-1, false, cx);
    }

    fn move_down(&mut self, _: &MoveDown, _: &mut Window, cx: &mut Context<Self>) {
        if self.mode == EditorMode::SingleLine {
            cx.emit(EditorEvent::Navigate(1));
            return;
        }
        self.vertical(1, false, cx);
    }

    fn shift_newline(&mut self, _: &ShiftNewline, _: &mut Window, cx: &mut Context<Self>) {
        if self.mode == EditorMode::SingleLine {
            cx.emit(EditorEvent::SubmitReverse);
            return;
        }
        self.insert("\n", cx);
        self.buffer.break_undo_group();
    }

    pub fn invalidate_layout(&mut self, cx: &mut Context<Self>) {
        self.layouts.iter_mut().for_each(|l| *l = None);
        self.layout_version = u64::MAX;
        self.autoscroll = true;
        cx.notify();
    }

    fn select_left(&mut self, _: &SelectLeft, _: &mut Window, cx: &mut Context<Self>) {
        self.goal_x = None;
        self.move_head(self.previous_grapheme(self.selection.head), true, cx);
    }

    fn select_right(&mut self, _: &SelectRight, _: &mut Window, cx: &mut Context<Self>) {
        self.goal_x = None;
        self.move_head(self.next_grapheme(self.selection.head), true, cx);
    }

    fn select_up(&mut self, _: &SelectUp, _: &mut Window, cx: &mut Context<Self>) {
        self.vertical(-1, true, cx);
    }

    fn select_down(&mut self, _: &SelectDown, _: &mut Window, cx: &mut Context<Self>) {
        self.vertical(1, true, cx);
    }

    fn move_word_left(&mut self, _: &MoveWordLeft, _: &mut Window, cx: &mut Context<Self>) {
        self.goal_x = None;
        self.move_head(self.previous_word(self.selection.head), false, cx);
    }

    fn move_word_right(&mut self, _: &MoveWordRight, _: &mut Window, cx: &mut Context<Self>) {
        self.goal_x = None;
        self.move_head(self.next_word(self.selection.head), false, cx);
    }

    fn select_word_left(&mut self, _: &SelectWordLeft, _: &mut Window, cx: &mut Context<Self>) {
        self.goal_x = None;
        self.move_head(self.previous_word(self.selection.head), true, cx);
    }

    fn select_word_right(&mut self, _: &SelectWordRight, _: &mut Window, cx: &mut Context<Self>) {
        self.goal_x = None;
        self.move_head(self.next_word(self.selection.head), true, cx);
    }

    fn line_start_position(&self) -> Position {
        let head = self.selection.head;
        let line = self.buffer.line(head.row);
        let indent = line.len() - line.trim_start().len();
        Position::new(head.row, if head.col == indent || head.col < indent { 0 } else { indent })
    }

    fn line_start(&mut self, _: &LineStart, _: &mut Window, cx: &mut Context<Self>) {
        self.goal_x = None;
        self.move_head(self.line_start_position(), false, cx);
    }

    fn line_end(&mut self, _: &LineEnd, _: &mut Window, cx: &mut Context<Self>) {
        self.goal_x = None;
        let head = self.selection.head;
        self.move_head(Position::new(head.row, self.buffer.line(head.row).len()), false, cx);
    }

    fn select_line_start(&mut self, _: &SelectLineStart, _: &mut Window, cx: &mut Context<Self>) {
        self.goal_x = None;
        self.move_head(self.line_start_position(), true, cx);
    }

    fn select_line_end(&mut self, _: &SelectLineEnd, _: &mut Window, cx: &mut Context<Self>) {
        self.goal_x = None;
        let head = self.selection.head;
        self.move_head(Position::new(head.row, self.buffer.line(head.row).len()), true, cx);
    }

    fn document_start(&mut self, _: &DocumentStart, _: &mut Window, cx: &mut Context<Self>) {
        self.move_head(Position::default(), false, cx);
    }

    fn document_end(&mut self, _: &DocumentEnd, _: &mut Window, cx: &mut Context<Self>) {
        self.move_head(self.buffer.end(), false, cx);
    }

    fn select_document_start(&mut self, _: &SelectDocumentStart, _: &mut Window, cx: &mut Context<Self>) {
        self.move_head(Position::default(), true, cx);
    }

    fn select_document_end(&mut self, _: &SelectDocumentEnd, _: &mut Window, cx: &mut Context<Self>) {
        self.move_head(self.buffer.end(), true, cx);
    }

    fn page_up(&mut self, _: &PageUp, _: &mut Window, cx: &mut Context<Self>) {
        self.page(-1, false, cx);
    }

    fn page_down(&mut self, _: &PageDown, _: &mut Window, cx: &mut Context<Self>) {
        self.page(1, false, cx);
    }

    fn select_page_up(&mut self, _: &SelectPageUp, _: &mut Window, cx: &mut Context<Self>) {
        self.page(-1, true, cx);
    }

    fn select_page_down(&mut self, _: &SelectPageDown, _: &mut Window, cx: &mut Context<Self>) {
        self.page(1, true, cx);
    }

    fn backspace(&mut self, _: &Backspace, _: &mut Window, cx: &mut Context<Self>) {
        if self.selection.is_empty() {
            let start = self.previous_grapheme(self.selection.head);
            if start == self.selection.head {
                return;
            }
            self.edit(start..self.selection.head, "", Selection::caret(start), true, cx);
        } else {
            let range = self.selection.range();
            self.edit(range.clone(), "", Selection::caret(range.start), false, cx);
        }
    }

    fn delete(&mut self, _: &Delete, _: &mut Window, cx: &mut Context<Self>) {
        if self.selection.is_empty() {
            let end = self.next_grapheme(self.selection.head);
            if end == self.selection.head {
                return;
            }
            self.edit(self.selection.head..end, "", Selection::caret(self.selection.head), true, cx);
        } else {
            let range = self.selection.range();
            self.edit(range.clone(), "", Selection::caret(range.start), false, cx);
        }
    }

    fn delete_word_left(&mut self, _: &DeleteWordLeft, _: &mut Window, cx: &mut Context<Self>) {
        let range = if self.selection.is_empty() { self.previous_word(self.selection.head)..self.selection.head } else { self.selection.range() };
        self.edit(range.clone(), "", Selection::caret(range.start), false, cx);
    }

    fn newline(&mut self, _: &Newline, _: &mut Window, cx: &mut Context<Self>) {
        if self.mode == EditorMode::SingleLine {
            cx.emit(EditorEvent::Submit);
            return;
        }
        let head = self.selection.head;
        let line = self.buffer.line(head.row).to_owned();
        let inside_fence = self.fences.get(head.row).is_some_and(|f| f.inside());
        if self.selection.is_empty() && head.col == line.len() && !inside_fence
            && let Some(marker) = highlight::list_marker(&line)
        {
            if marker.body_start >= line.len() && line[marker.body_start..].trim().is_empty() {
                let start = Position::new(head.row, 0);
                self.edit(start..head, "", Selection::caret(start), false, cx);
                return;
            }
            let indent = &line[..marker.indent];
            let bullet = match marker.number {
                Some(n) => format!("{}{}", n + 1, marker.delimiter),
                None => line[marker.indent..marker.marker_end].to_owned(),
            };
            let task = if marker.task.is_some() { "[ ] " } else { "" };
            let text = format!("\n{indent}{bullet} {task}");
            self.insert(&text, cx);
            self.buffer.break_undo_group();
            return;
        }
        let indent: String = line.chars().take_while(|c| *c == ' ' || *c == '\t').take(head.col).collect();
        self.insert(&format!("\n{indent}"), cx);
        self.buffer.break_undo_group();
    }

    fn selected_rows(&self) -> Range<usize> {
        let range = self.selection.range();
        let last = if range.end.col == 0 && range.end.row > range.start.row { range.end.row - 1 } else { range.end.row };
        range.start.row..last + 1
    }

    fn indent(&mut self, _: &Indent, _: &mut Window, cx: &mut Context<Self>) {
        if self.mode == EditorMode::SingleLine {
            return;
        }
        if self.selection.is_empty() {
            let width = 4 - self.selection.head.col % 4;
            self.insert(&" ".repeat(width), cx);
            return;
        }
        self.shift_rows(true, cx);
    }

    fn outdent(&mut self, _: &Outdent, _: &mut Window, cx: &mut Context<Self>) {
        if self.mode == EditorMode::SingleLine {
            return;
        }
        self.shift_rows(false, cx);
    }

    fn shift_rows(&mut self, inward: bool, cx: &mut Context<Self>) {
        let rows = self.selected_rows();
        let mut lines = Vec::with_capacity(rows.len());
        let mut removed_first = 0;
        let mut removed_last = 0;
        for row in rows.clone() {
            let line = self.buffer.line(row);
            let (text, removed) = if inward {
                (format!("    {line}"), 0)
            } else {
                let spaces = line.len() - line.trim_start_matches(' ').len();
                let strip = if line.starts_with('\t') { 1 } else { spaces.min(4) };
                (line[strip..].to_owned(), strip)
            };
            if row == rows.start {
                removed_first = removed;
            }
            if row + 1 == rows.end {
                removed_last = removed;
            }
            lines.push(text);
        }
        let start = Position::new(rows.start, 0);
        let end = Position::new(rows.end - 1, self.buffer.line(rows.end - 1).len());
        let shift = |position: Position, removed: usize| {
            if position.row < rows.start || position.row >= rows.end {
                return position;
            }
            let col = if inward { position.col + 4 } else { position.col.saturating_sub(removed) };
            Position::new(position.row, col)
        };
        let anchor = shift(self.selection.anchor, if self.selection.anchor.row == rows.start { removed_first } else { removed_last });
        let head = shift(self.selection.head, if self.selection.head.row == rows.start { removed_first } else { removed_last });
        self.edit(start..end, &lines.join("\n"), Selection { anchor, head }, false, cx);
    }

    fn select_all(&mut self, _: &SelectAll, _: &mut Window, cx: &mut Context<Self>) {
        let end = self.buffer.end();
        self.set_selection(Selection { anchor: Position::default(), head: end }, cx);
        self.autoscroll = false;
    }

    fn copy(&mut self, _: &Copy, _: &mut Window, cx: &mut Context<Self>) {
        let text = if self.selection.is_empty() { format!("{}\n", self.buffer.line(self.selection.head.row)) } else { self.selected_text() };
        cx.write_to_clipboard(ClipboardItem::new_string(text));
    }

    fn cut(&mut self, _: &Cut, _: &mut Window, cx: &mut Context<Self>) {
        if self.selection.is_empty() {
            let row = self.selection.head.row;
            cx.write_to_clipboard(ClipboardItem::new_string(format!("{}\n", self.buffer.line(row))));
            self.remove_rows(row..row + 1, cx);
            return;
        }
        cx.write_to_clipboard(ClipboardItem::new_string(self.selected_text()));
        let range = self.selection.range();
        self.edit(range.clone(), "", Selection::caret(range.start), false, cx);
    }

    fn paste(&mut self, _: &Paste, _: &mut Window, cx: &mut Context<Self>) {
        let Some(text) = cx.read_from_clipboard().and_then(|item| item.text()) else {
            return;
        };
        let text = text.replace("\r\n", "\n").replace('\r', "\n");
        let text = if self.mode == EditorMode::SingleLine { text.replace('\n', " ") } else { text };
        self.insert(&text, cx);
        self.buffer.break_undo_group();
    }

    fn undo(&mut self, _: &Undo, _: &mut Window, cx: &mut Context<Self>) {
        if let Some(selection) = self.buffer.undo() {
            self.selection = selection;
            self.rebuild_metadata();
            self.after_edit(cx);
        }
    }

    fn redo(&mut self, _: &Redo, _: &mut Window, cx: &mut Context<Self>) {
        if let Some(selection) = self.buffer.redo() {
            self.selection = selection;
            self.rebuild_metadata();
            self.after_edit(cx);
        }
    }

    fn remove_rows(&mut self, rows: Range<usize>, cx: &mut Context<Self>) {
        let count = self.buffer.line_count();
        let (start, end) = if rows.end < count {
            (Position::new(rows.start, 0), Position::new(rows.end, 0))
        } else if rows.start > 0 {
            (Position::new(rows.start - 1, self.buffer.line(rows.start - 1).len()), self.buffer.end())
        } else {
            (Position::default(), self.buffer.end())
        };
        let caret = self.buffer.clamp(Position::new(start.row.min(count.saturating_sub(rows.len() + 1).max(0)), 0));
        let caret = if rows.end < count { Position::new(rows.start, 0) } else { caret };
        self.edit(start..end, "", Selection::caret(caret), false, cx);
    }

    fn duplicate_line(&mut self, _: &DuplicateLine, _: &mut Window, cx: &mut Context<Self>) {
        let rows = self.selected_rows();
        let text: Vec<&str> = (rows.clone()).map(|row| self.buffer.line(row)).collect();
        let block = text.join("\n");
        let end = Position::new(rows.end - 1, self.buffer.line(rows.end - 1).len());
        let shift = rows.len();
        let after = Selection {
            anchor: Position::new(self.selection.anchor.row + shift, self.selection.anchor.col),
            head: Position::new(self.selection.head.row + shift, self.selection.head.col),
        };
        self.edit(end..end, &format!("\n{block}"), after, false, cx);
    }

    fn delete_line(&mut self, _: &DeleteLine, _: &mut Window, cx: &mut Context<Self>) {
        self.remove_rows(self.selected_rows(), cx);
    }

    fn move_line(&mut self, delta: isize, cx: &mut Context<Self>) {
        let rows = self.selected_rows();
        let count = self.buffer.line_count();
        if (delta < 0 && rows.start == 0) || (delta > 0 && rows.end >= count) {
            return;
        }
        let (block_start, block_end, swap) = if delta < 0 { (rows.start - 1, rows.end, rows.start - 1) } else { (rows.start, rows.end + 1, rows.end) };
        let mut lines: Vec<String> = (block_start..block_end).map(|row| self.buffer.line(row).to_owned()).collect();
        let moved = lines.remove(swap - block_start);
        if delta < 0 {
            lines.push(moved);
        } else {
            lines.insert(0, moved);
        }
        let start = Position::new(block_start, 0);
        let end = Position::new(block_end - 1, self.buffer.line(block_end - 1).len());
        let shift = |p: Position| Position::new((p.row as isize + delta) as usize, p.col);
        let after = Selection { anchor: shift(self.selection.anchor), head: shift(self.selection.head) };
        self.edit(start..end, &lines.join("\n"), after, false, cx);
    }

    fn move_line_up(&mut self, _: &MoveLineUp, _: &mut Window, cx: &mut Context<Self>) {
        self.move_line(-1, cx);
    }

    fn move_line_down(&mut self, _: &MoveLineDown, _: &mut Window, cx: &mut Context<Self>) {
        self.move_line(1, cx);
    }

    pub fn wrap_selection(&mut self, before: &str, after: &str, placeholder: &str, cx: &mut Context<Self>) {
        let range = self.selection.range();
        let selected = self.buffer.slice(range.clone());
        let content = if selected.is_empty() { placeholder } else { selected.as_str() };
        if !selected.is_empty() && selected.starts_with(before) && selected.ends_with(after) && selected.len() >= before.len() + after.len() {
            let inner = &selected[before.len()..selected.len() - after.len()];
            let end = Buffer::end_of(range.start, inner);
            self.edit(range.clone(), inner, Selection { anchor: range.start, head: end }, false, cx);
            return;
        }
        let text = format!("{before}{content}{after}");
        let inner_start = Buffer::end_of(range.start, before);
        let inner_end = Buffer::end_of(inner_start, content);
        self.edit(range, &text, Selection { anchor: inner_start, head: inner_end }, false, cx);
    }

    fn toggle_bold(&mut self, _: &ToggleBold, _: &mut Window, cx: &mut Context<Self>) {
        self.wrap_selection("**", "**", "bold", cx);
    }

    fn toggle_italic(&mut self, _: &ToggleItalic, _: &mut Window, cx: &mut Context<Self>) {
        self.wrap_selection("*", "*", "italic", cx);
    }

    fn toggle_inline_code(&mut self, _: &ToggleInlineCode, _: &mut Window, cx: &mut Context<Self>) {
        self.wrap_selection("`", "`", "code", cx);
    }

    fn insert_link(&mut self, _: &InsertLink, _: &mut Window, cx: &mut Context<Self>) {
        let selected = self.selected_text();
        if selected.starts_with("http://") || selected.starts_with("https://") {
            let range = self.selection.range();
            let text = format!("[title]({selected})");
            let start = Buffer::end_of(range.start, "[");
            self.edit(range, &text, Selection { anchor: start, head: Buffer::end_of(start, "title") }, false, cx);
            return;
        }
        let range = self.selection.range();
        let label = if selected.is_empty() { "link text" } else { selected.as_str() };
        let text = format!("[{label}](https://)");
        let url_start = Buffer::end_of(range.start, &format!("[{label}]("));
        let url_end = Buffer::end_of(url_start, "https://");
        self.edit(range, &text, Selection { anchor: url_start, head: url_end }, false, cx);
    }

    pub fn insert_block(&mut self, language: &str, cx: &mut Context<Self>) {
        let head = self.selection.head;
        let line = self.buffer.line(head.row);
        let prefix = if head.col > 0 && !line[..head.col].trim().is_empty() { "\n\n" } else if head.col > 0 { "\n" } else { "" };
        let text = format!("{prefix}```{language}\n\n```\n");
        let range = self.selection.range();
        let inside = Buffer::end_of(range.start, &format!("{prefix}```{language}\n"));
        self.edit(range, &text, Selection::caret(inside), false, cx);
    }

    fn insert_code_block(&mut self, _: &InsertCodeBlock, _: &mut Window, cx: &mut Context<Self>) {
        self.insert_block("", cx);
    }

    fn insert_mermaid_block(&mut self, _: &InsertMermaidBlock, _: &mut Window, cx: &mut Context<Self>) {
        self.insert_block("mermaid", cx);
    }

    fn cancel(&mut self, _: &Cancel, _: &mut Window, cx: &mut Context<Self>) {
        if self.mode == EditorMode::SingleLine {
            cx.emit(EditorEvent::Cancel);
            return;
        }
        if !self.selection.is_empty() {
            self.set_selection(Selection::caret(self.selection.head), cx);
        } else {
            cx.emit(EditorEvent::Cancel);
        }
    }

    pub fn set_search(&mut self, query: &str, cx: &mut Context<Self>) {
        self.search.query = query.to_owned();
        self.search.current = None;
        self.refresh_search();
        cx.notify();
    }

    fn refresh_search(&mut self) {
        self.search.matches.clear();
        let query = self.search.query.to_lowercase();
        if query.is_empty() {
            self.search.current = None;
            return;
        }
        for (row, line) in self.buffer.lines().iter().enumerate() {
            let lower = line.to_lowercase();
            if lower.len() != line.len() {
                for (col, _) in line.match_indices(&self.search.query) {
                    self.search.matches.push(Position::new(row, col)..Position::new(row, col + query.len()));
                }
                continue;
            }
            for (col, _) in lower.match_indices(&query) {
                self.search.matches.push(Position::new(row, col)..Position::new(row, col + query.len()));
            }
        }
        if self.search.current.is_some_and(|ix| ix >= self.search.matches.len()) {
            self.search.current = None;
        }
    }

    pub fn search_step(&mut self, forward: bool, cx: &mut Context<Self>) {
        if self.search.matches.is_empty() {
            return;
        }
        let count = self.search.matches.len();
        let head = self.selection.head;
        let next = match self.search.current {
            Some(ix) if forward => (ix + 1) % count,
            Some(ix) => (ix + count - 1) % count,
            None if forward => self.search.matches.iter().position(|m| m.start >= head).unwrap_or(0),
            None => self.search.matches.iter().rposition(|m| m.start < head).unwrap_or(count - 1),
        };
        self.search.current = Some(next);
        let range = self.search.matches[next].clone();
        self.set_selection(Selection { anchor: range.start, head: range.end }, cx);
    }

    pub fn replace_current(&mut self, replacement: &str, cx: &mut Context<Self>) {
        let Some(ix) = self.search.current else {
            self.search_step(true, cx);
            return;
        };
        let Some(range) = self.search.matches.get(ix).cloned() else {
            return;
        };
        let end = Buffer::end_of(range.start, replacement);
        self.edit(range, replacement, Selection::caret(end), false, cx);
        self.search.current = None;
        self.search_step(true, cx);
    }

    pub fn replace_all(&mut self, replacement: &str, cx: &mut Context<Self>) -> usize {
        let matches = std::mem::take(&mut self.search.matches);
        let count = matches.len();
        for range in matches.into_iter().rev() {
            let end = Buffer::end_of(range.start, replacement);
            self.edit(range, replacement, Selection::caret(end), false, cx);
        }
        count
    }

    fn on_mouse_down(&mut self, event: &MouseDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        self.focus(window, cx);
        let position = self.position_for_point(event.position);
        self.goal_x = None;
        self.is_selecting = true;
        match event.click_count {
            2 => {
                let line = self.buffer.line(position.row);
                let (start, end) = line
                    .split_word_bound_indices()
                    .find(|(i, word)| position.col >= *i && position.col < i + word.len())
                    .map(|(i, word)| (i, i + word.len()))
                    .unwrap_or((position.col, position.col));
                self.set_selection(Selection { anchor: Position::new(position.row, start), head: Position::new(position.row, end) }, cx);
            }
            3 => {
                let end = if position.row + 1 < self.buffer.line_count() {
                    Position::new(position.row + 1, 0)
                } else {
                    Position::new(position.row, self.buffer.line(position.row).len())
                };
                self.set_selection(Selection { anchor: Position::new(position.row, 0), head: end }, cx);
            }
            _ => self.move_head(position, event.modifiers.shift, cx),
        }
        self.autoscroll = false;
    }

    fn on_mouse_up(&mut self, _: &MouseUpEvent, _: &mut Window, _: &mut Context<Self>) {
        self.is_selecting = false;
    }

    fn on_mouse_move(&mut self, event: &MouseMoveEvent, _: &mut Window, cx: &mut Context<Self>) {
        if self.is_selecting && event.dragging() {
            let position = self.position_for_point(event.position);
            self.move_head(position, true, cx);
            self.autoscroll = false;
        }
    }

    fn position_for_point(&self, point_in_window: Point<Pixels>) -> Position {
        let Some(bounds) = self.last_bounds else {
            return Position::default();
        };
        let line_height = self.line_height();
        let y = point_in_window.y - bounds.top() - px(PAD_TOP);
        if y < px(0.) {
            return Position::default();
        }
        let visual = (y.as_f32() / line_height.as_f32()) as usize;
        if visual >= self.total_rows() {
            return self.buffer.end();
        }
        let x = point_in_window.x - bounds.left() - self.text_origin.x;
        self.position_for_visual(visual, x, line_height)
    }

    fn utf16_offset(&self, position: Position) -> usize {
        let before: usize = self.utf16_lens[..position.row].iter().map(|len| len + 1).sum();
        before + self.buffer.line(position.row)[..position.col].encode_utf16().count()
    }

    fn position_from_utf16(&self, mut offset: usize) -> Position {
        for (row, len) in self.utf16_lens.iter().enumerate() {
            if offset <= *len {
                let line = self.buffer.line(row);
                let mut col = 0;
                for (i, ch) in line.char_indices() {
                    if offset == 0 {
                        return Position::new(row, i);
                    }
                    offset = offset.saturating_sub(ch.len_utf16());
                    col = i + ch.len_utf8();
                }
                return Position::new(row, col);
            }
            offset -= len + 1;
        }
        self.buffer.end()
    }

    fn range_from_utf16(&self, range: &Range<usize>) -> Range<Position> {
        self.position_from_utf16(range.start)..self.position_from_utf16(range.end)
    }

    fn range_to_utf16(&self, range: &Range<Position>) -> Range<usize> {
        self.utf16_offset(range.start)..self.utf16_offset(range.end)
    }

    fn ensure_layouts(&mut self, width: Pixels, window: &mut Window, theme: &Theme) {
        let wrap_width = if self.wrap && self.mode == EditorMode::Document { Some(width) } else { None };
        let changed = self.layout_width != wrap_width || self.layout_version != self.buffer.version();
        if self.layout_width != wrap_width {
            self.layouts.iter_mut().for_each(|l| *l = None);
            self.layout_width = wrap_width;
        }
        if !changed && self.layouts.iter().all(|l| l.is_some()) {
            return;
        }
        let font_size = self.font_size;
        let mut tokens = Vec::new();
        for row in 0..self.buffer.line_count() {
            if self.layouts[row].is_some() {
                continue;
            }
            let text = self.buffer.line(row);
            let runs = self.runs_for_line(row, text, theme, &mut tokens);
            let shaped = window
                .text_system()
                .shape_text(SharedString::from(text.to_owned()), font_size, &runs, wrap_width, None)
                .ok()
                .and_then(|mut lines| lines.pop())
                .unwrap_or_default();
            let rows = shaped.wrap_boundaries().len() + 1;
            self.layouts[row] = Some(LineLayout { wrapped: shaped, rows });
        }
        let mut total = 0;
        self.row_starts.clear();
        for layout in &self.layouts {
            self.row_starts.push(total);
            total += layout.as_ref().map(|l| l.rows).unwrap_or(1);
        }
        self.row_starts.push(total);
        self.layout_version = self.buffer.version();
    }

    fn runs_for_line(&self, row: usize, text: &str, theme: &Theme, tokens: &mut Vec<(Range<usize>, Token)>) -> Vec<TextRun> {
        let base_font = font(if self.mode == EditorMode::SingleLine { theme.sans.clone() } else { theme.mono.clone() });
        let plain = TextRun { len: text.len(), font: base_font, color: theme.text, background_color: None, underline: None, strikethrough: None };
        if !self.highlighting || text.is_empty() {
            return vec![plain];
        }
        let state = self.fences.get(row).copied().unwrap_or_default();
        highlight::line_tokens(text, state, tokens);
        if tokens.is_empty() {
            return vec![plain];
        }
        let mut boundaries: Vec<usize> = vec![0, text.len()];
        for (range, _) in tokens.iter() {
            boundaries.push(range.start);
            boundaries.push(range.end);
        }
        boundaries.sort_unstable();
        boundaries.dedup();
        let mut runs = Vec::with_capacity(boundaries.len());
        for pair in boundaries.windows(2) {
            let (start, end) = (pair[0], pair[1]);
            if start == end {
                continue;
            }
            let mut run = TextRun { len: end - start, ..plain.clone() };
            for (range, token) in tokens.iter() {
                if range.start <= start && end <= range.end {
                    apply_token(&mut run, *token, theme);
                }
            }
            runs.push(run);
        }
        runs
    }
}

fn apply_token(run: &mut TextRun, token: Token, theme: &Theme) {
    let syntax = &theme.syntax;
    match token {
        Token::Heading => {
            run.color = syntax.heading;
            run.font.weight = FontWeight::BOLD;
        }
        Token::Marker => run.color = syntax.marker,
        Token::Code => run.color = syntax.code,
        Token::Bold => run.font.weight = FontWeight::BOLD,
        Token::Italic => run.font.style = FontStyle::Italic,
        Token::Link => run.color = syntax.link,
        Token::Quote => {
            run.color = syntax.quote;
            run.font.style = FontStyle::Italic;
        }
        Token::Fence => run.color = theme.faint,
        Token::String => run.color = syntax.string,
        Token::Keyword => run.color = syntax.keyword,
        Token::Comment => run.color = syntax.comment,
        Token::Number => run.color = syntax.number,
    }
}

impl EventEmitter<EditorEvent> for Editor {}

impl Focusable for Editor {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl EntityInputHandler for Editor {
    fn text_for_range(&mut self, range_utf16: Range<usize>, actual_range: &mut Option<Range<usize>>, _: &mut Window, _: &mut Context<Self>) -> Option<String> {
        let range = self.range_from_utf16(&range_utf16);
        actual_range.replace(self.range_to_utf16(&range));
        Some(self.buffer.slice(range))
    }

    fn selected_text_range(&mut self, _: bool, _: &mut Window, _: &mut Context<Self>) -> Option<UTF16Selection> {
        Some(UTF16Selection { range: self.range_to_utf16(&self.selection.range()), reversed: self.selection.reversed() })
    }

    fn marked_text_range(&self, _: &mut Window, _: &mut Context<Self>) -> Option<Range<usize>> {
        self.marked.as_ref().map(|range| self.range_to_utf16(range))
    }

    fn unmark_text(&mut self, _: &mut Window, _: &mut Context<Self>) {
        self.marked = None;
    }

    fn replace_text_in_range(&mut self, range_utf16: Option<Range<usize>>, new_text: &str, _: &mut Window, cx: &mut Context<Self>) {
        let range = range_utf16
            .as_ref()
            .map(|range| self.range_from_utf16(range))
            .or_else(|| self.marked.clone())
            .unwrap_or_else(|| self.selection.range());
        let text = if self.mode == EditorMode::SingleLine { new_text.replace('\n', " ") } else { new_text.to_owned() };
        let end = Buffer::end_of(range.start, &text);
        let coalesce = text.chars().count() == 1 && !text.contains('\n');
        self.edit(range, &text, Selection::caret(end), coalesce, cx);
    }

    fn replace_and_mark_text_in_range(
        &mut self,
        range_utf16: Option<Range<usize>>,
        new_text: &str,
        new_selected_range_utf16: Option<Range<usize>>,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let range = range_utf16
            .as_ref()
            .map(|range| self.range_from_utf16(range))
            .or_else(|| self.marked.clone())
            .unwrap_or_else(|| self.selection.range());
        let end = Buffer::end_of(range.start, new_text);
        let selection = match new_selected_range_utf16 {
            Some(selected) => {
                let base = self.utf16_offset(range.start);
                let start = self.position_from_utf16(base + selected.start);
                let finish = self.position_from_utf16(base + selected.end);
                Selection { anchor: start, head: finish }
            }
            None => Selection::caret(end),
        };
        let start = range.start;
        self.edit(range, new_text, Selection::caret(end), false, cx);
        self.marked = (!new_text.is_empty()).then_some(start..end);
        self.selection = Selection { anchor: self.buffer.clamp(selection.anchor), head: self.buffer.clamp(selection.head) };
    }

    fn bounds_for_range(&mut self, range_utf16: Range<usize>, element_bounds: Bounds<Pixels>, _: &mut Window, _: &mut Context<Self>) -> Option<Bounds<Pixels>> {
        let range = self.range_from_utf16(&range_utf16);
        let line_height = self.line_height();
        let (visual, x) = self.visual_position(range.start)?;
        let origin = point(
            element_bounds.left() + self.text_origin.x + x,
            element_bounds.top() + px(PAD_TOP) + line_height * visual as f32,
        );
        Some(Bounds::new(origin, size(px(2.), line_height)))
    }

    fn character_index_for_point(&mut self, point: Point<Pixels>, _: &mut Window, _: &mut Context<Self>) -> Option<usize> {
        let position = self.position_for_point(point);
        Some(self.utf16_offset(position))
    }
}

impl Render for Editor {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let single = self.mode == EditorMode::SingleLine;
        let theme = cx.global::<Theme>();
        let mut container = div()
            .id(("editor-scroll", cx.entity_id()))
            .key_context("Editor")
            .track_focus(&self.focus_handle)
            .cursor(CursorStyle::IBeam)
            .on_action(cx.listener(Self::move_left))
            .on_action(cx.listener(Self::move_right))
            .on_action(cx.listener(Self::move_up))
            .on_action(cx.listener(Self::move_down))
            .on_action(cx.listener(Self::select_left))
            .on_action(cx.listener(Self::select_right))
            .on_action(cx.listener(Self::select_up))
            .on_action(cx.listener(Self::select_down))
            .on_action(cx.listener(Self::move_word_left))
            .on_action(cx.listener(Self::move_word_right))
            .on_action(cx.listener(Self::select_word_left))
            .on_action(cx.listener(Self::select_word_right))
            .on_action(cx.listener(Self::line_start))
            .on_action(cx.listener(Self::line_end))
            .on_action(cx.listener(Self::select_line_start))
            .on_action(cx.listener(Self::select_line_end))
            .on_action(cx.listener(Self::document_start))
            .on_action(cx.listener(Self::document_end))
            .on_action(cx.listener(Self::select_document_start))
            .on_action(cx.listener(Self::select_document_end))
            .on_action(cx.listener(Self::page_up))
            .on_action(cx.listener(Self::page_down))
            .on_action(cx.listener(Self::select_page_up))
            .on_action(cx.listener(Self::select_page_down))
            .on_action(cx.listener(Self::backspace))
            .on_action(cx.listener(Self::delete))
            .on_action(cx.listener(Self::delete_word_left))
            .on_action(cx.listener(Self::newline))
            .on_action(cx.listener(Self::shift_newline))
            .on_action(cx.listener(Self::indent))
            .on_action(cx.listener(Self::outdent))
            .on_action(cx.listener(Self::select_all))
            .on_action(cx.listener(Self::copy))
            .on_action(cx.listener(Self::cut))
            .on_action(cx.listener(Self::paste))
            .on_action(cx.listener(Self::undo))
            .on_action(cx.listener(Self::redo))
            .on_action(cx.listener(Self::duplicate_line))
            .on_action(cx.listener(Self::delete_line))
            .on_action(cx.listener(Self::move_line_up))
            .on_action(cx.listener(Self::move_line_down))
            .on_action(cx.listener(Self::toggle_bold))
            .on_action(cx.listener(Self::toggle_italic))
            .on_action(cx.listener(Self::toggle_inline_code))
            .on_action(cx.listener(Self::insert_link))
            .on_action(cx.listener(Self::insert_code_block))
            .on_action(cx.listener(Self::insert_mermaid_block))
            .on_action(cx.listener(Self::cancel))
            .on_mouse_down(MouseButton::Left, cx.listener(Self::on_mouse_down))
            .on_mouse_up(MouseButton::Left, cx.listener(Self::on_mouse_up))
            .on_mouse_up_out(MouseButton::Left, cx.listener(Self::on_mouse_up))
            .on_mouse_move(cx.listener(Self::on_mouse_move))
            .font_family(if single { theme.sans.clone() } else { theme.mono.clone() })
            .text_size(self.font_size)
            .line_height(self.line_height())
            .text_color(theme.text);
        if single {
            container = container.w_full().h(self.line_height()).overflow_hidden();
        } else {
            container = container.size_full().overflow_y_scroll().track_scroll(&self.scroll_handle);
            if !self.wrap {
                container = container.overflow_x_scroll();
            }
        }
        container.child(EditorElement { editor: cx.entity() })
    }
}

pub struct EditorElement {
    editor: Entity<Editor>,
}

pub struct EditorPrepaint {
    rows: Vec<(usize, Point<Pixels>)>,
    quads: Vec<gpui::PaintQuad>,
    cursor: Option<gpui::PaintQuad>,
    gutter: Vec<(ShapedLine, Point<Pixels>)>,
    placeholder: Option<(ShapedLine, Point<Pixels>)>,
    line_height: Pixels,
}

impl IntoElement for EditorElement {
    type Element = Self;

    fn into_element(self) -> Self {
        self
    }
}

impl Element for EditorElement {
    type RequestLayoutState = ();
    type PrepaintState = EditorPrepaint;

    fn id(&self) -> Option<ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static core::panic::Location<'static>> {
        None
    }

    fn request_layout(&mut self, _: Option<&GlobalElementId>, _: Option<&InspectorElementId>, window: &mut Window, cx: &mut App) -> (LayoutId, ()) {
        let editor = self.editor.clone();
        let single = editor.read(cx).mode == EditorMode::SingleLine;
        let mut style = Style::default();
        style.size.width = relative(1.).into();
        if single {
            style.size.height = editor.read(cx).line_height().into();
            return (window.request_layout(style, [], cx), ());
        }
        let layout = window.request_measured_layout(style, move |known, available, window, cx| {
            let theme = cx.global::<Theme>().clone();
            let width = known.width.unwrap_or(match available.width {
                gpui::AvailableSpace::Definite(w) => w,
                _ => px(800.),
            });
            editor.update(cx, |editor, _| {
                let gutter = gutter_width(editor);
                let max_text = px(820.);
                let inset = px(TEXT_INSET);
                let text_width = (width - gutter - inset * 2.).max(px(120.));
                let text_width = if editor.centered { text_width.min(max_text) } else { text_width };
                let left_pad = if editor.centered { ((width - gutter - text_width) / 2.).max(inset) } else { inset };
                editor.text_origin = point(left_pad + gutter, px(PAD_TOP));
                editor.ensure_layouts(text_width, window, &theme);
                let rows = editor.total_rows() as f32;
                let height = px(PAD_TOP + PAD_BOTTOM) + editor.line_height() * rows;
                let content_width = if editor.wrap {
                    width
                } else {
                    editor
                        .layouts
                        .iter()
                        .flatten()
                        .map(|l| l.wrapped.width())
                        .fold(px(0.), |a, b| if b > a { b } else { a })
                        + left_pad
                        + gutter
                        + inset
                };
                size(content_width.max(width), height)
            })
        });
        (layout, ())
    }

    fn prepaint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _: &mut (),
        window: &mut Window,
        cx: &mut App,
    ) -> EditorPrepaint {
        let theme = cx.global::<Theme>().clone();
        let mask = window.content_mask().bounds;
        self.editor.update(cx, |editor, _| {
            let single = editor.mode == EditorMode::SingleLine;
            let line_height = editor.line_height();
            editor.last_bounds = Some(bounds);
            if single {
                editor.text_origin = point(px(0.), px(0.));
                editor.ensure_layouts(bounds.size.width, window, &theme);
                editor.viewport = Some(bounds);
            } else {
                editor.viewport = Some(mask);
            }
            let pad_top = if single { px(0.) } else { px(PAD_TOP) };
            if editor.autoscroll && !single {
                editor.autoscroll = false;
                if let Some((visual, _)) = editor.visual_position(editor.selection.head) {
                    let cursor_top = pad_top + line_height * visual as f32;
                    let cursor_bottom = cursor_top + line_height;
                    let offset = editor.scroll_handle.offset();
                    let view_top = -offset.y;
                    let view_height = mask.size.height;
                    let margin = line_height * 2.;
                    let mut new_top = view_top;
                    if cursor_top - margin < view_top {
                        new_top = (cursor_top - margin).max(px(0.));
                    } else if cursor_bottom + margin > view_top + view_height {
                        new_top = cursor_bottom + margin - view_height;
                    }
                    if new_top != view_top {
                        editor.scroll_handle.set_offset(point(offset.x, -new_top));
                        window.refresh();
                    }
                }
            }
            if single {
                let offset_x = editor.text_origin.x;
                if let Some((_, x)) = editor.visual_position(editor.selection.head) {
                    let width = bounds.size.width - px(4.);
                    let shifted = if x + offset_x > width { width - x } else if x + offset_x < px(0.) { -x } else { offset_x };
                    editor.text_origin.x = shifted.min(px(0.));
                }
            }
            let origin = point(bounds.left() + editor.text_origin.x, bounds.top() + pad_top);
            let first_visual = (((mask.top() - origin.y).as_f32() / line_height.as_f32()).floor().max(0.0)) as usize;
            let last_visual = (((mask.bottom() - origin.y).as_f32() / line_height.as_f32()).ceil().max(0.0)) as usize;
            let total = editor.total_rows();
            let first_row = editor.line_for_visual_row(first_visual.min(total.saturating_sub(1)));
            let mut rows = Vec::new();
            let mut quads = Vec::new();
            let mut gutter = Vec::new();
            let selection = editor.selection.range();
            let head = editor.selection.head;
            let focused = editor.focus_handle.is_focused(window);
            let number_font = font(theme.mono.clone());
            let number_size = editor.font_size * 0.86;
            let mut cursor = None;
            let text_width = editor.layout_width.unwrap_or(bounds.size.width - editor.text_origin.x);
            let mut row = first_row;
            while row < editor.buffer.line_count() && editor.row_starts[row] <= last_visual {
                let Some(layout) = editor.layouts[row].as_ref() else {
                    row += 1;
                    continue;
                };
                let y = origin.y + line_height * editor.row_starts[row] as f32;
                rows.push((row, point(origin.x, y)));
                if !single && row == head.row && selection.start == selection.end {
                    quads.push(fill(
                        Bounds::new(point(bounds.left(), y), size(bounds.size.width, line_height * layout.rows as f32)),
                        theme.current_line,
                    ));
                }
                if editor.line_numbers && !single {
                    let color = if row == head.row { theme.accent } else { theme.faint };
                    let run = TextRun { len: 0, font: number_font.clone(), color, background_color: None, underline: None, strikethrough: None };
                    let label = SharedString::from((row + 1).to_string());
                    let run = TextRun { len: label.len(), ..run };
                    let shaped = window.text_system().shape_line(label, number_size, &[run], None);
                    let x = origin.x - px(GUTTER_GAP) - shaped.width;
                    gutter.push((shaped, point(x, y)));
                }
                for (range, color, pad) in editor
                    .search
                    .matches
                    .iter()
                    .enumerate()
                    .filter(|(_, m)| m.start.row == row)
                    .map(|(ix, m)| (m.clone(), if Some(ix) == editor.search.current { theme.accent.alpha(0.55) } else { theme.warning.alpha(0.28) }, true))
                    .chain(std::iter::once((selection.clone(), theme.selection, false)))
                {
                    if range.start == range.end || range.end.row < row || range.start.row > row {
                        continue;
                    }
                    let start_col = if range.start.row == row { range.start.col } else { 0 };
                    let end_col = if range.end.row == row { range.end.col } else { editor.buffer.line(row).len() };
                    let line_height_f = line_height;
                    let Some(start_pt) = layout.wrapped.position_for_index(start_col, line_height_f) else { continue };
                    let Some(end_pt) = layout.wrapped.position_for_index(end_col, line_height_f) else { continue };
                    let start_sub = (start_pt.y.as_f32() / line_height_f.as_f32()).round() as usize;
                    let end_sub = (end_pt.y.as_f32() / line_height_f.as_f32()).round() as usize;
                    for sub in start_sub..=end_sub {
                        let x0 = if sub == start_sub { start_pt.x } else { px(0.) };
                        let mut x1 = if sub == end_sub { end_pt.x } else { text_width };
                        if range.end.row > row && sub == end_sub && !pad {
                            x1 = x1 + px(8.);
                        }
                        if x1 <= x0 && !(range.end.row > row && sub == end_sub) {
                            continue;
                        }
                        let rect = Bounds::new(point(origin.x + x0, y + line_height * sub as f32), size((x1 - x0).max(px(2.)), line_height));
                        if pad {
                            quads.push(gpui::quad(rect, px(3.), color, px(0.), gpui::transparent_black(), gpui::BorderStyle::default()));
                        } else {
                            quads.push(fill(rect, color));
                        }
                    }
                }
                if focused && row == head.row
                    && let Some(local) = layout.wrapped.position_for_index(head.col, line_height)
                {
                    cursor = Some(fill(Bounds::new(point(origin.x + local.x, y + local.y + px(2.)), size(px(2.), line_height - px(4.))), theme.cursor));
                }
                if let Some(marked) = editor.marked.as_ref()
                    && marked.start.row == row
                    && let Some(s) = layout.wrapped.position_for_index(marked.start.col, line_height)
                    && let Some(e) = layout.wrapped.position_for_index(marked.end.col.min(editor.buffer.line(row).len()), line_height)
                {
                    quads.push(fill(Bounds::new(point(origin.x + s.x, y + s.y + line_height - px(2.)), size(e.x - s.x, px(1.))), theme.accent));
                }
                row += 1;
            }
            let placeholder = if editor.buffer.line_count() == 1 && editor.buffer.line(0).is_empty() && !editor.placeholder.is_empty() {
                let run = TextRun {
                    len: editor.placeholder.len(),
                    font: font(if single { theme.sans.clone() } else { theme.mono.clone() }),
                    color: theme.faint,
                    background_color: None,
                    underline: None,
                    strikethrough: None,
                };
                let shaped = window.text_system().shape_line(editor.placeholder.clone(), editor.font_size, &[run], None);
                Some((shaped, origin))
            } else {
                None
            };
            EditorPrepaint { rows, quads, cursor, gutter, placeholder, line_height }
        })
    }

    fn paint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _: &mut (),
        prepaint: &mut EditorPrepaint,
        window: &mut Window,
        cx: &mut App,
    ) {
        let focus_handle = self.editor.read(cx).focus_handle.clone();
        window.handle_input(&focus_handle, ElementInputHandler::new(bounds, self.editor.clone()), cx);
        for quad in prepaint.quads.drain(..) {
            window.paint_quad(quad);
        }
        let line_height = prepaint.line_height;
        self.editor.update(cx, |editor, cx| {
            for (row, origin) in &prepaint.rows {
                if let Some(layout) = editor.layouts.get(*row).and_then(|l| l.as_ref()) {
                    let _ = layout.wrapped.paint(*origin, line_height, TextAlign::Left, None, window, cx);
                }
            }
        });
        for (shaped, origin) in &prepaint.gutter {
            let _ = shaped.paint(*origin, line_height, TextAlign::Left, None, window, cx);
        }
        if let Some((shaped, origin)) = &prepaint.placeholder {
            let _ = shaped.paint(*origin, line_height, TextAlign::Left, None, window, cx);
        }
        if let Some(cursor) = prepaint.cursor.take() {
            window.paint_quad(cursor);
        }
    }
}

fn gutter_width(editor: &Editor) -> Pixels {
    if !editor.line_numbers {
        return px(0.);
    }
    let digits = editor.buffer.line_count().to_string().len().max(2) as f32;
    px(GUTTER_GAP) + editor.font_size * 0.86 * 0.62 * digits
}
