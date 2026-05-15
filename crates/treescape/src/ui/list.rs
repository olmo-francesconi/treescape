use super::color::{icon_for, tile_color};
use super::fmt::{inset_rect, pad_right, truncate};
use crate::app::{App, Snapshot};
use humansize::{format_size, BINARY};
use ratatui::{
    layout::{Alignment, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
    Frame,
};

pub fn draw_list_view(f: &mut Frame, area: Rect, app: &mut App, snap: &Snapshot) {
    let cur = snap.resolve(&app.path_names);
    let visible = app.visible_children(cur);
    if app.selected_name.is_none() {
        app.selected_name = visible.first().map(|c| c.name.clone());
    }
    // Keep last_layout empty when in list mode; spatial nav doesn't apply.
    app.last_layout.clear();
    app.last_order = visible.iter().map(|c| c.name.clone()).collect();

    let inner = inset_rect(area, 1, 0);
    if inner.height == 0 {
        return;
    }
    let selected_idx = app.selected_index(&visible).unwrap_or(0);

    let rows = inner.height as usize;
    if selected_idx < app.list_offset {
        app.list_offset = selected_idx;
    } else if selected_idx >= app.list_offset + rows {
        app.list_offset = selected_idx + 1 - rows;
    }
    if visible.len() <= rows {
        app.list_offset = 0;
    } else if app.list_offset + rows > visible.len() {
        app.list_offset = visible.len() - rows;
    }

    let view_total = cur.size.max(1);
    let inner_w = inner.width as usize;

    let size_w = 11;
    let pct_w = 7;
    let bar_w = 18;
    let spacers = 3 * 3;
    let name_w = inner_w
        .saturating_sub(size_w + pct_w + bar_w + spacers)
        .max(8);

    let lines: Vec<Line> = visible
        .iter()
        .enumerate()
        .skip(app.list_offset)
        .take(rows)
        .map(|(i, child)| {
            let selected = i == selected_idx;
            let pct = child.size as f64 / view_total as f64;
            let bar = list_bar(child.size, view_total, bar_w);

            let name_text = if child.is_link {
                format!("{} → {}", icon_for(child), child.name)
            } else {
                format!("{} {}", icon_for(child), child.name)
            };
            let name = truncate(&name_text, name_w);
            let name_padded = pad_right(&name, name_w);

            let kind_color = tile_color(child);
            let mut name_style = Style::default();
            if child.is_dir {
                name_style = name_style.add_modifier(Modifier::BOLD);
            }
            if child.is_link {
                name_style = name_style.add_modifier(Modifier::ITALIC);
            }
            if selected {
                name_style = name_style.add_modifier(Modifier::REVERSED);
            }

            let bar_style = if selected {
                Style::default()
                    .fg(kind_color)
                    .add_modifier(Modifier::REVERSED)
            } else {
                Style::default().fg(kind_color)
            };
            let dim_style = if selected {
                Style::default()
                    .fg(Color::DarkGray)
                    .add_modifier(Modifier::REVERSED)
            } else {
                Style::default().fg(Color::DarkGray)
            };
            let value_style = if selected {
                Style::default().add_modifier(Modifier::REVERSED)
            } else {
                Style::default()
            };

            Line::from(vec![
                Span::styled(name_padded, name_style),
                Span::styled("   ", value_style),
                Span::styled(
                    format!("{:>1$}", format_size(child.size, BINARY), size_w),
                    value_style,
                ),
                Span::styled("   ", value_style),
                Span::styled(bar.0, bar_style),
                Span::styled(bar.1, dim_style),
                Span::styled("   ", value_style),
                Span::styled(
                    format!("{:>1$}", format!("{:.1}%", pct * 100.0), pct_w),
                    value_style,
                ),
            ])
        })
        .collect();

    f.render_widget(Paragraph::new(lines), inner);

    if visible.is_empty() {
        let msg = if snap.done {
            "(empty directory)"
        } else {
            "scanning…"
        };
        let p = Paragraph::new(msg)
            .style(Style::default().fg(Color::DarkGray))
            .alignment(Alignment::Center);
        f.render_widget(p, area);
    }
}

/// Returns the filled and empty halves of a horizontal bar as separate
/// strings so callers can style them independently.
fn list_bar(value: u64, max: u64, width: usize) -> (String, String) {
    let frac = (value as f64) / (max.max(1) as f64);
    let filled = ((frac * width as f64).round() as usize).min(width);
    ("▓".repeat(filled), "░".repeat(width - filled))
}
