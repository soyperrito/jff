use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Margin, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{
    Block, Borders, List, ListItem, Paragraph, Scrollbar, ScrollbarOrientation, ScrollbarState,
    Wrap,
};

use super::app::{App, Focus};
use super::markdown::Block as MdBlock;

pub fn draw(frame: &mut Frame, app: &mut App) {
    let area = frame.area();
    let chunks = Layout::vertical([
        Constraint::Length(3),
        Constraint::Min(5),
        Constraint::Length(9),
        Constraint::Length(2),
    ])
    .split(area);

    draw_content(frame, app, chunks[1]);
    draw_header(frame, app, chunks[0]);
    draw_related(frame, app, chunks[2]);
    draw_footer(frame, app, chunks[3]);
}

fn draw_header(frame: &mut Frame, app: &App, area: ratatui::layout::Rect) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(app.theme.border_type)
        .border_style(Style::default().fg(app.theme.border))
        .title(Line::from(vec![
            Span::styled(
                " jff ",
                Style::default()
                    .fg(app.theme.accent)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                app.current.title.clone(),
                Style::default()
                    .fg(app.theme.title)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw(" "),
        ]));

    let line = if app.total_lines == 0 {
        "0/0".to_string()
    } else {
        format!(
            "{}/{}",
            app.scroll.saturating_add(1).min(app.total_lines),
            app.total_lines
        )
    };
    let meta = format!(
        "{} · {} · {} related · line {} · focus {}",
        app.current.kind,
        app.current.reference,
        app.related.len(),
        line,
        app.focus.label()
    );

    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(
            meta,
            Style::default().fg(app.theme.muted),
        )))
        .block(block),
        area,
    );
}

struct LayoutEntry {
    index: usize,
    top: u16,
    height: u16,
}

fn draw_content(frame: &mut Frame, app: &mut App, area: ratatui::layout::Rect) {
    let border_color = if app.focus == Focus::Content {
        app.theme.accent
    } else {
        app.theme.border
    };
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(app.theme.border_type)
        .border_style(Style::default().fg(border_color))
        .title(Line::from(Span::styled(
            " Content ",
            Style::default().fg(app.theme.text),
        )));

    frame.render_widget(block, area);

    let inner = area.inner(Margin {
        horizontal: 1,
        vertical: 1,
    });

    if inner.width == 0 || inner.height == 0 {
        app.total_lines = 0;
        app.max_scroll = 0;
        return;
    }

    let scrollbar_width = 1;
    let content_width = inner.width.saturating_sub(scrollbar_width);

    if content_width == 0 {
        app.total_lines = 0;
        app.max_scroll = 0;
        return;
    }

    let content_area = Rect {
        width: content_width,
        ..inner
    };
    let scrollbar_area = Rect {
        x: inner.x + content_width,
        y: inner.y,
        width: scrollbar_width,
        height: inner.height,
    };

    let layout = content_layout(app, content_width);
    let total = layout
        .last()
        .map(|entry| entry.top.saturating_add(entry.height))
        .unwrap_or(0);

    app.total_lines = total;
    app.max_scroll = total.saturating_sub(inner.height);
    app.scroll = app.scroll.min(app.max_scroll);

    if app.rendered.blocks.is_empty() {
        frame.render_widget(
            Paragraph::new(Line::from(Span::styled(
                "This entry is empty. Press e to edit it.",
                Style::default().fg(app.theme.muted),
            ))),
            content_area,
        );
        render_scrollbar(frame, app, scrollbar_area);
        return;
    }

    let viewport_top = app.scroll;
    let viewport_bottom = app.scroll.saturating_add(inner.height);

    for entry in &layout {
        let block_top = entry.top;
        let block_bottom = entry.top.saturating_add(entry.height);

        if block_bottom <= viewport_top {
            continue;
        }

        if block_top >= viewport_bottom {
            break;
        }

        let visible_top = block_top.max(viewport_top);
        let visible_bottom = block_bottom.min(viewport_bottom);
        let visible_height = visible_bottom.saturating_sub(visible_top);

        if visible_height == 0 {
            continue;
        }

        let rect = Rect {
            x: content_area.x,
            y: inner.y + (visible_top - viewport_top),
            width: content_area.width,
            height: visible_height,
        };
        let source = app.rendered.blocks[entry.index].clone();

        match source {
            MdBlock::Text(lines) => {
                let skip = viewport_top.saturating_sub(block_top);
                let paragraph = Paragraph::new(Text::from(lines)).wrap(Wrap { trim: false });
                frame.render_widget(paragraph.scroll((skip, 0)), rect);
            }
        }
    }

    render_scrollbar(frame, app, scrollbar_area);
}

