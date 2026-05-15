use ratatui::style::Color;
use std::path::Path;
use treescape_core::tree::Node;

/// Detected terminal theme. Drives the direction of the selected-tile
/// brightness shift (lighter on dark, darker on light).
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum Theme {
    Dark,
    Light,
}

/// Unique color for a node, hashed from its absolute path so it stays
/// stable across re-scans. Pastel range — low saturation, high lightness.
/// Slight per-node S/L wobble keeps same-hue siblings distinguishable.
pub fn tile_color(node: &Node) -> Color {
    let key = node.path.to_string_lossy();
    let h = fnv_hash(&key);
    let hue = (h % 360) as f64;
    let s_jitter = (h.wrapping_div(360) % 17) as f64 / 17.0;
    let l_jitter = (h.wrapping_div(7919) % 13) as f64 / 13.0;
    let s = 0.30 + 0.15 * s_jitter;
    let l = 0.72 + 0.10 * l_jitter;
    let (r, g, b) = hsl_to_rgb(hue, s, l);
    Color::Rgb(r, g, b)
}

/// Shift a pastel tile color to mark it as selected. Boosts saturation (so
/// the same hue pops against its washed-out neighbours) and pushes lightness
/// toward the contrasting end of the theme.
pub fn selected_shade(base: Color, theme: Theme) -> Color {
    let Color::Rgb(r, g, b) = base else {
        return base;
    };
    let (h, mut s, mut l) = rgb_to_hsl(r, g, b);
    s = (s + 0.40).min(0.90);
    let target = match theme {
        Theme::Dark => 0.88,
        Theme::Light => 0.38,
    };
    let alpha = 0.55;
    l += (target - l) * alpha;
    let (r, g, b) = hsl_to_rgb(h, s, l);
    Color::Rgb(r, g, b)
}

/// Pick a readable label color against a given tile color.
pub fn label_fg(bg: Color) -> Color {
    let Color::Rgb(r, g, b) = bg else {
        return Color::Reset;
    };
    let lum = 0.299 * r as f32 + 0.587 * g as f32 + 0.114 * b as f32;
    if lum > 140.0 {
        Color::Rgb(20, 20, 24)
    } else {
        Color::Rgb(238, 238, 244)
    }
}

/// Coarse content class for a file, derived from its extension.
/// Single source of truth — icon lookup goes through this; future
/// extension-based coloring would route through this too.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum ExtKind {
    Code,
    Markup,
    Text,
    Image,
    Video,
    Audio,
    Archive,
    Pdf,
    Other,
}

pub fn classify(name: &str) -> ExtKind {
    let ext = Path::new(name)
        .extension()
        .map(|s| s.to_string_lossy().to_lowercase())
        .unwrap_or_default();
    match ext.as_str() {
        "rs" | "go" | "py" | "rb" | "java" | "kt" | "swift" | "c" | "h" | "cpp" | "hpp" | "cc"
        | "js" | "mjs" | "cjs" | "jsx" | "ts" | "tsx" | "lua" | "pl" | "sh" | "bash" | "zsh"
        | "php" => ExtKind::Code,
        "html" | "htm" | "xml" | "svg" | "css" | "scss" | "sass" | "less" => ExtKind::Markup,
        "md" | "mdx" | "txt" | "rst" | "adoc" | "asciidoc" | "json" | "yaml" | "yml" | "toml"
        | "csv" | "tsv" | "ini" | "conf" | "log" => ExtKind::Text,
        "png" | "jpg" | "jpeg" | "gif" | "webp" | "bmp" | "tiff" | "ico" | "psd" | "ai" => {
            ExtKind::Image
        }
        "mp4" | "mov" | "mkv" | "avi" | "webm" | "flv" => ExtKind::Video,
        "mp3" | "wav" | "flac" | "ogg" | "m4a" | "aac" => ExtKind::Audio,
        "zip" | "tar" | "gz" | "bz2" | "xz" | "7z" | "rar" | "dmg" | "iso" | "pkg" | "deb"
        | "rpm" => ExtKind::Archive,
        "pdf" => ExtKind::Pdf,
        _ => ExtKind::Other,
    }
}

/// Nerd Font glyph for a node, hardcoded against Font Awesome codepoints
/// that ship with every patched Nerd Font. Falls back to a generic file
/// icon for unknown extensions.
pub fn icon_for(node: &Node) -> &'static str {
    if node.is_link {
        return "\u{f0c1}"; //  link
    }
    if node.is_dir {
        return "\u{f07b}";
    }
    match classify(&node.name) {
        ExtKind::Code => "\u{f1c9}",
        ExtKind::Markup => "\u{f13b}",
        ExtKind::Text => "\u{f15c}",
        ExtKind::Image => "\u{f1c5}",
        ExtKind::Video => "\u{f1c8}",
        ExtKind::Audio => "\u{f1c7}",
        ExtKind::Archive => "\u{f1c6}",
        ExtKind::Pdf => "\u{f1c1}",
        ExtKind::Other => "\u{f15b}",
    }
}

fn fnv_hash(s: &str) -> u32 {
    let mut h: u32 = 2166136261;
    for b in s.bytes() {
        h ^= b as u32;
        h = h.wrapping_mul(16777619);
    }
    h
}

fn hsl_to_rgb(h: f64, s: f64, l: f64) -> (u8, u8, u8) {
    let c = (1.0 - (2.0 * l - 1.0).abs()) * s;
    let h2 = h / 60.0;
    let x = c * (1.0 - (h2.rem_euclid(2.0) - 1.0).abs());
    let (r1, g1, b1) = match h2 as u32 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    let m = l - c / 2.0;
    (
        ((r1 + m) * 255.0).clamp(0.0, 255.0) as u8,
        ((g1 + m) * 255.0).clamp(0.0, 255.0) as u8,
        ((b1 + m) * 255.0).clamp(0.0, 255.0) as u8,
    )
}

fn rgb_to_hsl(r: u8, g: u8, b: u8) -> (f64, f64, f64) {
    let r = r as f64 / 255.0;
    let g = g as f64 / 255.0;
    let b = b as f64 / 255.0;
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let l = (max + min) / 2.0;
    if (max - min).abs() < f64::EPSILON {
        return (0.0, 0.0, l);
    }
    let d = max - min;
    let s = if l > 0.5 {
        d / (2.0 - max - min)
    } else {
        d / (max + min)
    };
    let h = if max == r {
        ((g - b) / d + if g < b { 6.0 } else { 0.0 }) * 60.0
    } else if max == g {
        ((b - r) / d + 2.0) * 60.0
    } else {
        ((r - g) / d + 4.0) * 60.0
    };
    (h, s, l)
}
