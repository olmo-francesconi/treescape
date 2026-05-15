mod app;
mod ui;

use anyhow::{Context, Result};
use clap::Parser;
use crossterm::{
    event::{self, Event},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{backend::CrosstermBackend, Terminal};
use std::{io, path::PathBuf, time::Duration};
use treescape_core::scan;

#[derive(Parser)]
#[command(
    name = "treescape",
    about = "Treemap disk-usage explorer for the terminal"
)]
struct Cli {
    /// Root path to scan
    #[arg(default_value = ".")]
    path: PathBuf,

    /// Show hidden files (dotfiles) from the start. They're always scanned
    /// so parent totals stay accurate; this just controls the initial view
    /// state. Press `H` at runtime to toggle.
    #[arg(long)]
    hidden: bool,
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let root_path = cli
        .path
        .canonicalize()
        .with_context(|| format!("cannot canonicalize {}", cli.path.display()))?;

    // Detect the terminal theme BEFORE entering raw mode — termbg owns
    // stdin briefly while it queries OSC 11.
    let theme = detect_theme();

    let mut terminal = setup_terminal()?;
    let state = scan::start_scan(root_path);
    let result = run(&mut terminal, state, theme, cli.hidden);
    restore_terminal(&mut terminal)?;
    result
}

fn detect_theme() -> ui::Theme {
    match termbg::theme(Duration::from_millis(120)) {
        Ok(termbg::Theme::Light) => ui::Theme::Light,
        Ok(termbg::Theme::Dark) => ui::Theme::Dark,
        Err(_) => ui::Theme::Dark,
    }
}

fn setup_terminal() -> Result<Terminal<CrosstermBackend<io::Stdout>>> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let mut terminal = Terminal::new(CrosstermBackend::new(stdout))?;
    terminal.clear()?;
    Ok(terminal)
}

fn restore_terminal(terminal: &mut Terminal<CrosstermBackend<io::Stdout>>) -> Result<()> {
    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;
    Ok(())
}

fn run(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    state: std::sync::Arc<std::sync::Mutex<treescape_core::ScanShared>>,
    theme: ui::Theme,
    show_hidden: bool,
) -> Result<()> {
    let mut app = app::App::new(state, theme, show_hidden);
    loop {
        terminal.draw(|f| ui::draw(f, &mut app))?;
        if event::poll(Duration::from_millis(100))? {
            if let Event::Key(key) = event::read()? {
                match app.handle_key(key.code) {
                    app::Action::Exit => break,
                    app::Action::Continue => {}
                }
            }
        }
    }
    Ok(())
}
