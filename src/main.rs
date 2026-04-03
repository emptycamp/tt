use std::io;

use clap::Parser;
use crossterm::{
    execute,
    terminal::{
        disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen, SetTitle,
    },
};
use ratatui::{backend::CrosstermBackend, Terminal};

use tt::app::App;
use tt::cli::{confirm_clear, CliAction, Cli};
use tt::store;

fn main() {
    let cli = Cli::parse();
    store::set_test_mode(cli.test);

    let mut app = match cli.action() {
        CliAction::NewTimer(secs, name) => App::with_timer(secs, name),
        CliAction::DurationOnly(secs) => App::with_duration_prompt(secs),
        CliAction::NameOnly(name) => App::with_name_prompt(name),
        CliAction::Resume => App::new(),
        CliAction::Clear => {
            if confirm_clear(cli.test) {
                store::clear();
                println!("Timer data cleared.");
            } else {
                println!("Cancelled.");
            }
            return;
        }
    };

    if let Err(e) = run_tui(&mut app) {
        eprintln!("Error: {e}");
        std::process::exit(1);
    }
}

fn run_tui(app: &mut App) -> io::Result<()> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, SetTitle("tt"))?;

    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // Install panic hook to restore terminal on crash
    let original_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let _ = disable_raw_mode();
        let _ = execute!(io::stdout(), LeaveAlternateScreen);
        original_hook(info);
    }));

    app.run(&mut terminal);

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    Ok(())
}
