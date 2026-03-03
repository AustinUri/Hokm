// hokm_client/src/tui/mod.rs
//
// Owns terminal setup + event loop.
// Calls ui::draw() every frame.

pub mod app;
pub mod ui;

use std::{io, time::Duration};

use crossterm::{
    event::{self, Event, KeyCode, KeyEventKind},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};

use ratatui::{backend::CrosstermBackend, Terminal};

use app::App;

pub fn run() -> io::Result<()> {
    // Raw mode -> keypresses are instant
    enable_raw_mode()?;

    // Alternate screen -> full-screen UI
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;

    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // App state
    let mut app = App::new();

    // ~30 FPS
    let tick_rate = Duration::from_millis(33);

    loop {
        // NOTE: app must be mutable because UI uses stateful widgets (ListState)
        terminal.draw(|f| ui::draw(f, &mut app))?;

        if event::poll(tick_rate)? {
            match event::read()? {
                Event::Key(key) if key.kind == KeyEventKind::Press => {
                    if handle_key(&mut app, key.code) {
                        break;
                    }
                }
                _ => {}
            }
        }

        app.on_tick();
    }

    // Restore terminal
    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;

    Ok(())
}

/// Return true = quit
fn handle_key(app: &mut App, code: KeyCode) -> bool {
    match code {
        KeyCode::Char('q') => return true,

        KeyCode::Down | KeyCode::Char('j') => app.hand_cursor_down(),
        KeyCode::Up | KeyCode::Char('k') => app.hand_cursor_up(),

        KeyCode::Enter => app.try_play_selected_card(),

        KeyCode::Char('r') => app.reset_match(),

        _ => {}
    }
    false
}
