use pulldown_cmark::{CodeBlockKind, Event, HeadingLevel, Options, Parser, Tag, TagEnd};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};

use super::theme::Theme;

pub fn render(content: &str, theme: &Theme) -> Document {
    let mut renderer = Renderer::new(theme);

    for event in Parser::new_ext(content, Options::ENABLE_MATH) {
        renderer.event(event);
    }

    renderer.finish()
}

#[derive(Debug, Clone, Default)]
pub struct Document {
    pub blocks: Vec<Block>,
}

#[derive(Debug, Clone)]
pub enum Block {
    Text(Vec<Line<'static>>),
    Image(ImageRef),
    Diagram(DiagramRef),
}

#[derive(Debug, Clone)]
pub struct ImageRef {
    pub alt: String,
    pub dest: String,
}

#[derive(Debug, Clone)]
pub struct DiagramRef {
    pub language: String,
    pub source: String,
}

enum ListKind {
    Bullet,
    Ordered(u64),
}

struct Renderer<'a> {
    theme: &'a Theme,
    blocks: Vec<Block>,
    lines: Vec<Line<'static>>,
    current: Vec<Span<'static>>,
    styles: Vec<Style>,
    pending_prefix: Option<String>,
    item_prefix: Option<String>,
    code_block: bool,
    diagram_language: Option<String>,
    diagram_source: String,
    quote_depth: usize,
    lists: Vec<ListKind>,
    image_label: Option<String>,
    image_dest: Option<String>,
}

impl<'a> Renderer<'a> {
    fn new(theme: &'a Theme) -> Self {
        Self {
            theme,
            blocks: Vec::new(),
            lines: Vec::new(),
            current: Vec::new(),
            styles: vec![Style::default().fg(theme.text)],
            pending_prefix: None,
            item_prefix: None,
            code_block: false,
            diagram_language: None,
            diagram_source: String::new(),
            quote_depth: 0,
            lists: Vec::new(),
            image_label: None,
            image_dest: None,
        }
    }

    fn event(&mut self, event: Event<'_>) {
        match event {
            Event::Start(tag) => self.start(tag),
            Event::End(tag) => self.end(tag),
            Event::Text(text) => {
                if self.diagram_language.is_some() {
                    self.diagram_source.push_str(&text);
                } else if let Some(label) = self.image_label.as_mut() {
                    label.push_str(&text);
                } else if self.code_block {
                    self.code_text(&text);
                } else {
                    self.text(&text);
                }
            }
            Event::Code(text) => {
                if let Some(label) = self.image_label.as_mut() {
                    label.push_str(&text);
                } else {
                    self.inline_code(&text);
                }
            }
            Event::InlineMath(text) => self.inline_math(&text),
            Event::DisplayMath(text) => self.display_math(&text),
            Event::SoftBreak => self.text(" "),
            Event::HardBreak => self.hard_break(),
            Event::Rule => self.rule(),
            Event::TaskListMarker(checked) => {
                self.text(if checked { "☑ " } else { "☐ " });
            }
            Event::FootnoteReference(name) => self.text(&format!("[^{name}]")),
            Event::Html(_) | Event::InlineHtml(_) => {}
        }
    }

