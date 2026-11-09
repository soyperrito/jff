use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Margin, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{
    Block, Borders, List, ListItem, Paragraph, Scrollbar, ScrollbarOrientation, ScrollbarState,
    Wrap,
};
use ratatui_image::protocol::StatefulProtocol;
use ratatui_image::{Resize, StatefulImage};

use super::app::{App, Focus};
use super::markdown::{Block as MdBlock, ImageRef};

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

    let layout = content_layout(app, content_width, inner.height);
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
        let fully_visible = block_top >= viewport_top && block_bottom <= viewport_bottom;
        let source = app.rendered.blocks[entry.index].clone();

        match source {
            MdBlock::Text(lines) => {
                let skip = viewport_top.saturating_sub(block_top);
                let paragraph = Paragraph::new(Text::from(lines)).wrap(Wrap { trim: false });
                frame.render_widget(paragraph.scroll((skip, 0)), rect);
            }
            MdBlock::Image(image) => {
                draw_image(frame, app, &image, rect, fully_visible);
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

fn content_layout(app: &App, width: u16, max_height: u16) -> Vec<LayoutEntry> {
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
            MdBlock::Image(_) => {
                if top > 0 {
                    top = top.saturating_add(1);
                }

                let height = image_block_height(width, max_height, app.images.is_some());
                layout.push(LayoutEntry { index, top, height });
                top = top.saturating_add(height).saturating_add(1);
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

fn image_block_height(width: u16, max_height: u16, enabled: bool) -> u16 {
    if !enabled {
        return 2.min(max_height);
    }

    if max_height <= 1 {
        return max_height;
    }

    let rows = (u32::from(width) / 3)
        .clamp(6, 16)
        .min(u32::from(max_height.saturating_sub(1))) as u16;
    rows + 1
}

fn draw_image(frame: &mut Frame, app: &mut App, image: &ImageRef, area: Rect, fully_visible: bool) {
    let caption = if image.alt.is_empty() {
        image.dest.clone()
    } else {
        image.alt.clone()
    };
    let image_color = app.theme.image;

    if !fully_visible {
        draw_image_message(
            frame,
            area,
            &format!("[image: {caption}] scroll to view"),
            image_color,
        );
        return;
    }

    let root = app.store.root().to_path_buf();
    let entry_path = app.current.abs_path(&root);
    let base_dir = entry_path.parent().unwrap_or(root.as_path()).to_path_buf();

    let Some(images) = app.images.as_mut() else {
        draw_image_message(
            frame,
            area,
            &format!("[image: {caption}] images disabled in config"),
            app.theme.muted,
        );
        return;
    };

    let path = match images.resolve(&root, &base_dir, &image.dest) {
        Ok(path) => path,
        Err(error) => {
            draw_image_message(
                frame,
                area,
                &format!("[image: {caption}] {error}"),
                app.theme.error,
            );
            return;
        }
    };

    match images.protocol(&path) {
        Ok(protocol) => {
            let caption_height = 1.min(area.height);
            let image_height = area.height.saturating_sub(caption_height);

            if image_height > 0 {
                let image_area = Rect {
                    height: image_height,
                    ..area
                };
                let widget: StatefulImage<StatefulProtocol> =
                    StatefulImage::new().resize(Resize::Scale(None));
                frame.render_stateful_widget(widget, image_area, protocol);
            }

            if caption_height > 0 {
                let caption_area = Rect {
                    y: area.y + image_height,
                    height: caption_height,
                    ..area
                };
                frame.render_widget(
                    Paragraph::new(Line::from(Span::styled(
                        format!("[image: {caption}]"),
                        Style::default().fg(image_color),
                    ))),
                    caption_area,
                );
            }
        }
        Err(error) => {
            draw_image_message(
                frame,
                area,
                &format!("[image: {caption}] {error}"),
                app.theme.error,
            );
        }
    }
}

fn draw_image_message(frame: &mut Frame, area: Rect, message: &str, color: Color) {
    if area.height == 0 {
        return;
    }

    frame.render_widget(
        Paragraph::new(Text::from(Line::from(Span::styled(
            message,
            Style::default().fg(color),
        ))))
        .wrap(Wrap { trim: true }),
        area,
    );
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
        let message = vec![
            Line::from(Span::styled(
                "No related entries.",
                Style::default().fg(app.theme.related),
            )),
            Line::from(Span::styled(
                "Add [more:term](path), [more:concept](path) or [more:t/c](ref).",
                Style::default().fg(app.theme.muted),
            )),
        ];

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
    let keys = "Tab focus · ↑↓/jk move · Enter open · e edit · r reload · Esc back · q quit";

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
