use ratatui::layout::Rect;

pub fn inset_rect(r: Rect, dx: u16, dy: u16) -> Rect {
    let w = r.width.saturating_sub(dx * 2);
    let h = r.height.saturating_sub(dy * 2);
    Rect {
        x: r.x + dx.min(r.width),
        y: r.y + dy.min(r.height),
        width: w,
        height: h,
    }
}

pub fn to_rt_rect(r: &treescape_core::Rect) -> Rect {
    Rect {
        x: u16::try_from(r.x).unwrap_or(u16::MAX),
        y: u16::try_from(r.y).unwrap_or(u16::MAX),
        width: u16::try_from(r.width).unwrap_or(u16::MAX),
        height: u16::try_from(r.height).unwrap_or(u16::MAX),
    }
}

pub fn truncate(s: &str, max: usize) -> String {
    if max == 0 {
        return String::new();
    }
    if s.chars().count() <= max {
        s.to_string()
    } else if max == 1 {
        "…".into()
    } else {
        let mut out: String = s.chars().take(max - 1).collect();
        out.push('…');
        out
    }
}

pub fn pad_right(s: &str, width: usize) -> String {
    let n = s.chars().count();
    if n >= width {
        s.to_string()
    } else {
        let mut out = s.to_string();
        out.extend(std::iter::repeat_n(' ', width - n));
        out
    }
}

pub fn fmt_count(n: u64) -> String {
    if n >= 1_000_000 {
        format!("{:.1}M", n as f64 / 1_000_000.0)
    } else if n >= 10_000 {
        format!("{:.1}k", n as f64 / 1_000.0)
    } else if n >= 1_000 {
        format!("{:.2}k", n as f64 / 1_000.0)
    } else {
        n.to_string()
    }
}
