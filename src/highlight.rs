use std::ops::Range;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Token {
    Heading,
    Marker,
    Code,
    Bold,
    Italic,
    Link,
    Quote,
    Fence,
    String,
    Keyword,
    Comment,
    Number,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct FenceState(pub i8);

impl FenceState {
    pub fn inside(self) -> bool {
        self.0 != 0
    }
}

pub struct FenceMarker {
    pub marker: char,
    pub count: usize,
    pub rest_blank: bool,
}

pub fn fence_marker(line: &str) -> Option<FenceMarker> {
    let trimmed = line.trim_start_matches(' ');
    if line.len() - trimmed.len() > 3 {
        return None;
    }
    let first = trimmed.chars().next()?;
    if first != '`' && first != '~' {
        return None;
    }
    let count = trimmed.chars().take_while(|&c| c == first).count();
    if count < 3 {
        return None;
    }
    let rest = &trimmed[count..];
    if first == '`' && rest.contains('`') {
        return None;
    }
    Some(FenceMarker { marker: first, count, rest_blank: rest.trim().is_empty() })
}

pub fn next_fence_state(state: FenceState, line: &str) -> (FenceState, bool) {
    let Some(fence) = fence_marker(line) else {
        return (state, false);
    };
    let value = if fence.marker == '`' { fence.count as i8 } else { -(fence.count as i8) };
    if !state.inside() {
        return (FenceState(value), true);
    }
    let same_kind = (state.0 > 0) == (value > 0);
    if same_kind && value.abs() >= state.0.abs() && fence.rest_blank {
        return (FenceState(0), true);
    }
    (state, false)
}

const KEYWORDS: &[&str] = &[
    "fn", "let", "mut", "pub", "use", "mod", "impl", "struct", "enum", "trait", "match", "if", "else", "for", "while",
    "loop", "return", "self", "Self", "true", "false", "const", "static", "where", "async", "await", "move", "ref", "in",
    "def", "class", "import", "from", "as", "None", "True", "False", "and", "or", "not", "is", "with", "try", "except",
    "finally", "raise", "lambda", "yield", "pass", "break", "continue", "elif", "function", "var", "export", "default",
    "new", "this", "null", "undefined", "interface", "type", "extends", "implements", "switch", "case", "do", "throw",
    "catch", "void", "int", "float", "string", "bool", "char", "double", "long", "public", "private", "protected",
    "package", "go", "chan", "defer", "select", "map", "range", "nil", "end", "then", "local", "require",
];

pub fn code_tokens(line: &str, out: &mut Vec<(Range<usize>, Token)>) {
    let bytes = line.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        let c = bytes[i];
        if c == b'"' || c == b'\'' {
            let start = i;
            i += 1;
            while i < bytes.len() && bytes[i] != c {
                if bytes[i] == b'\\' {
                    i += 1;
                }
                i += 1;
            }
            i = (i + 1).min(bytes.len());
            out.push((start..i, Token::String));
        } else if c == b'#' || (c == b'/' && bytes.get(i + 1) == Some(&b'/')) || (c == b'-' && bytes.get(i + 1) == Some(&b'-')) {
            out.push((i..bytes.len(), Token::Comment));
            return;
        } else if c.is_ascii_digit() && (i == 0 || !is_word(bytes[i - 1])) {
            let start = i;
            while i < bytes.len() && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'.' || bytes[i] == b'_') {
                i += 1;
            }
            out.push((start..i, Token::Number));
        } else if is_word(c) {
            let start = i;
            while i < bytes.len() && is_word(bytes[i]) {
                i += 1;
            }
            if KEYWORDS.contains(&&line[start..i]) {
                out.push((start..i, Token::Keyword));
            }
        } else {
            i += 1;
        }
    }
}

fn is_word(c: u8) -> bool {
    c.is_ascii_alphanumeric() || c == b'_'
}

pub fn heading_level(line: &str) -> Option<usize> {
    let trimmed = line.trim_start_matches(' ');
    if line.len() - trimmed.len() > 3 {
        return None;
    }
    let level = trimmed.chars().take_while(|&c| c == '#').count();
    if (1..=6).contains(&level) && trimmed[level..].starts_with([' ', '\t']) || (level >= 1 && level <= 6 && trimmed.len() == level) {
        Some(level)
    } else {
        None
    }
}

pub struct ListMarker {
    pub indent: usize,
    pub marker_end: usize,
    pub number: Option<u64>,
    pub delimiter: char,
    pub task: Option<bool>,
    pub body_start: usize,
}