    fn start(&mut self, tag: Tag<'_>) {
        match tag {
            Tag::Heading { level, .. } => {
                self.finish_line();
                self.blank();

                let mut style = self
                    .current_style()
                    .fg(self.theme.title)
                    .add_modifier(Modifier::BOLD);

                if level == HeadingLevel::H1 {
                    style = style.add_modifier(Modifier::UNDERLINED);
                }

                self.styles.push(style);
            }
            Tag::Paragraph => {
                self.finish_line();

                if self.pending_prefix.is_none() {
                    if let Some(prefix) = self.item_prefix.clone() {
                        self.pending_prefix = Some(prefix);
                    } else if self.quote_depth > 0 {
                        self.pending_prefix = Some("│ ".repeat(self.quote_depth));
                    }
                }
            }
            Tag::Emphasis => self.push_modifier(Modifier::ITALIC),
            Tag::Strong => self.push_modifier(Modifier::BOLD),
            Tag::Strikethrough => self.push_modifier(Modifier::CROSSED_OUT),
            Tag::CodeBlock(kind) => {
                self.finish_line();
                self.blank();

                let diagram = match kind {
                    CodeBlockKind::Fenced(info) => {
                        let language = info
                            .split_whitespace()
                            .next()
                            .unwrap_or_default()
                            .to_ascii_lowercase();

                        match language.as_str() {
                            "mermaid" | "mmd" | "dot" | "graphviz" | "typst" => Some(language),
                            _ => None,
                        }
                    }
                    CodeBlockKind::Indented => None,
                };

                if let Some(language) = diagram {
                    self.diagram_language = Some(language);
                    self.diagram_source.clear();
                } else {
                    self.code_block = true;
                    self.pending_prefix = Some("  ".to_string());
                }
            }
            Tag::List(start) => {
                self.finish_line();
                self.lists.push(match start {
                    Some(next) => ListKind::Ordered(next),
                    None => ListKind::Bullet,
                });
            }
            Tag::Item => self.start_item(),
            Tag::BlockQuote(_) => {
                self.finish_line();
                self.blank();
                self.quote_depth += 1;
                self.pending_prefix = Some("│ ".repeat(self.quote_depth));
            }
            Tag::Link { .. } => {
                let style = self
                    .current_style()
                    .fg(self.theme.accent)
                    .add_modifier(Modifier::UNDERLINED);
                self.styles.push(style);
            }
            Tag::Image { dest_url, .. } => {
                self.finish_line();
                self.flush_text();
                self.image_label = Some(String::new());
                self.image_dest = Some(dest_url.to_string());
            }
            Tag::Table(_) => {
                self.finish_line();
                self.blank();
            }
            Tag::TableRow => self.finish_line(),
            Tag::TableCell if !self.current.is_empty() => {
                self.current
                    .push(Span::styled(" │ ", Style::default().fg(self.theme.border)));
            }
            _ => {}
        }
    }

    fn end(&mut self, tag: TagEnd) {
        match tag {
            TagEnd::Heading(_) => {
                self.finish_line();
                self.blank();
                self.pop_style();
            }
            TagEnd::Paragraph => {
                self.finish_line();

                if self.lists.is_empty() && self.quote_depth == 0 {
                    self.blank();
                }
            }
            TagEnd::Emphasis | TagEnd::Strong | TagEnd::Strikethrough | TagEnd::Link => {
                self.pop_style()
            }
            TagEnd::CodeBlock => {
                if let Some(language) = self.diagram_language.take() {
                    let source = std::mem::take(&mut self.diagram_source);
                    self.finish_line();
                    self.flush_text();
                    self.blocks.push(Block::Diagram(DiagramRef {
                        language,
                        source: source.trim_end().to_string(),
                    }));
                    return;
                }

                self.finish_line();
                self.code_block = false;
                self.pending_prefix = None;
                self.blank();
            }
            TagEnd::List(_) => {
                self.lists.pop();
                self.finish_line();
                self.blank();
            }
            TagEnd::Item => {
                self.finish_line();
                self.item_prefix = None;
                self.pending_prefix = None;
            }
            TagEnd::BlockQuote(_) => {
                self.finish_line();
                self.quote_depth = self.quote_depth.saturating_sub(1);
                self.pending_prefix = None;
                self.blank();
            }
            TagEnd::Image => {
                let label = self.image_label.take().unwrap_or_default();
                let dest = self.image_dest.take().unwrap_or_default();
                let alt = if label.trim().is_empty() {
                    dest.clone()
                } else {
                    label.trim().to_string()
                };

                self.finish_line();
                self.flush_text();
                self.blocks.push(Block::Image(ImageRef { alt, dest }));
            }
            TagEnd::TableRow => self.finish_line(),
            TagEnd::TableHead => {
                self.finish_line();
                self.lines.push(Line::styled(
                    "─".repeat(40),
                    Style::default().fg(self.theme.border),
                ));
            }
            TagEnd::Table => {
                self.finish_line();
                self.blank();
            }
            _ => {}
        }
    }

