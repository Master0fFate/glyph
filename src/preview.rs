use crate::highlight::{self, Token};
use crate::markdown::{Align, Block, Document, ListItem, RichText};
use crate::theme::Theme;
use gpui::{
    AnyElement, App, ClipboardItem, ElementId, FontStyle, FontWeight, InteractiveText, MouseButton, Pixels, SharedString,
    StrikethroughStyle, StyledText, TextRun, UnderlineStyle, Window, div, font, img, prelude::*, px, svg,
};
use std::cell::Cell;
use std::path::{Path, PathBuf};
use std::rc::Rc;

pub type LinkHandler = Rc<dyn Fn(&str, &mut Window, &mut App)>;
pub type JumpHandler = Rc<dyn Fn(usize, &mut Window, &mut App)>;

pub struct PreviewContext {
    pub theme: Theme,
    pub base_dir: Option<PathBuf>,
    pub on_link: LinkHandler,
    pub on_jump: JumpHandler,
    pub tab_id: usize,
    counter: Cell<usize>,
}

const BASE_SIZE: f32 = 15.5;
const MAX_IMAGE_BYTES: u64 = 2 * 1024 * 1024;

impl PreviewContext {
    pub fn new(theme: Theme, base_dir: Option<PathBuf>, tab_id: usize, on_link: LinkHandler, on_jump: JumpHandler) -> Self {
        Self { theme, base_dir, on_link, on_jump, tab_id, counter: Cell::new(0) }
    }

    fn next_id(&self, kind: &'static str) -> ElementId {
        let ix = self.counter.get();
        self.counter.set(ix + 1);
        ElementId::NamedInteger(SharedString::from(kind), (self.tab_id as u64) << 32 | ix as u64)
    }

    pub fn render(&self, document: &Document) -> Vec<AnyElement> {
        document
            .blocks
            .iter()
            .map(|block| {
                let line = block.line();
                let on_jump = self.on_jump.clone();
                div()
                    .id(self.next_id("block"))
                    .w_full()
                    .on_mouse_down(MouseButton::Left, move |event, window, cx| {
                        if event.click_count == 2 {
                            on_jump(line, window, cx);
                        }
                    })
                    .child(self.block(block, false))
                    .into_any_element()
            })
            .collect()
    }

    fn block(&self, block: &Block, tight: bool) -> AnyElement {
        let theme = &self.theme;
        let gap = if tight { px(6.) } else { px(14.) };
        match block {
            Block::Heading { level, text, .. } => {
                let (size, weight, top) = match level {
                    1 => (30., FontWeight::BOLD, 22.),
                    2 => (23., FontWeight::SEMIBOLD, 26.),
                    3 => (18.5, FontWeight::SEMIBOLD, 20.),
                    4 => (16., FontWeight::SEMIBOLD, 16.),
                    _ => (14., FontWeight::SEMIBOLD, 14.),
                };
                let color = if *level >= 5 { theme.muted } else { theme.text };
                let mut heading = div().pt(px(top)).pb(px(8.)).line_height(px(size * 1.3)).text_size(px(size)).child(self.rich(text, px(size), color, weight));
                if *level <= 2 {
                    heading = heading.border_b_1().border_color(theme.border).mb(px(10.));
                }
                heading.into_any_element()
            }
            Block::Paragraph { text, .. } => div().pb(gap).child(self.rich(text, px(BASE_SIZE), theme.text, FontWeight::NORMAL)).into_any_element(),
            Block::Code { lang, text, .. } => self.code_block(lang, text).pb(gap).into_any_element(),
            Block::Quote { blocks, .. } => div()
                .pb(gap)
                .child(
                    div()
                        .border_l_2()
                        .border_color(theme.accent)
                        .pl_4()
                        .py_1()
                        .text_color(theme.muted)
                        .children(blocks.iter().map(|b| self.block(b, true))),
                )
                .into_any_element(),
            Block::List { start, items, .. } => div()
                .pb(gap)
                .flex()
                .flex_col()
                .gap(px(4.))
                .children(items.iter().enumerate().map(|(ix, item)| self.list_item(item, start.map(|s| s + ix as u64))))
                .into_any_element(),
            Block::Table { header, rows, aligns, .. } => self.table(header, rows, aligns).pb(gap).into_any_element(),
            Block::Rule { .. } => div().my_4().h(px(1.)).w_full().bg(theme.border_strong).into_any_element(),
            Block::Image { alt, src, .. } => self.image(alt, src).pb(gap).into_any_element(),
        }
    }

