use super::color::{icon_for, tile_color};
use super::fmt::{fmt_count, truncate};
use crate::app::{App, Snapshot, ViewMode};
use humansize::{format_size, BINARY};
use ratatui::{
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
    Frame,
};
use std::time::Instant;
use treescape_core::tree::Node;

const SPINNER: &[&str] = &["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];

// Layout depends on the view:
//
//   Tile mode (4 rows):           List mode (2 rows):
//     row 0  ─── hairline ───       row 0  ─── hairline ───
//     row 1  selection line         row 1  scan stats
//     row 2  ─── hairline ───
//     row 3  scan stats
pub fn draw_footer(f: &mut Frame, area: Rect, app: &App, snap: &Snapshot) {
    if area.height == 0 {
        return;
    }
    let cur = snap.resolve(&app.path_names);

    let stats_row = area.y + area.height - 1;
    let stats_hairline_row = area.y + area.height - 2;

    if area.height >= 2 {
        draw_hairline_row(f, area.x, stats_hairline_row, area.width);
    }

    if app.view == ViewMode::Tile && area.height >= 4 {
        let selection_hairline_row = area.y;
        let selection_row = area.y + 1;
        draw_hairline_row(f, area.x, selection_hairline_row, area.width);

        let visible = app.visible_children(cur);
        let selected = app
            .selected_index(&visible)
            .and_then(|i| visible.get(i))
            .map(|c| c.as_ref());
        draw_selection_line(f, area.x, selection_row, area.width, selected);
    }

    let stats_area = Rect {
        x: area.x + 1,
        y: stats_row,
        width: area.width.saturating_sub(2),
        height: 1,
    };
    let line = scan_stats_line(snap);
    f.render_widget(Paragraph::new(line), stats_area);
}

fn draw_hairline_row(f: &mut Frame, x: u16, y: u16, width: u16) {
    let area = Rect {
        x,
        y,
        width,
        height: 1,
    };
    let hairline = Block::default()
        .borders(Borders::TOP)
        .border_style(Style::default().fg(Color::DarkGray));
    f.render_widget(hairline, area);
}

fn draw_selection_line(f: &mut Frame, x: u16, y: u16, width: u16, selected: Option<&Node>) {
    let line_area = Rect {
        x: x + 1,
        y,
        width: width.saturating_sub(2),
        height: 1,
    };

    let line = match selected {
        Some(child) => {
            let kind = if child.is_link {
                "link"
            } else if child.is_dir {
                "dir "
            } else {
                "file"
            };
            let items = if child.is_dir {
                let n = child.total_count().saturating_sub(1) as u64;
                format!(" · {} items", fmt_count(n))
            } else {
                String::new()
            };
            Line::from(vec![
                Span::styled(
                    format!(" {} ", kind),
                    Style::default().fg(Color::Black).bg(tile_color(child)),
                ),
                Span::raw("  "),
                Span::styled(
                    format!("{} ", icon_for(child)),
                    Style::default().fg(tile_color(child)),
                ),
                Span::styled(
                    child.name.clone(),
                    Style::default().add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    format!("  ·  {}{}", format_size(child.size, BINARY), items),
                    Style::default().fg(Color::Gray),
                ),
            ])
        }
        None => Line::from(Span::styled(
            " no selection",
            Style::default().fg(Color::DarkGray),
        )),
    };

    f.render_widget(Paragraph::new(line), line_area);
}

fn scan_stats_line(snap: &Snapshot) -> Line<'static> {
    if snap.done {
        let elapsed = snap
            .finished_at
            .map(|f| f.duration_since(snap.started_at))
            .unwrap_or_default();
        let mut spans = vec![
            Span::styled(
                " done ",
                Style::default()
                    .fg(Color::Green)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled("· ", Style::default().fg(Color::DarkGray)),
            Span::styled(
                fmt_count(snap.files_scanned),
                Style::default().add_modifier(Modifier::BOLD),
            ),
            Span::styled(" files", Style::default().fg(Color::DarkGray)),
            Span::styled(" · ", Style::default().fg(Color::DarkGray)),
            Span::styled(
                format_size(snap.bytes_scanned, BINARY),
                Style::default().add_modifier(Modifier::BOLD),
            ),
            Span::styled(" · ", Style::default().fg(Color::DarkGray)),
            Span::styled(format!("{:.2}s", elapsed.as_secs_f64()), Style::default()),
        ];
        if snap.unreadable > 0 {
            spans.push(Span::styled(" · ", Style::default().fg(Color::DarkGray)));
            spans.push(Span::styled(
                format!("{} unreadable", fmt_count(snap.unreadable)),
                Style::default().fg(Color::Yellow),
            ));
        }
        Line::from(spans)
    } else {
        let i = (Instant::now().duration_since(snap.started_at).as_millis() / 80) as usize
            % SPINNER.len();
        let mut spans = vec![
            Span::styled(
                format!(" {} ", SPINNER[i]),
                Style::default().fg(Color::Yellow),
            ),
            Span::styled(
                "scanning",
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(" · ", Style::default().fg(Color::DarkGray)),
            Span::styled(
                fmt_count(snap.files_scanned),
                Style::default().add_modifier(Modifier::BOLD),
            ),
            Span::styled(" files", Style::default().fg(Color::DarkGray)),
            Span::styled(" · ", Style::default().fg(Color::DarkGray)),
            Span::styled(
                format_size(snap.bytes_scanned, BINARY),
                Style::default().add_modifier(Modifier::BOLD),
            ),
        ];
        if snap.unreadable > 0 {
            spans.push(Span::styled(" · ", Style::default().fg(Color::DarkGray)));
            spans.push(Span::styled(
                format!("{} unreadable", fmt_count(snap.unreadable)),
                Style::default().fg(Color::Yellow),
            ));
        }
        if let Some(p) = snap.current_path.as_ref() {
            spans.push(Span::styled(" · ", Style::default().fg(Color::DarkGray)));
            spans.push(Span::styled(
                truncate(&p.display().to_string(), 60),
                Style::default().fg(Color::DarkGray),
            ));
        }
        Line::from(spans)
    }
}
