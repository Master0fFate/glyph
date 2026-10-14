use pulldown_cmark::{Alignment, CodeBlockKind, Event, Options, Parser, Tag, TagEnd};
use std::ops::Range;

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct SpanStyle {
    pub bold: bool,
    pub italic: bool,
    pub code: bool,
    pub strike: bool,
}

#[derive(Clone, Debug)]
pub struct Span {
    pub range: Range<usize>,
    pub style: SpanStyle,
    pub link: Option<String>,
}

#[derive(Clone, Debug, Default)]
pub struct RichText {
    pub text: String,
    pub spans: Vec<Span>,
}

impl RichText {
    fn push(&mut self, text: &str, style: SpanStyle, link: Option<&str>) {
        if text.is_empty() {
            return;
        }
        let start = self.text.len();
        self.text.push_str(text);
        let range = start..self.text.len();
        if let Some(last) = self.spans.last_mut()
            && last.range.end == start
            && last.style == style
            && last.link.as_deref() == link
        {
            last.range.end = range.end;
            return;
        }
        self.spans.push(Span { range, style, link: link.map(str::to_owned) });
    }

    pub fn is_empty(&self) -> bool {
        self.text.is_empty()
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Align {
    Left,
    Center,
    Right,
}

#[derive(Clone, Debug)]
pub struct ListItem {
    pub checked: Option<bool>,
    pub blocks: Vec<Block>,
}

#[derive(Clone, Debug)]
pub enum Block {
    Heading { level: u8, text: RichText, line: usize },
    Paragraph { text: RichText, line: usize },
    Code { lang: String, text: String, line: usize },
    Quote { blocks: Vec<Block>, line: usize },
    List { start: Option<u64>, items: Vec<ListItem>, line: usize },
    Table { header: Vec<RichText>, rows: Vec<Vec<RichText>>, aligns: Vec<Align>, line: usize },
    Rule { line: usize },
    Image { alt: String, src: String, line: usize },
}

impl Block {
    pub fn line(&self) -> usize {
        match self {
            Block::Heading { line, .. }
            | Block::Paragraph { line, .. }
            | Block::Code { line, .. }
            | Block::Quote { line, .. }
            | Block::List { line, .. }
            | Block::Table { line, .. }
            | Block::Rule { line }
            | Block::Image { line, .. } => *line,
        }
    }
}

#[derive(Clone, Debug)]
pub struct Heading {
    pub level: u8,
    pub text: String,
    pub line: usize,
}

#[derive(Clone, Debug, Default)]
pub struct Document {
    pub blocks: Vec<Block>,
    pub headings: Vec<Heading>,
}

enum Frame {
    Root(Vec<Block>),
    Quote(Vec<Block>, usize),
    List { start: Option<u64>, items: Vec<ListItem>, line: usize },
    Item(ListItem),
    Table { header: Vec<RichText>, rows: Vec<Vec<RichText>>, aligns: Vec<Align>, line: usize },
    Row(Vec<RichText>),
    Cell(RichText),
}

enum InlineKind {
    Paragraph,
    Heading(u8),
}

struct Builder {
    line_starts: Vec<usize>,
    frames: Vec<Frame>,
    inline: Option<(RichText, usize, InlineKind)>,
    styles: Vec<SpanStyle>,
    links: Vec<String>,
    code: Option<(String, String, usize)>,
    image: Option<(String, String, usize)>,
    headings: Vec<Heading>,
}

impl Builder {
    fn line_of(&self, offset: usize) -> usize {
        match self.line_starts.binary_search(&offset) {
            Ok(ix) => ix,
            Err(ix) => ix.saturating_sub(1),
        }
    }

    fn style(&self) -> SpanStyle {
        self.styles.last().copied().unwrap_or_default()
    }

    fn push_style(&mut self, apply: impl FnOnce(&mut SpanStyle)) {
        let mut style = self.style();
        apply(&mut style);
        self.styles.push(style);
    }

    fn push_block(&mut self, block: Block) {
        match self.frames.last_mut() {
            Some(Frame::Root(blocks)) | Some(Frame::Quote(blocks, _)) => blocks.push(block),
            Some(Frame::Item(item)) => item.blocks.push(block),
            Some(Frame::List { items, .. }) => items.push(ListItem { checked: None, blocks: vec![block] }),
            _ => {}
        }
    }

    fn text(&mut self, text: &str) {
        if let Some((alt, _, _)) = self.image.as_mut() {
            alt.push_str(text);
            return;
        }
        if let Some((code, _, _)) = self.code.as_mut() {
            code.push_str(text);
            return;
        }
        let style = self.style();
        let link = self.links.last().cloned();
        if let Some(Frame::Cell(cell)) = self.frames.last_mut() {
            cell.push(text, style, link.as_deref());
            return;
        }
        let (rich, _, _) = self.inline.get_or_insert_with(|| (RichText::default(), 0, InlineKind::Paragraph));
        rich.push(text, style, link.as_deref());
    }

    fn flush_inline(&mut self) {
        let Some((text, line, kind)) = self.inline.take() else {
            return;
        };
        if text.is_empty() {
            return;
        }
        let block = match kind {
            InlineKind::Paragraph => Block::Paragraph { text, line },
            InlineKind::Heading(level) => {
                self.headings.push(Heading { level, text: text.text.clone(), line });
                Block::Heading { level, text, line }
            }
        };
        self.push_block(block);
    }

    fn start(&mut self, tag: Tag, offset: usize) {
        let line = self.line_of(offset);
        match tag {
            Tag::Paragraph => {
                self.flush_inline();
                self.inline = Some((RichText::default(), line, InlineKind::Paragraph));
            }
            Tag::Heading { level, .. } => {
                self.flush_inline();
                self.inline = Some((RichText::default(), line, InlineKind::Heading(level as u8)));
            }
            Tag::BlockQuote(_) => {
                self.flush_inline();
                self.frames.push(Frame::Quote(Vec::new(), line));
            }
            Tag::CodeBlock(kind) => {
                self.flush_inline();
                let lang = match kind {
                    CodeBlockKind::Fenced(info) => info.split_whitespace().next().unwrap_or("").to_owned(),
                    CodeBlockKind::Indented => String::new(),
                };
                self.code = Some((String::new(), lang, line));
            }
            Tag::List(start) => {
                self.flush_inline();
                self.frames.push(Frame::List { start, items: Vec::new(), line });
            }
            Tag::Item => {
                self.flush_inline();
                self.frames.push(Frame::Item(ListItem { checked: None, blocks: Vec::new() }));
            }
            Tag::Emphasis => self.push_style(|s| s.italic = true),
            Tag::Strong => self.push_style(|s| s.bold = true),
            Tag::Strikethrough => self.push_style(|s| s.strike = true),
            Tag::Link { dest_url, .. } => self.links.push(dest_url.to_string()),
            Tag::Image { dest_url, .. } => self.image = Some((String::new(), dest_url.to_string(), line)),
            Tag::Table(aligns) => {
                self.flush_inline();
                let aligns = aligns
                    .iter()
                    .map(|a| match a {
                        Alignment::Center => Align::Center,
                        Alignment::Right => Align::Right,
                        _ => Align::Left,
                    })
                    .collect();
                self.frames.push(Frame::Table { header: Vec::new(), rows: Vec::new(), aligns, line });
            }
            Tag::TableHead | Tag::TableRow => self.frames.push(Frame::Row(Vec::new())),
            Tag::TableCell => self.frames.push(Frame::Cell(RichText::default())),
            _ => {}
        }
    }

    fn end(&mut self, tag: TagEnd) {
        match tag {
            TagEnd::Paragraph | TagEnd::Heading(_) => self.flush_inline(),
            TagEnd::BlockQuote(_) => {
                self.flush_inline();
                if let Some(Frame::Quote(blocks, line)) = self.frames.pop() {
                    self.push_block(Block::Quote { blocks, line });
                }
            }
            TagEnd::List(_) => {
                self.flush_inline();
                if let Some(Frame::List { start, items, line }) = self.frames.pop() {
                    self.push_block(Block::List { start, items, line });
                }
            }
            TagEnd::Item => {
                self.flush_inline();
                if let Some(Frame::Item(item)) = self.frames.pop()
                    && let Some(Frame::List { items, .. }) = self.frames.last_mut()
                {
                    items.push(item);
                }
            }
            TagEnd::CodeBlock => {
                if let Some((text, lang, line)) = self.code.take() {
                    let text = text.strip_suffix('\n').map(str::to_owned).unwrap_or(text);
                    self.push_block(Block::Code { lang, text, line });
                }
            }
            TagEnd::Emphasis | TagEnd::Strong | TagEnd::Strikethrough => {
                self.styles.pop();
            }
            TagEnd::Link => {
                self.links.pop();
            }
            TagEnd::Image => {
                if let Some((alt, src, line)) = self.image.take() {
                    self.flush_inline();
                    self.push_block(Block::Image { alt, src, line });
                }
            }
            TagEnd::TableHead => {
                if let Some(Frame::Row(cells)) = self.frames.pop()
                    && let Some(Frame::Table { header, .. }) = self.frames.last_mut()
                {
                    *header = cells;
                }
            }
            TagEnd::TableRow => {
                if let Some(Frame::Row(cells)) = self.frames.pop()
                    && let Some(Frame::Table { rows, .. }) = self.frames.last_mut()
                {
                    rows.push(cells);
                }
            }
            TagEnd::TableCell => {
                if let Some(Frame::Cell(cell)) = self.frames.pop()
                    && let Some(Frame::Row(cells)) = self.frames.last_mut()
                {
                    cells.push(cell);
                }
            }
            TagEnd::Table => {
                if let Some(Frame::Table { header, rows, aligns, line }) = self.frames.pop() {
                    self.push_block(Block::Table { header, rows, aligns, line });
                }
            }
            _ => {}
        }
    }
}

const OPTIONS: Options = Options::ENABLE_TABLES.union(Options::ENABLE_STRIKETHROUGH).union(Options::ENABLE_TASKLISTS);

pub fn parse(source: &str) -> Document {
    let mut line_starts = vec![0];
    line_starts.extend(source.match_indices('\n').map(|(i, _)| i + 1));
    let mut builder = Builder {
        line_starts,
        frames: vec![Frame::Root(Vec::new())],
        inline: None,
        styles: Vec::new(),
        links: Vec::new(),
        code: None,
        image: None,
        headings: Vec::new(),
    };
    for (event, range) in Parser::new_ext(source, OPTIONS).into_offset_iter() {
        match event {
            Event::Start(tag) => builder.start(tag, range.start),
            Event::End(tag) => builder.end(tag),
            Event::Text(text) => builder.text(&text),
            Event::Code(code) => {
                builder.push_style(|s| s.code = true);
                builder.text(&code);
                builder.styles.pop();
            }
            Event::SoftBreak => builder.text(" "),
            Event::HardBreak => builder.text("\n"),
            Event::Rule => {
                builder.flush_inline();
                let line = builder.line_of(range.start);
                builder.push_block(Block::Rule { line });
            }
            Event::TaskListMarker(checked) => {
                if let Some(Frame::Item(item)) = builder.frames.last_mut() {
                    item.checked = Some(checked);
                }
            }
            _ => {}
        }
    }
    builder.flush_inline();
    let blocks = match builder.frames.into_iter().next() {
        Some(Frame::Root(blocks)) => blocks,
        _ => Vec::new(),
    };
    Document { blocks, headings: builder.headings }
}

pub fn to_html(source: &str) -> String {
    let mut html = String::with_capacity(source.len() * 2);
    pulldown_cmark::html::push_html(&mut html, Parser::new_ext(source, OPTIONS));
    html
}
