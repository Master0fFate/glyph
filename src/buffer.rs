use std::ops::Range;

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug, Default, Hash)]
pub struct Position {
    pub row: usize,
    pub col: usize,
}

impl Position {
    pub fn new(row: usize, col: usize) -> Self {
        Self { row, col }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct Selection {
    pub anchor: Position,
    pub head: Position,
}

impl Selection {
    pub fn caret(position: Position) -> Self {
        Self { anchor: position, head: position }
    }

    pub fn range(&self) -> Range<Position> {
        if self.anchor <= self.head {
            self.anchor..self.head
        } else {
            self.head..self.anchor
        }
    }

    pub fn is_empty(&self) -> bool {
        self.anchor == self.head
    }

    pub fn reversed(&self) -> bool {
        self.head < self.anchor
    }
}

struct Edit {
    start: Position,
    old: String,
    new: String,
    before: Selection,
    after: Selection,
    coalesce: bool,
}

pub struct Buffer {
    lines: Vec<String>,
    undo: Vec<Edit>,
    redo: Vec<Edit>,
    version: u64,
    saved_version: u64,
}

const UNDO_LIMIT: usize = 500;

impl Buffer {
    pub fn from_text(text: &str) -> Self {
        Self {
            lines: text.split('\n').map(str::to_owned).collect(),
            undo: Vec::new(),
            redo: Vec::new(),
            version: 0,
            saved_version: 0,
        }
    }

    pub fn text(&self) -> String {
        self.lines.join("\n")
    }

    pub fn lines(&self) -> &[String] {
        &self.lines
    }

    pub fn line(&self, row: usize) -> &str {
        &self.lines[row]
    }

    pub fn line_count(&self) -> usize {
        self.lines.len()
    }

    pub fn end(&self) -> Position {
        let row = self.lines.len() - 1;
        Position::new(row, self.lines[row].len())
    }

    pub fn version(&self) -> u64 {
        self.version
    }

    pub fn is_dirty(&self) -> bool {
        self.version != self.saved_version
    }

    pub fn mark_saved(&mut self) {
        self.saved_version = self.version;
    }

    pub fn clamp(&self, position: Position) -> Position {
        let row = position.row.min(self.lines.len() - 1);
        let line = &self.lines[row];
        let mut col = position.col.min(line.len());
        while col > 0 && !line.is_char_boundary(col) {
            col -= 1;
        }
        Position::new(row, col)
    }

    pub fn slice(&self, range: Range<Position>) -> String {
        if range.start.row == range.end.row {
            return self.lines[range.start.row][range.start.col..range.end.col].to_owned();
        }
        let mut out = String::new();
        out.push_str(&self.lines[range.start.row][range.start.col..]);
        for row in range.start.row + 1..range.end.row {
            out.push('\n');
            out.push_str(&self.lines[row]);
        }
        out.push('\n');
        out.push_str(&self.lines[range.end.row][..range.end.col]);
        out
    }

    pub fn end_of(start: Position, text: &str) -> Position {
        match text.rsplit_once('\n') {
            Some((head, tail)) => Position::new(start.row + head.matches('\n').count() + 1, tail.len()),
            None => Position::new(start.row, start.col + text.len()),
        }
    }


    pub fn replace(&mut self, range: Range<Position>, new: &str, before: Selection, after: Selection, coalesce: bool) -> Position {
        let old = self.slice(range.clone());
        if old == new {
            return range.end;
        }
        let end = self.splice(range.start, range.end, new);
        self.redo.clear();
        self.version += 1;
        if coalesce
            && let Some(last) = self.undo.last_mut()
            && last.coalesce
            && merge(last, range.start, &old, new, after)
        {
            return end;
        }
        self.undo.push(Edit { start: range.start, old, new: new.to_owned(), before, after, coalesce });
        if self.undo.len() > UNDO_LIMIT {
            self.undo.remove(0);
        }
        end
    }

    fn splice(&mut self, start: Position, end: Position, new: &str) -> Position {
        let head = &self.lines[start.row][..start.col];
        let tail = &self.lines[end.row][end.col..];
        let mut combined = String::with_capacity(head.len() + new.len() + tail.len());
        combined.push_str(head);
        combined.push_str(new);
        combined.push_str(tail);
        let replacement: Vec<String> = combined.split('\n').map(str::to_owned).collect();
        self.lines.splice(start.row..=end.row, replacement);
        Self::end_of(start, new)
    }

    pub fn undo(&mut self) -> Option<Selection> {
        let edit = self.undo.pop()?;
        let end = Self::end_of(edit.start, &edit.new);
        self.splice(edit.start, end, &edit.old);
        self.version += 1;
        let before = edit.before;
        self.redo.push(edit);
        Some(before)
    }

    pub fn redo(&mut self) -> Option<Selection> {
        let edit = self.redo.pop()?;
        let end = Self::end_of(edit.start, &edit.old);
        self.splice(edit.start, end, &edit.new);
        self.version += 1;
        let after = edit.after;
        self.undo.push(edit);
        Some(after)
    }

    pub fn break_undo_group(&mut self) {
        if let Some(last) = self.undo.last_mut() {
            last.coalesce = false;
        }
    }
}

fn merge(last: &mut Edit, start: Position, old: &str, new: &str, after: Selection) -> bool {
    let typing = old.is_empty() && !new.contains('\n') && last.old.is_empty() && Buffer::end_of(last.start, &last.new) == start;
    if typing {
        last.new.push_str(new);
        last.after = after;
        return true;
    }
    let deleting = new.is_empty() && !old.contains('\n') && last.new.is_empty() && Buffer::end_of(start, old) == last.start;
    if deleting {
        last.old.insert_str(0, old);
        last.start = start;
        last.after = after;
        return true;
    }
    false
}
