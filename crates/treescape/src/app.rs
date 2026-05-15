use crate::ui::Theme;
use crossterm::event::KeyCode;
use ratatui::layout::Rect;
use std::{
    path::PathBuf,
    sync::{Arc, Mutex},
    time::Instant,
};
use treescape_core::{scan::ScanShared, tree::Node};

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum ViewMode {
    Tile,
    List,
}

/// How tile cell areas relate to byte sizes.
/// - `Linear`: area ∝ size. Truth-faithful, but a single huge sibling
///   crushes everything else to <1 cell.
/// - `Log`: area ∝ ln(1 + size). Small files stay visible; intra-magnitude
///   differences are flattened.
/// - `Sqrt`: middle ground. Big-vs-small is still dominant; same-magnitude
///   siblings still distinguishable.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum ScaleMode {
    Linear,
    Log,
    Sqrt,
}

impl ScaleMode {
    pub fn next(self) -> Self {
        match self {
            ScaleMode::Linear => ScaleMode::Log,
            ScaleMode::Log => ScaleMode::Sqrt,
            ScaleMode::Sqrt => ScaleMode::Linear,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            ScaleMode::Linear => "linear",
            ScaleMode::Log => "log",
            ScaleMode::Sqrt => "sqrt",
        }
    }

    /// Convert a byte size into a treemap weight. Returns u64 because
    /// `treemap::squarify` takes integer weights; we scale floats up so
    /// rounding doesn't collapse small distinctions.
    pub fn weight(self, size: u64) -> u64 {
        let s = size as f64;
        let raw = match self {
            ScaleMode::Linear => s,
            ScaleMode::Log => (1.0 + s).ln() * 1_000_000.0,
            ScaleMode::Sqrt => s.sqrt() * 1_000.0,
        };
        raw.max(1.0) as u64
    }
}

pub struct App {
    pub state: Arc<Mutex<ScanShared>>,
    /// Child names from root down to the currently-zoomed directory.
    pub path_names: Vec<String>,
    /// Name of the selected child within the current directory.
    pub selected_name: Option<String>,
    /// Rects from the previous frame's layout (used by spatial nav in tile view).
    pub last_layout: Vec<Rect>,
    /// Names paired with `last_layout`, same order.
    pub last_order: Vec<String>,
    /// Selected cell's index within `last_layout` (set by the renderer).
    pub last_selected_idx: Option<usize>,
    pub view: ViewMode,
    /// Vertical scroll offset for the list view.
    pub list_offset: usize,
    /// Detected terminal theme; drives selected-tile shade direction.
    pub theme: Theme,
    /// How tile cell areas are scaled relative to byte sizes.
    pub scale_mode: ScaleMode,
    /// Whether dotfile children are visible. Hidden files are always
    /// *scanned* (so parent totals stay honest); this only affects render.
    /// Toggle with `H`.
    pub show_hidden: bool,
}

pub struct Snapshot {
    pub tree: Arc<Node>,
    pub files_scanned: u64,
    pub bytes_scanned: u64,
    pub unreadable: u64,
    pub current_path: Option<PathBuf>,
    pub done: bool,
    pub started_at: Instant,
    pub finished_at: Option<Instant>,
}

impl Snapshot {
    /// Walk `path_names` from the root. Stops early if a name doesn't resolve
    /// (e.g. the path points into a subtree that hasn't been scanned yet).
    pub fn resolve<'a>(&'a self, path_names: &[String]) -> &'a Node {
        let mut node: &Node = &self.tree;
        for name in path_names {
            match node.children.iter().find(|c| &c.name == name) {
                Some(c) => node = c,
                None => return node,
            }
        }
        node
    }
}

impl App {
    pub fn new(state: Arc<Mutex<ScanShared>>, theme: Theme, show_hidden: bool) -> Self {
        Self {
            state,
            path_names: Vec::new(),
            selected_name: None,
            last_layout: Vec::new(),
            last_order: Vec::new(),
            last_selected_idx: None,
            view: ViewMode::Tile,
            list_offset: 0,
            theme,
            scale_mode: ScaleMode::Log,
            show_hidden,
        }
    }