    fn list_item(&self, item: &ListItem, number: Option<u64>) -> AnyElement {
        let theme = &self.theme;
        let marker: AnyElement = match (item.checked, number) {
            (Some(checked), _) => div()
                .size(px(16.))
                .mt(px(5.))
                .rounded(px(4.))
                .border_1()
                .border_color(if checked { theme.accent } else { theme.border_strong })
                .bg(if checked { theme.accent } else { theme.card })
                .flex()
                .items_center()
                .justify_center()
                .when(checked, |this| this.child(svg().path("icons/check.svg").size(px(12.)).text_color(theme.on_accent)))
                .into_any_element(),
            (None, Some(n)) => div().min_w(px(22.)).text_color(theme.muted).text_right().child(format!("{n}.")).into_any_element(),
            (None, None) => div().size(px(6.)).mt(px(10.)).rounded_full().bg(theme.faint).into_any_element(),
        };
        div()
            .flex()
            .gap(px(10.))
            .items_start()
            .child(div().flex_shrink_0().w(px(22.)).flex().justify_center().child(marker))
            .child(div().flex_1().min_w_0().flex().flex_col().children(item.blocks.iter().map(|b| self.block(b, true))))
            .into_any_element()
    }

    fn table(&self, header: &[RichText], rows: &[Vec<RichText>], aligns: &[Align]) -> gpui::Div {
        let theme = &self.theme;
        let columns = header.len().max(rows.iter().map(|r| r.len()).max().unwrap_or(0)).max(1);
        let align = |ix: usize| aligns.get(ix).copied().unwrap_or(Align::Left);
        let row = |cells: &[RichText], head: bool| {
            let mut row = div().flex().w_full().border_b_1().border_color(theme.border);
            if head {
                row = row.bg(theme.raised);
            }
            row.children((0..columns).map(|ix| {
                let mut cell = div().flex_1().min_w_0().px_3().py_2().text_size(px(14.));
                cell = match align(ix) {
                    Align::Left => cell.text_left(),
                    Align::Center => cell.text_center(),
                    Align::Right => cell.text_right(),
                };
                let weight = if head { FontWeight::SEMIBOLD } else { FontWeight::NORMAL };
                match cells.get(ix) {
                    Some(text) => cell.child(self.rich(text, px(14.), theme.text, weight)),
                    None => cell,
                }
            }))
        };
        div()
            .w_full()
            .rounded(px(10.))
            .border_1()
            .border_color(theme.border)
            .overflow_hidden()
            .child(row(header, true))
            .children(rows.iter().map(|cells| row(cells, false)))
    }

    fn image(&self, alt: &str, src: &str) -> gpui::Div {
        let theme = &self.theme;
        let notice = |message: String| {
            div()
                .flex()
                .items_center()
                .gap_2()
                .px_3()
                .py_2()
                .rounded(px(8.))
                .border_1()
                .border_color(theme.border)
                .bg(theme.card)
                .text_size(px(13.))
                .text_color(theme.muted)
                .child(svg().path("icons/image.svg").size(px(14.)).text_color(theme.faint))
                .child(message)
        };
        match self.resolve_image(src) {
            Ok(path) => div().flex().flex_col().gap_2().child(img(path).max_w_full().rounded(px(10.))).when(!alt.is_empty(), |this| {
                this.child(div().text_size(px(13.)).text_color(theme.faint).child(alt.to_owned()))
            }),
            Err(reason) => notice(format!("{reason}: {src}")),
        }
    }