fn render_scrollbar(frame: &mut Frame, app: &App, area: Rect) {
    if area.width == 0 || area.height == 0 {
        return;
    }

    let mut state = ScrollbarState::new(app.total_lines.max(1) as usize)
        .position(app.scroll as usize)
        .viewport_content_length(area.height as usize);
    let scrollbar = Scrollbar::new(ScrollbarOrientation::VerticalRight)
        .thumb_symbol("█")
        .track_symbol(Some("│"))
        .begin_symbol(Some("↑"))
        .end_symbol(Some("↓"))
        .thumb_style(Style::default().fg(app.theme.scrollbar_thumb))
        .track_style(Style::default().fg(app.theme.scrollbar))
        .begin_style(Style::default().fg(app.theme.scrollbar))
        .end_style(Style::default().fg(app.theme.scrollbar));

    frame.render_stateful_widget(scrollbar, area, &mut state);
}

fn content_layout(app: &App, width: u16) -> Vec<LayoutEntry> {
    let mut layout = Vec::new();
    let mut top = 0u16;

    for (index, block) in app.rendered.blocks.iter().enumerate() {
        match block {
            MdBlock::Text(lines) => {
                let height = text_height(lines, width);

                if height > 0 {
                    layout.push(LayoutEntry { index, top, height });
                    top = top.saturating_add(height);
                }
            }
        }
    }

    layout
}

fn text_height(lines: &[Line<'static>], width: u16) -> u16 {
    if lines.is_empty() {
        return 0;
    }

    let paragraph = Paragraph::new(Text::from(lines.to_vec())).wrap(Wrap { trim: false });
    paragraph.line_count(width).min(u16::MAX as usize) as u16
}

fn draw_related(frame: &mut Frame, app: &mut App, area: ratatui::layout::Rect) {
    let border_color = if app.focus == Focus::Related {
        app.theme.accent
    } else {
        app.theme.border
    };
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(app.theme.border_type)
        .border_style(Style::default().fg(border_color))
        .title(Line::from(Span::styled(
            format!(" Related ({}) ", app.related.len()),
            Style::default().fg(app.theme.text),
        )));

    if app.related.is_empty() {
        let message = vec![Line::from(Span::styled(
            "No related entries.",
            Style::default().fg(app.theme.related),
        ))];

        frame.render_widget(
            Paragraph::new(message)
                .block(block)
                .wrap(Wrap { trim: true }),
            area,
        );
        return;
    }

    let label_width = (area.width as usize).saturating_sub(31).clamp(12, 40);
    let items: Vec<ListItem> = app
        .related
        .iter()
        .map(|item| {
            let (label, kind, reference, style) = match &item.target {
                Some(target) => (
                    item.heading.clone().unwrap_or_else(|| target.title.clone()),
                    target.kind.to_string(),
                    target.reference.clone(),
                    Style::default().fg(app.theme.related),
                ),
                None => (
                    "[missing]".to_string(),
                    item.link.kind.to_string(),
                    item.link.dest.clone(),
                    Style::default().fg(app.theme.error),
                ),
            };

            ListItem::new(Line::from(vec![
                Span::styled(
                    format!(
                        "{:<width$}",
                        ellipsize(&label, label_width),
                        width = label_width
                    ),
                    style,
                ),
                Span::styled(
                    format!(" {:<8}", kind),
                    Style::default().fg(app.theme.muted),
                ),
                Span::styled(reference, Style::default().fg(app.theme.muted)),
            ]))
        })
        .collect();

    app.related_state.select(Some(app.selected));

    let list = List::new(items)
        .block(block)
        .highlight_symbol("▶ ")
        .highlight_style(
            Style::default()
                .bg(app.theme.highlight_bg)
                .fg(app.theme.highlight_fg)
                .add_modifier(Modifier::BOLD),
        );

    frame.render_stateful_widget(list, area, &mut app.related_state);
}

fn draw_footer(frame: &mut Frame, app: &App, area: ratatui::layout::Rect) {
    let status = app.status.clone().unwrap_or_default();
    let status_style = if app.status.is_some() {
        Style::default().fg(app.theme.status)
    } else {
        Style::default()
    };
    let keys = "Tab focus · ↑↓/jk move · Enter open · e edit · r reload · ←/Esc back · q quit";

    frame.render_widget(
        Paragraph::new(vec![
            Line::from(Span::styled(status, status_style)),
            Line::from(Span::styled(keys, Style::default().fg(app.theme.muted))),
        ]),
        area,
    );
}

fn ellipsize(value: &str, max_chars: usize) -> String {
    if max_chars == 0 {
        return String::new();
    }

    if value.chars().count() <= max_chars {
        return value.to_string();
    }

    let mut truncated: String = value.chars().take(max_chars.saturating_sub(1)).collect();
    truncated.push('…');
    truncated
}
