//! treescape-core: UI-agnostic primitives for scanning a directory tree
//! and laying it out as a squarified treemap.

pub mod scan;
pub mod tree;
pub mod treemap;

pub use scan::{start_scan, ScanShared};
pub use tree::Node;
pub use treemap::squarify;

/// Pixel-space rectangle used by the treemap layout. Coordinates and
/// extents are non-negative integers; consumers convert into their own
/// space (`ratatui::layout::Rect`, SVG viewport, etc.) as needed.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Rect {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

impl Rect {
    pub const fn new(x: u32, y: u32, width: u32, height: u32) -> Self {
        Self {
            x,
            y,
            width,
            height,
        }
    }
}
