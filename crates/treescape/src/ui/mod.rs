//! Terminal UI layer for treescape.
//!
//! Top-level layout
//!
//! ```text
//! ╭─ treescape — /path/to/scan ──────────────────────────────╮
//! │                                                          │
//! │   main view (tile or list, full width)                   │
//! │                                                          │
//! │ ────────────────────────────────────────────────────── │
//! │   scan stats line                                         │
//! ╰────────────────────────────────────────────────────────── ╯
//! ```
//!
//! The outer rounded box is a single ratatui Block. Inner hairlines
//! (footer divider, optional selection divider in tile mode) are drawn
//! with one-sided Blocks and tee'd into the outer chrome via manual
//! seam patches.

mod color;
mod fmt;
mod footer;
mod list;
mod tile;

pub use color::Theme;

use crate::app::{App, Snapshot, ViewMode};
use humansize::{format_size, BINARY};
use ratatui::{
    layout::{Alignment, Constraint, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{
        block::{Position, Title},
        Block, BorderType, Borders,
    },
    Frame,
};

pub fn draw(f: &mut Frame, app: &mut App) {
    let snap = app.snapshot();
    let area = f.area();

    let outer_title = title_line(app, &snap);
    let bottom_keys = key_bindings_line(app);
    let outer = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(Color::DarkGray))
        .title(Title::from(outer_title).alignment(Alignment::Left))
        .title(
            Title::from(bottom_keys)
                .position(Position::Bottom)
                .alignment(Alignment::Right),
        );
    let inside = outer.inner(area);
    f.render_widget(outer, area);

    // Footer height depends on view: tile mode shows a 2-row selection
    // panel above the hairline + stats line; list mode shows just the
    // hairline + stats line.
    let footer_h: u16 = match app.view {
        ViewMode::Tile => 4,
        ViewMode::List => 2,
    };

    let body = Layout::vertical([Constraint::Min(1), Constraint::Length(footer_h)]).split(inside);
    let main_area = body[0];
    let footer_area = body[1];

    draw_main(f, main_area, app, &snap);
    footer::draw_footer(f, footer_area, app, &snap);

    if footer_area.height >= 2 && area.width >= 2 {
        let y = footer_area.y + footer_area.height - 2;
        patch_outer_seam(f, area, y, '├', '┤');
    }
    if app.view == ViewMode::Tile && footer_area.height >= 4 && area.width >= 2 {
        let y = footer_area.y;
        patch_outer_seam(f, area, y, '├', '┤');
    }
}

fn patch_outer_seam(f: &mut Frame, area: Rect, y: u16, left: char, right: char) {
    let buf = f.buffer_mut();
    let style = Style::default().fg(Color::DarkGray);
    if area.x < buf.area.width && y < buf.area.height {
        let c = &mut buf[(area.x, y)];
        c.set_char(left);
        c.set_style(style);
    }
    let rx = area.x + area.width - 1;
    if rx < buf.area.width && y < buf.area.height {
        let c = &mut buf[(rx, y)];
        c.set_char(right);
        c.set_style(style);
    }
}

fn title_line(app: &App, snap: &Snapshot) -> Line<'static> {
    let path = app.current_path(snap).display().to_string();
    let hidden_bytes = app.hidden_bytes(snap.resolve(&app.path_names));
    // The outer block's border_style is DarkGray. Title spans are rendered
    // on top of the border row — `Style::default()` has `fg: None` which
    // means "don't change", so the cells inherit the border's DarkGray.
    // Use `Color::Reset` on the plain spans to explicitly fall back to the
    // terminal's default foreground.
    let plain = Style::default().fg(Color::Reset);
    let mut spans = vec![
        Span::styled(" ", plain),
        Span::styled(
            "treescape",
            Style::default()
                .fg(Color::Magenta)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled("  —  ", plain),
        Span::styled(path, plain),
    ];
    if hidden_bytes > 0 {
        spans.push(Span::styled(
            format!("  ·  {} hidden", format_size(hidden_bytes, BINARY)),
            plain,
        ));
    }
    if app.view == ViewMode::Tile {
        spans.push(Span::styled(
            format!("  ·  scale: {}", app.scale_mode.label()),
            plain,
        ));
    }
    spans.push(Span::styled(" ", plain));
    Line::from(spans)
}

fn draw_main(f: &mut Frame, area: Rect, app: &mut App, snap: &Snapshot) {
    match app.view {
        ViewMode::Tile => tile::draw_tile_view(f, area, app, snap),
        ViewMode::List => list::draw_list_view(f, area, app, snap),
    }
}

/// Keybinding chips embedded in the bottom outer border. Renders as
/// `─[q exit]─[Enter zoom]─[Esc back]─[v view]─[s scale]─`.
/// The `s scale` chip is omitted in list view since size scaling only
/// affects the tile view.
fn key_bindings_line(app: &App) -> Line<'static> {
    let dim = Style::default().fg(Color::DarkGray);
    let key = Style::default()
        .fg(Color::Gray)
        .add_modifier(Modifier::BOLD);
    let mut chips: Vec<(&'static str, &'static str)> = vec![
        ("q", "exit"),
        ("Enter", "zoom"),
        ("Esc", "back"),
        ("v", "view"),
    ];
    if app.view == ViewMode::Tile {
        chips.push(("s", "scale"));
    }
    chips.push(("H", if app.show_hidden { "hide ." } else { "show ." }));

    let mut spans = vec![Span::styled("─", dim)];
    for (i, (k, label)) in chips.iter().enumerate() {
        spans.push(Span::styled("[", dim));
        spans.push(Span::styled(*k, key));
        spans.push(Span::styled(format!(" {label}"), dim));
        spans.push(Span::styled("]", dim));
        if i + 1 < chips.len() {
            spans.push(Span::styled("─", dim));
        }
    }
    spans.push(Span::styled("─", dim));
    Line::from(spans)
}