    /// Children of `node` that should be displayed under the current
    /// `show_hidden` setting. Hidden files (`.foo`) are filtered out
    /// when `show_hidden` is false. The parent's `size` still includes
    /// them — only the rendered list shrinks.
    pub fn visible_children<'a>(&self, node: &'a Node) -> Vec<&'a Arc<Node>> {
        node.children
            .iter()
            .filter(|c| self.show_hidden || !c.name.starts_with('.'))
            .collect()
    }

    /// Sum the sizes of children of `node` that are currently filtered out
    /// (dotfiles when `show_hidden` is false). Used by the title bar to
    /// surface "you're missing N bytes" honestly.
    pub fn hidden_bytes(&self, node: &Node) -> u64 {
        if self.show_hidden {
            return 0;
        }
        node.children
            .iter()
            .filter(|c| c.name.starts_with('.'))
            .map(|c| c.size)
            .sum()
    }

    pub fn snapshot(&self) -> Snapshot {
        // If a producer thread panicked mid-write, render the last-known
        // state instead of taking the UI down with it.
        let s = self.state.lock().unwrap_or_else(|e| e.into_inner());
        Snapshot {
            tree: s.tree.clone(),
            files_scanned: s.files_scanned(),
            bytes_scanned: s.bytes_scanned(),
            unreadable: s.unreadable(),
            current_path: s.current_path.clone(),
            done: s.done,
            started_at: s.started_at,
            finished_at: s.finished_at,
        }
    }

    /// Index of `selected_name` within a slice of visible children.
    /// Returns `None` if nothing is selected or the selection isn't visible.
    pub fn selected_index(&self, visible: &[&Arc<Node>]) -> Option<usize> {
        let name = self.selected_name.as_ref()?;
        visible.iter().position(|c| &c.name == name)
    }

    pub fn handle_key(&mut self, key: KeyCode) -> Action {
        if matches!(key, KeyCode::Char('q')) {
            return Action::Exit;
        }
        match key {
            // Esc no longer exits at root — only `q` does. At root, Esc is
            // simply a no-op (zoom_out pops nothing).
            KeyCode::Esc | KeyCode::Backspace => self.zoom_out(),
            KeyCode::Enter => self.zoom_in(),
            KeyCode::Left | KeyCode::Char('h') => self.move_dir(Dir::Left),
            KeyCode::Right | KeyCode::Char('l') => self.move_dir(Dir::Right),
            KeyCode::Up | KeyCode::Char('k') => self.move_dir(Dir::Up),
            KeyCode::Down | KeyCode::Char('j') => self.move_dir(Dir::Down),
            KeyCode::Tab => self.cycle(1),
            KeyCode::BackTab => self.cycle(-1),
            KeyCode::Char('v') => self.toggle_view(),
            KeyCode::Char('s') => self.scale_mode = self.scale_mode.next(),
            KeyCode::Char('H') => self.toggle_hidden(),
            _ => {}
        }
        Action::Continue
    }

    fn toggle_hidden(&mut self) {
        self.show_hidden = !self.show_hidden;
        // If the current selection is no longer visible, snap to the first
        // visible child so the user doesn't end up with an invisible cursor.
        let snap = self.snapshot();
        let visible_names: Vec<String> = snap
            .resolve(&self.path_names)
            .children
            .iter()
            .filter(|c| self.show_hidden || !c.name.starts_with('.'))
            .map(|c| c.name.clone())
            .collect();
        let still_visible = self
            .selected_name
            .as_ref()
            .map(|s| visible_names.iter().any(|n| n == s))
            .unwrap_or(false);
        if !still_visible {
            self.selected_name = visible_names.first().cloned();
        }
        self.list_offset = 0;
    }

    fn toggle_view(&mut self) {
        self.view = match self.view {
            ViewMode::Tile => ViewMode::List,
            ViewMode::List => ViewMode::Tile,
        };
    }

    fn zoom_in(&mut self) {
        let sel = match &self.selected_name {
            Some(n) => n.clone(),
            None => return,
        };
        let snap = self.snapshot();
        let cur = snap.resolve(&self.path_names);
        let Some(child) = cur.children.iter().find(|c| c.name == sel) else {
            return;
        };
        if !child.is_dir {
            return;
        }
        // Pick the first *visible* child for the new selection so we don't
        // land on something the user has hidden.
        let inner_visible = self.visible_children(child);
        let first_inner = inner_visible.first().map(|c| c.name.clone());
        self.path_names.push(sel);
        self.selected_name = first_inner;
        self.list_offset = 0;
    }

    fn zoom_out(&mut self) {
        if let Some(name) = self.path_names.pop() {
            self.selected_name = Some(name);
            self.list_offset = 0;
        }
    }

    fn cycle(&mut self, delta: i32) {
        let snap = self.snapshot();
        let cur = snap.resolve(&self.path_names);
        let visible = self.visible_children(cur);
        let n = visible.len();
        if n == 0 {
            return;
        }
        let cur_idx = self.selected_index(&visible).unwrap_or(0);
        let new = ((cur_idx as i32 + delta).rem_euclid(n as i32)) as usize;
        self.selected_name = Some(visible[new].name.clone());
    }

    fn move_dir(&mut self, dir: Dir) {
        match self.view {
            ViewMode::Tile => self.move_dir_spatial(dir),
            ViewMode::List => self.move_dir_list(dir),
        }
    }

    fn move_dir_list(&mut self, dir: Dir) {
        match dir {
            Dir::Up => self.cycle(-1),
            Dir::Down => self.cycle(1),
            _ => {}
        }
    }

    /// Spatial navigation. See `pick_neighbor` for the scoring rules.
    fn move_dir_spatial(&mut self, dir: Dir) {
        let Some(idx) = self.last_selected_idx else {
            return;
        };
        if idx >= self.last_layout.len() || idx >= self.last_order.len() {
            return;
        }
        if let Some(i) = pick_neighbor(&self.last_layout, idx, dir) {
            self.selected_name = Some(self.last_order[i].clone());
        }
    }

    /// Absolute path of the currently-zoomed directory.
    pub fn current_path(&self, snap: &Snapshot) -> PathBuf {
        snap.resolve(&self.path_names).path.clone()
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub(crate) enum Dir {
    Left,
    Right,
    Up,
    Down,
}

/// Pick the neighbour cell in `layout` nearest to `layout[idx]` in `dir`.
/// Candidates must lie strictly to that side (`r.right <= cur.left` for Left,
/// etc.). Among candidates we sort by, in order:
///   1. has STRICT perpendicular-axis overlap with the current cell
///      (touching at a single boundary does NOT count),
///   2. nearest along the primary axis,
///   3. uppermost (Left/Right) or leftmost (Up/Down) as a tiebreaker.
///
/// Strict overlap is what keeps diagonals from beating the inline
/// neighbour: in a perfectly-tiled treemap every diagonal cell touches
/// the current cell at exactly one boundary.
pub(crate) fn pick_neighbor(layout: &[Rect], idx: usize, dir: Dir) -> Option<usize> {
    if idx >= layout.len() {
        return None;
    }
    let cur = layout[idx];
    let cur_l = cur.x as i32;
    let cur_r = cur_l + cur.width as i32;
    let cur_t = cur.y as i32;
    let cur_b = cur_t + cur.height as i32;

    let mut best: Option<(usize, (i32, i32, i32))> = None;
    for (i, r) in layout.iter().enumerate() {
        if i == idx || r.width == 0 || r.height == 0 {
            continue;
        }
        let cl = r.x as i32;
        let cr = cl + r.width as i32;
        let ct = r.y as i32;
        let cb = ct + r.height as i32;

        let key = match dir {
            Dir::Left => {
                if cr > cur_l {
                    continue;
                }
                let overlap = cur_t < cb && ct < cur_b;
                (i32::from(!overlap), cur_l - cr, ct)
            }
            Dir::Right => {
                if cl < cur_r {
                    continue;
                }
                let overlap = cur_t < cb && ct < cur_b;
                (i32::from(!overlap), cl - cur_r, ct)
            }
            Dir::Up => {
                if cb > cur_t {
                    continue;
                }
                let overlap = cur_l < cr && cl < cur_r;
                (i32::from(!overlap), cur_t - cb, cl)
            }
            Dir::Down => {
                if ct < cur_b {
                    continue;
                }
                let overlap = cur_l < cr && cl < cur_r;
                (i32::from(!overlap), ct - cur_b, cl)
            }
        };

        if best.is_none_or(|(_, b)| key < b) {
            best = Some((i, key));
        }
    }
    best.map(|(i, _)| i)
}

pub enum Action {
    Continue,
    Exit,
}

#[cfg(test)]
mod tests {
    use super::*;

    // 2x2 grid:
    //   A B
    //   C D
    fn grid_2x2() -> Vec<Rect> {
        vec![
            Rect {
                x: 0,
                y: 0,
                width: 10,
                height: 5,
            }, // 0 = A
            Rect {
                x: 10,
                y: 0,
                width: 10,
                height: 5,
            }, // 1 = B
            Rect {
                x: 0,
                y: 5,
                width: 10,
                height: 5,
            }, // 2 = C
            Rect {
                x: 10,
                y: 5,
                width: 10,
                height: 5,
            }, // 3 = D
        ]
    }

    #[test]
    fn spatial_nav_2x2_cardinals() {
        let g = grid_2x2();
        // From A: right -> B, down -> C, up/left -> none.
        assert_eq!(pick_neighbor(&g, 0, Dir::Right), Some(1));
        assert_eq!(pick_neighbor(&g, 0, Dir::Down), Some(2));
        assert_eq!(pick_neighbor(&g, 0, Dir::Up), None);
        assert_eq!(pick_neighbor(&g, 0, Dir::Left), None);

        // From D: left -> C, up -> B.
        assert_eq!(pick_neighbor(&g, 3, Dir::Left), Some(2));
        assert_eq!(pick_neighbor(&g, 3, Dir::Up), Some(1));
    }

    #[test]
    fn spatial_nav_prefers_overlap_over_diagonal() {
        // Layout:
        //   A B
        //   C D
        // From A (0) going Right, both B (inline) and D (diagonal) lie
        // strictly to the right. The overlap-flag tiebreak must pick B.
        let g = grid_2x2();
        assert_eq!(pick_neighbor(&g, 0, Dir::Right), Some(1));
        assert_eq!(pick_neighbor(&g, 0, Dir::Down), Some(2));
    }

    #[test]
    fn spatial_nav_oob_idx_returns_none() {
        let g = grid_2x2();
        assert_eq!(pick_neighbor(&g, 99, Dir::Left), None);
    }
}
