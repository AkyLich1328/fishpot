// =====================================================================
// Точка входа
// =====================================================================

mod api;
mod app;
mod config;
mod mpv;
mod viz;
mod ui;
mod ytdlp;

use std::io;
use std::path::PathBuf; 
use crossterm::{
    event::{DisableMouseCapture, EnableMouseCapture},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{backend::CrosstermBackend, Terminal};

fn main() -> io::Result<()> {
    // Проверка внешних зависимостей до входа в alt-screen,
    // чтобы сообщение об ошибке было видно в обычном терминале.
    for cmd in &["mpv", "yt-dlp", "ffmpeg"] {
        if std::process::Command::new(cmd).arg("--version").output().is_err() {
            eprintln!("Ошибка: утилита '{}' не найдена. Установите: sudo pacman -S {}", cmd, cmd);
            std::process::exit(1);
        }
    }

    // Собираем аргументы командной строки (файлы или папки)
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut initial_files = Vec::new();
    for arg in args {
        let p = PathBuf::from(arg);
        if p.exists() {
            initial_files.push(p);
        }
    }

    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let mut app = app::App::new(initial_files); 
    let result = app.run(&mut terminal);

    // Восстановление терминала
    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen, DisableMouseCapture)?;
    terminal.show_cursor()?;

    if let Err(e) = result {
        eprintln!("Ошибка приложения: {e}");
        std::process::exit(1);
    }
    Ok(())
}