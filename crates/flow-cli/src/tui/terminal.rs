//! ターミナル設定と復元

use crossterm::{
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{Terminal, backend::CrosstermBackend};
use std::io::{self, Stdout, stdout};

pub type Tui = Terminal<CrosstermBackend<Stdout>>;

/// ターミナルをTUIモードにセットアップ
pub fn setup_terminal() -> io::Result<Tui> {
    enable_raw_mode()?;
    let mut stdout = stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    Terminal::new(backend)
}

/// ターミナルを通常モードに復元
pub fn restore_terminal(terminal: &mut Tui) -> io::Result<()> {
    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;
    Ok(())
}