    fn start_item(&mut self) {
        self.finish_line();

        let depth = self.lists.len().saturating_sub(1);
        let indent = "  ".repeat(depth);
        let marker = match self.lists.last_mut() {
            Some(ListKind::Ordered(next)) => {
                let marker = format!("{next}. ");
                *next = next.saturating_add(1);
                marker
            }
            _ => "• ".to_string(),
        };
        let quote = "│ ".repeat(self.quote_depth);

        self.item_prefix = Some(format!("{quote}{indent}  "));
        self.pending_prefix = Some(format!("{quote}{indent}{marker}"));
    }

    fn current_style(&self) -> Style {
        self.styles.last().cloned().unwrap_or_default()
    }

    fn push_modifier(&mut self, modifier: Modifier) {
        let style = self.current_style().add_modifier(modifier);
        self.styles.push(style);
    }

    fn pop_style(&mut self) {
        if self.styles.len() > 1 {
            self.styles.pop();
        }
    }

    fn ensure_prefix(&mut self) {
        if self.current.is_empty()
            && let Some(prefix) = self.pending_prefix.take()
        {
            let style = Style::default().fg(self.theme.border);
            self.current.push(Span::styled(prefix, style));
        }
    }

    fn text(&mut self, text: &str) {
        if text.is_empty() {
            return;
        }

        self.ensure_prefix();
        let style = self.current_style();
        self.current.push(Span::styled(text.to_string(), style));
    }

    fn inline_code(&mut self, text: &str) {
        self.ensure_prefix();
        let style = self
            .current_style()
            .fg(self.theme.code)
            .bg(self.theme.code_bg)
            .add_modifier(Modifier::BOLD);
        self.current.push(Span::styled(format!(" {text} "), style));
    }

    fn inline_math(&mut self, text: &str) {
        self.ensure_prefix();
        let rendered = unicodeit::replace(text);
        let style = self
            .current_style()
            .fg(self.theme.math)
            .add_modifier(Modifier::ITALIC);
        self.current
            .push(Span::styled(format!(" {rendered} "), style));
    }

    fn display_math(&mut self, text: &str) {
        self.finish_line();
        self.blank();

        let rendered = unicodeit::replace(text.trim());
        self.lines.push(Line::styled(
            format!("  {rendered}"),
            Style::default()
                .fg(self.theme.math)
                .add_modifier(Modifier::BOLD),
        ));
        self.blank();
    }

    fn code_text(&mut self, text: &str) {
        let mut parts = text.split('\n').peekable();

        while let Some(part) = parts.next() {
            if part.is_empty() {
                if self.current.is_empty() {
                    self.lines.push(Line::default());
                }
            } else {
                self.ensure_prefix();
                let style = self
                    .current_style()
                    .fg(self.theme.code)
                    .bg(self.theme.code_bg);
                self.current.push(Span::styled(part.to_string(), style));
            }

            if parts.peek().is_some() {
                self.finish_line();
                self.pending_prefix = Some("  ".to_string());
            }
        }
    }

    fn hard_break(&mut self) {
        self.finish_line();

        if let Some(prefix) = self.item_prefix.clone() {
            self.pending_prefix = Some(prefix);
        } else if self.quote_depth > 0 {
            self.pending_prefix = Some("│ ".repeat(self.quote_depth));
        }
    }

    fn rule(&mut self) {
        self.finish_line();
        self.blank();
        self.lines.push(Line::styled(
            "─".repeat(40),
            Style::default().fg(self.theme.border),
        ));
        self.blank();
    }

    fn finish_line(&mut self) {
        if !self.current.is_empty() {
            self.lines
                .push(Line::from(std::mem::take(&mut self.current)));
        }

        self.pending_prefix = None;
    }

    fn blank(&mut self) {
        self.finish_line();

        if !self.lines.is_empty() && !self.lines.last().is_some_and(|line| line.spans.is_empty()) {
            self.lines.push(Line::default());
        }
    }

    fn flush_text(&mut self) {
        self.finish_line();

        while self.lines.last().is_some_and(|line| line.spans.is_empty()) {
            self.lines.pop();
        }

        if !self.lines.is_empty() {
            self.blocks
                .push(Block::Text(std::mem::take(&mut self.lines)));
        }
    }

