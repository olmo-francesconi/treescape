use super::color::icon_for;
use super::color::{label_fg, selected_shade, tile_color, Theme};
use super::fmt::{to_rt_rect, truncate};
use crate::app::{App, Snapshot};
use humansize::{format_size, BINARY};
use ratatui::{
    buffer::Buffer,
    layout::{Alignment, Rect},
    style::{Color, Modifier, Style},
    widgets::Paragraph,
    Frame,
};
use treescape_core::{tree::Node, treemap};

pub fn draw_tile_view(f: &mut Frame, area: Rect, app: &mut App, snap: &Snapshot) {
    let cur = snap.resolve(&app.path_names);
    let visible = app.visible_children(cur);

    // Tile areas are warped via `app.scale_mode` (linear / log / sqrt).
    // The displayed size text on each tile is still the real size — only
    // the visual area is warped. Press `s` to cycle modes.
    let mode = app.scale_mode;
    let weights: Vec<u64> = visible.iter().map(|c| mode.weight(c.size)).collect();

    // Tell squarify how wide each child's label needs to be so it can
    // prefer row splits that keep names un-truncated. Width budget per
    // cell: "{icon} {name}" (or "{icon} → {name}" for symlinks) plus 1
    // column of padding on each side — matches `draw_cell` below.
    let name_widths: Vec<usize> = visible
        .iter()
        .map(|c| {
            let prefix = if c.is_link { 4 } else { 2 };
            c.name.chars().count() + prefix + 2
        })
        .collect();

    let core_area = treescape_core::Rect::new(
        area.x as u32,
        area.y as u32,
        area.width as u32,
        area.height as u32,
    );
    let core_rects = treemap::squarify_with_labels(&weights, &name_widths, core_area);
    let rects: Vec<Rect> = core_rects.iter().map(to_rt_rect).collect();

    app.last_layout = rects.clone();
    app.last_order = visible.iter().map(|c| c.name.clone()).collect();

    if app.selected_name.is_none() {
        app.selected_name = visible.first().map(|c| c.name.clone());
    }
    let selected_idx = app.selected_index(&visible);
    app.last_selected_idx = selected_idx;

    let buf = f.buffer_mut();
    for (i, (child, r)) in visible.iter().zip(rects.iter()).enumerate() {
        draw_cell(buf, *r, child, Some(i) == selected_idx, app.theme);
    }

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

fn draw_cell(buf: &mut Buffer, rect: Rect, node: &Node, selected: bool, theme: Theme) {
    if rect.width == 0 || rect.height == 0 {
        return;
    }

    let base = tile_color(node);
    let fill = if selected {
        selected_shade(base, theme)
    } else {
        base
    };
    let fg = label_fg(fill);

    let body_style = Style::default().bg(fill).fg(fg);

    // Solid fill across the entire cell. No borders, no grid.
    for y in rect.top()..rect.bottom() {
        for x in rect.left()..rect.right() {
            if x >= buf.area.width || y >= buf.area.height {
                continue;
            }
            let cell = &mut buf[(x, y)];
            cell.set_char(' ');
            cell.set_style(body_style);
        }
    }

    if rect.width < 3 || rect.height < 1 {
        return;
    }
    let pad_x: u16 = if rect.width >= 5 { 1 } else { 0 };
    let usable_w = rect.width.saturating_sub(pad_x * 2) as usize;
    if usable_w == 0 {
        return;
    }

    let label_text = if node.is_link {
        format!("{} → {}", icon_for(node), node.name)
    } else {
        format!("{} {}", icon_for(node), node.name)
    };
    let label = truncate(&label_text, usable_w);
    let mut label_style = body_style;
    if node.is_dir {
        label_style = label_style.add_modifier(Modifier::BOLD);
    }
    if selected {
        label_style = label_style.add_modifier(Modifier::BOLD);
    }
    if node.is_link {
        label_style = label_style.add_modifier(Modifier::ITALIC);
    }
    buf.set_string(rect.left() + pad_x, rect.top(), &label, label_style);

    if rect.height >= 2 && rect.width >= 8 {
        let size_str = truncate(&format_size(node.size, BINARY), usable_w);
        buf.set_string(rect.left() + pad_x, rect.top() + 1, &size_str, body_style);
    }
}