    fn resolve_image(&self, src: &str) -> Result<PathBuf, &'static str> {
        if src.starts_with("http://") || src.starts_with("https://") || src.starts_with("data:") {
            return Err("Remote images are disabled");
        }
        let base = self.base_dir.as_deref().ok_or("Save the document to preview local images")?;
        let candidate = Path::new(src);
        if candidate.is_absolute() {
            return Err("Only images under the document folder are shown");
        }
        let joined = base.join(candidate);
        let canonical = crate::document::canonical(&joined);
        if !canonical.starts_with(base) {
            return Err("Only images under the document folder are shown");
        }
        let extension = canonical.extension().and_then(|e| e.to_str()).map(|e| e.to_ascii_lowercase());
        if !matches!(extension.as_deref(), Some("png" | "jpg" | "jpeg" | "gif" | "webp" | "bmp")) {
            return Err("Only raster images are shown");
        }
        let size = std::fs::metadata(&canonical).map_err(|_| "Image not found")?.len();
        if size > MAX_IMAGE_BYTES {
            return Err("Image exceeds 2 MiB");
        }
        Ok(canonical)
    }

    fn code_block(&self, lang: &str, text: &str) -> gpui::Div {
        let theme = &self.theme;
        let mermaid = lang.eq_ignore_ascii_case("mermaid");
        let label = if lang.is_empty() { "code".to_owned() } else { lang.to_lowercase() };
        let copy_text = text.to_owned();
        let mono = font(theme.mono.clone());
        let mut tokens = Vec::new();
        let body = div().px_4().py_3().font_family(theme.mono.clone()).text_size(px(13.)).line_height(px(21.)).flex().flex_col().children(
            text.lines().map(|line| {
                tokens.clear();
                highlight::code_tokens(line, &mut tokens);
                let runs = runs_for_tokens(line, &tokens, &mono, theme);
                let text: SharedString = if line.is_empty() { " ".into() } else { line.to_owned().into() };
                let runs = if line.is_empty() { vec![TextRun { len: 1, ..runs[0].clone() }] } else { runs };
                div().w_full().whitespace_nowrap().child(StyledText::new(text).with_runs(runs))
            }),
        );
        div()
            .w_full()
            .rounded(px(10.))
            .border_1()
            .border_color(theme.border)
            .bg(theme.card)
            .overflow_hidden()
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .px_3()
                    .py_1p5()
                    .border_b_1()
                    .border_color(theme.border)
                    .bg(theme.shell)
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_2()
                            .text_size(px(11.))
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(theme.faint)
                            .child(label.to_uppercase())
                            .when(mermaid, |this| this.child(div().font_weight(FontWeight::NORMAL).child("· diagram source"))),
                    )
                    .child(
                        div()
                            .id(self.next_id("copy"))
                            .flex()
                            .items_center()
                            .gap_1()
                            .px_2()
                            .py_1()
                            .rounded(px(6.))
                            .text_size(px(11.))
                            .text_color(theme.muted)
                            .cursor_pointer()
                            .hover(|this| this.bg(theme.hover).text_color(theme.text))
                            .on_click(move |_, _, cx| cx.write_to_clipboard(ClipboardItem::new_string(copy_text.clone())))
                            .child(svg().path("icons/copy.svg").size(px(12.)).text_color(theme.muted))
                            .child("Copy"),
                    ),
            )
            .when(mermaid, |this| {
                this.child(
                    div()
                        .flex()
                        .items_center()
                        .gap_2()
                        .px_3()
                        .py_2()
                        .text_size(px(12.))
                        .text_color(theme.muted)
                        .border_b_1()
                        .border_color(theme.border)
                        .child(svg().path("icons/info.svg").size(px(13.)).text_color(theme.accent))
                        .child("Mermaid diagrams render in the HTML export; the native preview shows the source."),
                )
            })
            .child(div().id(self.next_id("code")).overflow_x_scroll().child(body))
    }

    fn rich(&self, text: &RichText, size: Pixels, color: gpui::Hsla, weight: FontWeight) -> AnyElement {
        let theme = &self.theme;
        let sans = font(theme.sans.clone());
        let mono = font(theme.mono.clone());
        let mut runs = Vec::with_capacity(text.spans.len());
        let mut links = Vec::new();
        let mut ranges = Vec::new();
        for span in &text.spans {
            let mut run = TextRun {
                len: span.range.len(),
                font: if span.style.code { mono.clone() } else { sans.clone() },
                color,
                background_color: None,
                underline: None,
                strikethrough: None,
            };
            run.font.weight = if span.style.bold { FontWeight::BOLD } else { weight };
            if span.style.italic {
                run.font.style = FontStyle::Italic;
            }
            if span.style.code {
                run.color = theme.syntax.code;
                run.background_color = Some(theme.raised);
            }
            if span.style.strike {
                run.strikethrough = Some(StrikethroughStyle { thickness: px(1.), color: Some(theme.muted) });
                run.color = theme.muted;
            }
            if let Some(link) = &span.link {
                run.color = theme.syntax.link;
                run.underline = Some(UnderlineStyle { color: Some(theme.syntax.link.alpha(0.5)), thickness: px(1.), wavy: false });
                links.push(link.clone());
                ranges.push(span.range.clone());
            }
            runs.push(run);
        }
        let covered: usize = runs.iter().map(|r| r.len).sum();
        if covered < text.text.len() {
            runs.push(TextRun { len: text.text.len() - covered, font: sans.clone(), color, background_color: None, underline: None, strikethrough: None });
        }
        let styled = StyledText::new(SharedString::from(text.text.clone())).with_runs(runs);
        if links.is_empty() {
            return div().text_size(size).child(styled).into_any_element();
        }
        let on_link = self.on_link.clone();
        div()
            .text_size(size)
            .child(InteractiveText::new(self.next_id("rich"), styled).on_click(ranges, move |ix, window, cx| on_link(&links[ix], window, cx)))
            .into_any_element()
    }
}

fn runs_for_tokens(line: &str, tokens: &[(std::ops::Range<usize>, Token)], mono: &gpui::Font, theme: &Theme) -> Vec<TextRun> {
    let plain = TextRun { len: line.len(), font: mono.clone(), color: theme.text, background_color: None, underline: None, strikethrough: None };
    if tokens.is_empty() || line.is_empty() {
        return vec![plain];
    }
    let mut boundaries: Vec<usize> = vec![0, line.len()];
    for (range, _) in tokens {
        boundaries.push(range.start.min(line.len()));
        boundaries.push(range.end.min(line.len()));
    }
    boundaries.sort_unstable();
    boundaries.dedup();
    boundaries
        .windows(2)
        .filter(|pair| pair[0] < pair[1])
        .map(|pair| {
            let mut run = TextRun { len: pair[1] - pair[0], ..plain.clone() };
            for (range, token) in tokens {
                if range.start <= pair[0] && pair[1] <= range.end {
                    run.color = match token {
                        Token::String => theme.syntax.string,
                        Token::Keyword => theme.syntax.keyword,
                        Token::Comment => theme.syntax.comment,
                        Token::Number => theme.syntax.number,
                        _ => theme.text,
                    };
                }
            }
            run
        })
        .collect()
}