    fn finish(mut self) -> Document {
        self.flush_text();
        Document {
            blocks: self.blocks,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;

    fn theme() -> Theme {
        Theme::from_config(&Config::default())
    }

    fn text_of(document: &Document) -> String {
        let mut text = String::new();

        for block in &document.blocks {
            if let Block::Text(lines) = block {
                for line in lines {
                    for span in &line.spans {
                        text.push_str(&span.content);
                    }
                }
            }
        }

        text
    }

    fn images_of(document: &Document) -> Vec<&ImageRef> {
        document
            .blocks
            .iter()
            .filter_map(|block| match block {
                Block::Image(image) => Some(image),
                Block::Text(_) | Block::Diagram(_) => None,
            })
            .collect()
    }

    fn diagrams_of(document: &Document) -> Vec<&DiagramRef> {
        document
            .blocks
            .iter()
            .filter_map(|block| match block {
                Block::Diagram(diagram) => Some(diagram),
                Block::Text(_) | Block::Image(_) => None,
            })
            .collect()
    }

    #[test]
    fn renders_headings_paragraphs_and_inline_markup() {
        let document = render(
            "# Water\n\nBody with **strong** and `code` and [a link](jff:wt1).\n",
            &theme(),
        );
        let text = text_of(&document);

        assert!(text.contains("Water"));
        assert!(text.contains("strong"));
        assert!(text.contains("code"));
        assert!(text.contains("a link"));
    }

    #[test]
    fn renders_lists_and_code_blocks() {
        let document = render(
            "- one\n- two\n\n1. first\n2. second\n\n```rust\nfn main() {}\n```\n",
            &theme(),
        );
        let text = text_of(&document);

        assert!(text.contains("one"));
        assert!(text.contains("two"));
        assert!(text.contains("first"));
        assert!(text.contains("second"));
        assert!(text.contains("fn main() {}"));
    }

    #[test]
    fn renders_quotes_tables_images_and_rules() {
        let document = render(
            "> quoted text\n\n| a | b |\n|---|---|\n| 1 | 2 |\n\n![alt text](x.png)\n\n---\n",
            &theme(),
        );
        let text = text_of(&document);
        let images = images_of(&document);

        assert!(text.contains("quoted text"));
        assert!(text.contains("a"));
        assert!(text.contains("b"));
        assert!(text.contains("─"));
        assert_eq!(images.len(), 1);
        assert_eq!(images[0].alt, "alt text");
        assert_eq!(images[0].dest, "x.png");
    }

    #[test]
    fn splits_text_around_image_blocks() {
        let document = render("Before\n\n![pic](img.png)\n\nAfter\n", &theme());

        assert_eq!(document.blocks.len(), 3);
        assert!(matches!(document.blocks[0], Block::Text(_)));
        assert!(matches!(document.blocks[1], Block::Image(_)));
        assert!(matches!(document.blocks[2], Block::Text(_)));
        assert_eq!(text_of(&document).matches("Before").count(), 1);
        assert_eq!(text_of(&document).matches("After").count(), 1);
    }

    #[test]
    fn renders_latex_math_as_unicode() {
        let document = render("Euler: $e^{i\\pi}+1=0$\n\n$$\\alpha + \\beta$$\n", &theme());
        let text = text_of(&document);

        assert!(text.contains('π'));
        assert!(text.contains('α'));
        assert!(text.contains('β'));
    }

    #[test]
    fn parses_diagram_code_fences() {
        let document = render(
            "```mermaid\ngraph TD\nA-->B\n```\n\n```dot\ndigraph { A -> B }\n```\n",
            &theme(),
        );
        let diagrams = diagrams_of(&document);

        assert_eq!(diagrams.len(), 2);
        assert_eq!(diagrams[0].language, "mermaid");
        assert!(diagrams[0].source.contains("A-->B"));
        assert_eq!(diagrams[1].language, "dot");
        assert!(diagrams[1].source.contains("A -> B"));
    }
}
