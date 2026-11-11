mod api;
mod app;
mod config;
mod mpv;
mod ui;
mod ytdlp;

use std::io;
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

    // enable_raw_mode отключает канонический ввод (Enter), чтобы читать каждое нажатие.
    // EnterAlternateScreen создает "второй экран" — после выхода вернётся исходный.
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let mut app = app::App::new();
    let result = app.run(&mut terminal);

    // Восстановление терминала обязательно даже при ошибке app.run.
    // Если этого не сделать, пользователь останется в "сломанном" терминале.
    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen, DisableMouseCapture)?;
    terminal.show_cursor()?;

    // Выводим ошибку app.run (если была) уже после восстановления терминала.
    if let Err(e) = result {
        eprintln!("Ошибка приложения: {e}");
        std::process::exit(1);
    }
    Ok(())
}