pub fn list_marker(line: &str) -> Option<ListMarker> {
    let indent = line.len() - line.trim_start().len();
    let rest = &line[indent..];
    let bytes = rest.as_bytes();
    let (number, marker_len, delimiter) = if matches!(bytes.first(), Some(b'-' | b'+' | b'*')) {
        (None, 1, bytes[0] as char)
    } else {
        let digits = bytes.iter().take_while(|b| b.is_ascii_digit()).count();
        if digits == 0 || digits > 9 || !matches!(bytes.get(digits), Some(b'.' | b')')) {
            return None;
        }
        (rest[..digits].parse().ok(), digits + 1, bytes[digits] as char)
    };
    let after = &rest[marker_len..];
    let spaces = after.len() - after.trim_start_matches(' ').len();
    if spaces == 0 && !after.is_empty() {
        return None;
    }
    let mut body_start = indent + marker_len + spaces;
    let mut task = None;
    let body = &line[body_start..];
    if let Some(check) = body.strip_prefix("[ ] ").map(|_| false).or_else(|| body.strip_prefix("[x] ").or_else(|| body.strip_prefix("[X] ")).map(|_| true)) {
        task = Some(check);
        body_start += 4;
    }
    Some(ListMarker { indent, marker_end: indent + marker_len, number, delimiter, task, body_start })
}

pub fn line_tokens(line: &str, state: FenceState, out: &mut Vec<(Range<usize>, Token)>) {
    out.clear();
    if line.len() > 6000 {
        return;
    }
    let (_, is_marker) = next_fence_state(state, line);
    if is_marker {
        out.push((0..line.len(), Token::Fence));
        return;
    }
    if state.inside() {
        out.push((0..line.len(), Token::Code));
        code_tokens(line, out);
        return;
    }
    if heading_level(line).is_some() {
        out.push((0..line.len(), Token::Heading));
        inline_tokens(line, 0, out);
        return;
    }
    let trimmed = line.trim_start();
    if trimmed.starts_with('>') {
        out.push((0..line.len(), Token::Quote));
        return;
    }
    let mut start = 0;
    if let Some(marker) = list_marker(line) {
        out.push((marker.indent..marker.body_start, Token::Marker));
        start = marker.body_start;
    }
    inline_tokens(line, start, out);
}

fn inline_tokens(line: &str, from: usize, out: &mut Vec<(Range<usize>, Token)>) {
    let bytes = line.as_bytes();
    let mut i = from;
    while i < bytes.len() {
        match bytes[i] {
            b'`' => {
                let ticks = bytes[i..].iter().take_while(|&&b| b == b'`').count();
                if let Some(close) = find_closing(line, i + ticks, &"`".repeat(ticks)) {
                    out.push((i..close + ticks, Token::Code));
                    i = close + ticks;
                    continue;
                }
                i += ticks;
            }
            b'*' | b'_' => {
                let c = bytes[i];
                let double = bytes.get(i + 1) == Some(&c);
                let delim = if double { 2 } else { 1 };
                let token = if double { Token::Bold } else { Token::Italic };
                let marker = std::str::from_utf8(&bytes[i..i + delim]).unwrap_or("*");
                if bytes.get(i + delim).is_some_and(|b| !b.is_ascii_whitespace())
                    && let Some(close) = find_closing(line, i + delim, marker)
                {
                    out.push((i..close + delim, token));
                    i = close + delim;
                    continue;
                }
                i += delim;
            }
            b'[' | b'!' => {
                let start = i;
                let bracket = if bytes[i] == b'!' { i + 1 } else { i };
                if bytes.get(bracket) == Some(&b'[')
                    && let Some(close) = line[bracket + 1..].find(']').map(|p| bracket + 1 + p)
                    && bytes.get(close + 1) == Some(&b'(')
                    && let Some(end) = line[close + 2..].find(')').map(|p| close + 2 + p)
                {
                    out.push((start..end + 1, Token::Link));
                    i = end + 1;
                    continue;
                }
                i += 1;
            }
            _ => i += 1,
        }
    }
}

fn find_closing(line: &str, from: usize, marker: &str) -> Option<usize> {
    let mut search = from;
    while let Some(found) = line[search..].find(marker) {
        let at = search + found;
        if at > from && !line.as_bytes()[at - 1].is_ascii_whitespace() {
            return Some(at);
        }
        search = at + marker.len();
    }
    None
}
