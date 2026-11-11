use ratatui::{
    layout::{Constraint, Layout, Rect}, 
    style::{Color, Modifier, Style}, 
    text::{Line, Span},
    widgets::{Block, Clear, Gauge, List, ListItem, Paragraph, ListState, BorderType, Padding}, 
    Frame,
};
use crate::app::{App, Focus, Mode, LocalItem};
use crate::config::parse_hex;

/// Верхняя строка статуса: громкость, режим повтора, прогресс скачивания, статус, пауза, играющий трек.
fn render_status_bar(f: &mut Frame, app: &App, area: Rect) {
    let bg_color = parse_hex(&app.config.theme.bg_color);
    let inactive_border = parse_hex(&app.config.theme.inactive_border);
    let active_border = parse_hex(&app.config.theme.active_border);

    let now = app.current.as_ref().map(|t| t.title.clone()).unwrap_or_else(|| "-".into());
    let paused_icon = if app.paused { "⏸" } else { "▶" };
    let repeat_icon = match app.repeat_mode {
        crate::app::RepeatMode::None => "🔁 Off",
        crate::app::RepeatMode::All => "🔁 All",
        crate::app::RepeatMode::One => "🔂 One",
    };
    
    // Показываем "⬇ X%" только когда прогресс реально известен.
    // Если скачивание идёт, но прогресс ещё не пришёл (стрим, неизвестный размер) — "⬇ ..."
    let download_str = match app.download_percent {
        Some(pct) if pct > 0.0 => format!(" ⬇ {:.1}% ", pct),
        Some(_) => " ⬇ ... ".to_string(),
        None => "".to_string(),
    };
    
    // Если идёт скачивание, но download_percent = None (мы только что запустили, 
    // но прогресс ещё не пришёл) — тоже покажем "⬇ ...".
    let download_str = if download_str.is_empty() && app.download_rx.is_some() {
        " ⬇ ... ".to_string()
    } else {
        download_str
    };

    let status_line = Line::from(vec![
        Span::styled(format!(" Vol {}% ", app.volume), Style::default().fg(Color::Yellow)),
        Span::styled("│ ", Style::default().fg(inactive_border)),
        Span::styled(format!(" {} ", repeat_icon), Style::default().fg(Color::Magenta)),
        Span::styled("│ ", Style::default().fg(inactive_border)),
        Span::styled(download_str, Style::default().fg(Color::LightGreen)),
        Span::styled(format!(" {} ", &app.status), Style::default().fg(active_border)),
        Span::styled("│ ", Style::default().fg(inactive_border)),
        Span::styled(format!(" {} ", paused_icon), Style::default().fg(Color::Green)),
        Span::styled(now, Style::default().fg(Color::White).add_modifier(Modifier::BOLD)),
    ]);
    f.render_widget(Paragraph::new(status_line).style(Style::default().bg(bg_color)), area);
}
pub fn draw(f: &mut Frame, app: &App) {
    // Заливаем весь экран фоновым цветом из темы — так интерфейс выглядит цельным.
    let bg_color = parse_hex(&app.config.theme.bg_color);
    f.render_widget(Block::default().style(Style::default().bg(bg_color)), f.area());

    // Полноэкранный режим — только визуализатор и строка статуса.
    if app.fullscreen_viz {
        let chunks = Layout::vertical([
            Constraint::Min(1),
            Constraint::Length(1),
        ]).split(f.area());

        render_visualizer(f, app, chunks[0]);
        render_status_bar(f, app, chunks[1]);
        
        if app.show_eq { render_eq(f, app); }
        if app.show_help { render_help(f, app); }
        if app.show_lyrics { render_lyrics(f, app); }
        if app.confirm_delete.is_some() { render_confirm_delete(f, app); }
        return;
    }

    // Обычный режим: поиск, основная зона, прогресс-бар, статус.
    let chunks = Layout::vertical([
        Constraint::Length(3),
        Constraint::Min(5),
        Constraint::Length(3),
        Constraint::Length(1),
    ]).split(f.area());

    let _active_border = parse_hex(&app.config.theme.active_border);
    let inactive_border = parse_hex(&app.config.theme.inactive_border);
    let text_color = parse_hex(&app.config.theme.text_color);
    let accent_color = parse_hex(&app.config.theme.accent_color);

    let spinner = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"][app.tick % 10];
    
    // Заголовок строки ввода зависит от режима.
    let (title, style) = match app.mode {
        Mode::Searching => (format!(" {} Поиск (Enter/Esc) ", spinner), Style::default().fg(accent_color)),
        Mode::Seeking => (format!(" {} Перемотка (mm:ss) ", spinner), Style::default().fg(Color::Cyan)),
        Mode::Importing => (format!(" {} URL плейлиста ", spinner), Style::default().fg(Color::Yellow)),
        Mode::CreatingDir => (format!(" {} Имя новой папки ", spinner), Style::default().fg(Color::Green)),
        Mode::FilteringLocal => (format!(" {} Фильтр файлов (Enter/Esc) ", spinner), Style::default().fg(Color::Cyan)),
        Mode::RenamingFile => (format!(" {} Новое имя файла (Enter/Esc) ", spinner), Style::default().fg(Color::Green)),
        Mode::Normal => (" Нажмите '/' для поиска, '?' для справки ".to_string(), Style::default().fg(text_color)),
    };

    // В строке ввода отображаем тот буфер, который относится к текущему режиму.
    let display_text = match app.mode {
        Mode::FilteringLocal => &app.local_filter,
        Mode::RenamingFile => &app.rename_input,
        _ => &app.input,
    };

    let search_block = Block::bordered()
        .title(title)
        .border_type(BorderType::Rounded)
        .border_style(style)
        .padding(Padding::horizontal(1));
    f.render_widget(Paragraph::new(display_text.as_str()).style(Style::default().fg(Color::White)).block(search_block), chunks[0]);

    // Основная зона. Если панель YouTube скрыта (клавиша Y) — визуализатор занимает её место.
    if app.show_results {
        let main_area = Layout::horizontal([
            Constraint::Percentage(25),
            Constraint::Percentage(40),
            Constraint::Percentage(35),
        ]).split(chunks[1]);

        render_local(f, app, main_area[0]);
        render_results(f, app, main_area[1]);
        render_visualizer(f, app, main_area[2]);
    } else {
        let main_area = Layout::horizontal([
            Constraint::Percentage(30),
            Constraint::Percentage(70),
        ]).split(chunks[1]);

        render_local(f, app, main_area[0]);
        render_visualizer(f, app, main_area[1]);
    }

    // Прогресс-бар с текущим временем и длительностью.
    let time_str = format!(
        " {:02}:{:02} / {:02}:{:02} ", 
        (app.time_pos / 60.0) as u64, (app.time_pos % 60.0) as u64, 
        (app.duration / 60.0) as u64, (app.duration % 60.0) as u64
    );
    let ratio = if app.duration > 0.0 { (app.time_pos / app.duration).min(1.0) } else { 0.0 };
    let progress_block = Block::bordered()
        .title(time_str)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(inactive_border));
    f.render_widget(
        Gauge::default()
            .block(progress_block)
            .gauge_style(Style::default().fg(accent_color))
            .ratio(ratio), 
        chunks[2]
    );

    render_status_bar(f, app, chunks[3]);

    // Всплывающие окна рисуются последними — они перекрывают всё остальное.
    if app.show_help { render_help(f, app); }
    if app.show_lyrics { render_lyrics(f, app); }
    if app.show_eq { render_eq(f, app); }
    if app.confirm_delete.is_some() { render_confirm_delete(f, app); }
}

/// Левая панель результатов поиска YouTube.
fn render_results(f: &mut Frame, app: &App, area: Rect) {
    let is_active = app.focus == Focus::Results;
    let active_border = parse_hex(&app.config.theme.active_border);
    let inactive_border = parse_hex(&app.config.theme.inactive_border);
    let text_color = parse_hex(&app.config.theme.text_color);
    let selected_bg = parse_hex(&app.config.theme.selected_bg);

    let items: Vec<ListItem> = app.results.iter().enumerate().map(|(i, t)| {
        let is_playing = app.current.as_ref().map(|c| c.id == t.id).unwrap_or(false);
        let mark = if t.local_path.is_some() { "💾 " } else { "" };
        let play_icon = if is_playing { "▶ " } else { "" };
        let text = format!("{:>3}. {}{}{}", i + 1, play_icon, mark, t.title);
        
        let style = if i == app.selected { 
            Style::default().bg(selected_bg).fg(Color::White)
        } else if is_playing {
            Style::default().fg(Color::LightGreen)
        } else { 
            Style::default().fg(text_color) 
        };
        ListItem::new(text).style(style)
    }).collect();
    
    let mut state = ListState::default();
    if !items.is_empty() { state.select(Some(app.selected)); }
    let block = Block::bordered()
        .title(" YouTube ")
        .title_alignment(ratatui::layout::Alignment::Center)
        .border_type(BorderType::Rounded)
        .border_style(if is_active { Style::default().fg(active_border) } else { Style::default().fg(inactive_border) })
        .padding(Padding::horizontal(1));
    f.render_stateful_widget(List::new(items).block(block), area, &mut state);
}

/// Правая (или левая) панель локальной библиотеки.
fn render_local(f: &mut Frame, app: &App, area: Rect) {
    let is_active = app.focus == Focus::Local;
    let active_border = parse_hex(&app.config.theme.active_border);
    let inactive_border = parse_hex(&app.config.theme.inactive_border);
    let selected_bg = parse_hex(&app.config.theme.selected_bg);

    let items: Vec<ListItem> = app.local_items.iter().enumerate().map(|(i, item)| {
        let (mut text, color) = match item {
            LocalItem::Dir(p) => (format!("📁 {}", p.file_name().unwrap_or_default().to_string_lossy()), Color::Cyan),
            LocalItem::File(t) => (format!("🎵 {}", t.title), Color::White),
        };
        
        let mut is_playing = false;
        if let LocalItem::File(t) = item {
            if let Some(c) = &app.current {
                if c.id == t.id {
                    is_playing = true;
                    text = format!("▶ {}", text);
                }
            }
        }

        let style = if i == app.local_selected { 
            Style::default().bg(selected_bg).fg(Color::White) 
        } else if is_playing {
            Style::default().fg(Color::LightGreen)
        } else { 
            Style::default().fg(color) 
        };
        ListItem::new(text).style(style)
    }).collect();
    
    // В заголовке показываем только последние 15 символов пути, чтобы влезло.
    let path_str = app.current_dir.to_string_lossy().to_string();
    let title = format!(" 📁 ...{} ", if path_str.len() > 15 { &path_str[path_str.len()-15..] } else { &path_str });
    
    let mut state = ListState::default();
    if !app.local_items.is_empty() { state.select(Some(app.local_selected)); }
    let block = Block::bordered()
        .title(title)
        .title_alignment(ratatui::layout::Alignment::Center)
        .border_type(BorderType::Rounded)
        .border_style(if is_active { Style::default().fg(active_border) } else { Style::default().fg(inactive_border) })
        .padding(Padding::horizontal(1));
    f.render_stateful_widget(List::new(items).block(block), area, &mut state);
}

/// Окно подтверждения удаления файла/папки.
fn render_confirm_delete(f: &mut Frame, app: &App) {
    let area = centered_rect(40, 20, f.area());
    f.render_widget(Clear, area);
    let accent_color = parse_hex(&app.config.theme.accent_color);
    
    let block = Block::bordered()
        .title(" Подтверждение ")
        .title_alignment(ratatui::layout::Alignment::Center)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(accent_color))
        .style(Style::default().bg(Color::Rgb(15, 15, 20)));
        
    let text = vec![
        Line::from("Вы уверены, что хотите удалить этот элемент?"), 
        Line::from(""),
        Line::from(Span::styled("y - Да  |  n/Esc - Нет", Style::default().fg(Color::Yellow))),
    ];
    f.render_widget(
        Paragraph::new(text)
            .block(block)
            .alignment(ratatui::layout::Alignment::Center), 
        area
    );
}

/// Рендеринг визуализатора. Делегирует работу конкретному режиму.
fn render_visualizer(f: &mut Frame, app: &App, area: Rect) {
    let active_border = parse_hex(&app.config.theme.active_border);
    let title = match app.viz_mode {
        0 => " 3D Сфера ",
        1 => " Радиальные волны ",
        2 => " Туннель ",
        3 => " Матрица ",
        4 => " RGB Череп ",
        5 => " ДНК ",
        6 => " Аквариум ",
        7 => " Ночной поезд ",
        _ => " Визуализатор ",
    };
    
    let block = Block::bordered()
        .title(title)
        .title_alignment(ratatui::layout::Alignment::Center)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(active_border))
        .style(Style::default().bg(Color::Rgb(10, 10, 15)));
        
    let inner = block.inner(area);
    f.render_widget(block, area);
    
    // cava присылает 32 байта — по одному на каждую полосу частот.
    let data = app.audio_data.lock().unwrap();
    let freqs: Vec<f64> = if data.len() == 32 {
        data.iter().map(|&x| x as f64 / 255.0).collect()
    } else {
        // Синтетическая симуляция, если cava не установлена.
        (0..32).map(|i| {
            let t = app.viz_time;
            let f = i as f64;
            ((t * 5.0 + f * 0.3).sin() * 0.5 + 0.5) * ((t * 2.0 + f * 0.1).cos() * 0.5 + 0.5)
        }).collect()
    };

    let bass: f64 = freqs[0..8].iter().sum::<f64>() / 8.0;
    let mid: f64 = freqs[8..20].iter().sum::<f64>() / 12.0;
    let treble: f64 = freqs[20..32].iter().sum::<f64>() / 12.0;

    // Режимы, которые рисуют напрямую в Frame (не используют canvas Брайля).
    if app.viz_mode == 7 { render_train(f, inner, app.viz_time, bass, mid, treble); return; }
    if app.viz_mode == 6 { render_aquarium(f, inner, app.viz_time, bass, treble); return; }
    if app.viz_mode == 3 { render_matrix_rain(f, inner, app.viz_time, bass, treble); return; }
    if app.viz_mode == 4 { render_skull(f, inner, app.viz_time, bass, treble); return; }

    // Режимы, использующие canvas Брайля: каждая точка — бит в символе Брайля (2x4).
    let w = inner.width as usize * 2; 
    let h = inner.height as usize * 4;
    let mut canvas = vec![(0u8, Color::Reset); w * h];

    match app.viz_mode {
        0 => draw_sphere(&mut canvas, w, h, app.viz_time, bass, mid, treble),
        1 => draw_radial(&mut canvas, w, h, app.viz_time, &freqs),
        2 => draw_tunnel(&mut canvas, w, h, app.viz_time, &freqs, bass),
        5 => draw_dna(&mut canvas, w, h, app.viz_time, bass, mid, treble),
        _ => {}
    }

    // Конвертируем попиксельный canvas в символы Брайля.
    let mut lines = Vec::new();
    for y in 0..inner.height {
        let mut spans = Vec::new();
        for x in 0..inner.width {
            let mut code = 0x2800;
            let mut col = Color::Reset; 
            let mut has = false;
            for dy in 0..4 { 
                for dx in 0..2 {
                    let idx = (x as usize*2+dx) + (y as usize*4+dy)*w;
                    if canvas[idx].0 > 0 { 
                        code |= braille(dx as u8, dy as u8); 
                        col = canvas[idx].1; 
                        has = true; 
                    }
                }
            }
            spans.push(if has { 
                Span::styled(char::from_u32(code).unwrap_or(' ').to_string(), Style::default().fg(col)) 
            } else { 
                Span::raw(" ") 
            });
        }
        lines.push(Line::from(spans));
    }
    f.render_widget(Paragraph::new(lines), inner);
}

/// Окно с текстом песни (караоке). Автоматически скроллит к текущей строке.
fn render_lyrics(f: &mut Frame, app: &App) {
    let accent_color = parse_hex(&app.config.theme.accent_color);
    let area = centered_rect(60, 80, f.area());
    f.render_widget(Clear, area); 
    
    let block = Block::bordered()
        .title(" Текст песни (L/Esc - закрыть, j/k - скролл) ")
        .title_alignment(ratatui::layout::Alignment::Center)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(accent_color))
        .style(Style::default().bg(Color::Rgb(15, 15, 20))); 
        
    let inner = block.inner(area);
    f.render_widget(block, area);

    if app.lyrics_rx.is_some() { 
        f.render_widget(Paragraph::new("Загрузка...").alignment(ratatui::layout::Alignment::Center), inner); 
        return; 
    }

    if let Some(lyrics) = &app.lyrics_data {
        // Ищем текущую строчку: последнюю, у которой time <= текущего времени трека.
        let mut current_idx = 0;
        for (i, (time, _)) in lyrics.iter().enumerate() {
            if *time >= 0.0 && *time <= app.time_pos + 0.15 { current_idx = i; }
            else if *time > app.time_pos + 0.15 { break; }
        }
        
        // Если пользователь вручную скроллил — используем его позицию. Иначе авто-центрирование.
        let scroll = if let Some(s) = app.lyrics_scroll { 
            s 
        } else { 
            (current_idx.saturating_sub(inner.height as usize / 2)) as u16 
        };
        
        let mut lines = Vec::new();
        for (i, (_, text)) in lyrics.iter().enumerate() {
            if text.is_empty() { continue; }
            if i == current_idx && lyrics[i].0 >= 0.0 {
                lines.push(Line::from(Span::styled(text, Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD))));
            } else {
                lines.push(Line::from(Span::styled(text, Style::default().fg(Color::Gray))));
            }
        }
        f.render_widget(
            Paragraph::new(lines)
                .scroll((scroll, 0))
                .alignment(ratatui::layout::Alignment::Center), 
            inner
        );
    } else { 
        f.render_widget(Paragraph::new("Текст не найден").alignment(ratatui::layout::Alignment::Center), inner); 
    }
}

/// Окно справки. Разбито на логические блоки.
fn render_help(f: &mut Frame, app: &App) {
    let accent_color = parse_hex(&app.config.theme.accent_color);
    let active_border = parse_hex(&app.config.theme.active_border);
    
    let area = centered_rect(75, 85, f.area()); 
    f.render_widget(Clear, area);
    
    let key_style = Style::default().fg(active_border).add_modifier(Modifier::BOLD);
    let desc_style = Style::default().fg(Color::Gray);

    let text = vec![
        Line::from(Span::styled("🎵 ОБЩИЕ КОМАНДЫ", Style::default().fg(accent_color).add_modifier(Modifier::BOLD))),
        Line::from(vec![Span::styled("/ ", key_style), Span::styled("Поиск музыки на YouTube", desc_style)]),
        Line::from(vec![Span::styled("i ", key_style), Span::styled("Скачать плейлист или трек по ссылке", desc_style)]),
        Line::from(vec![Span::styled("Y ", key_style), Span::styled("Скрыть/Показать панель YouTube", desc_style)]),
        Line::from(vec![Span::styled("F ", key_style), Span::styled("Полноэкранный визуализатор", desc_style)]),
        Line::from(vec![Span::styled("E ", key_style), Span::styled("Открыть эквалайзер", desc_style)]),
        Line::from(vec![Span::styled("L ", key_style), Span::styled("Показать текст песни (караоке)", desc_style)]),
        Line::from(vec![Span::styled("V ", key_style), Span::styled("Сменить визуализацию (8 режимов)", desc_style)]),
              Line::from(vec![Span::styled("D ", key_style), Span::styled("Скачивание аудиофайла", desc_style)]),
        Line::from(vec![Span::styled("q ", key_style), Span::styled("Выход", desc_style)]),
        Line::from(""),
        
        Line::from(Span::styled("▶️ ВОСПРОИЗВЕДЕНИЕ", Style::default().fg(accent_color).add_modifier(Modifier::BOLD))),
        Line::from(vec![Span::styled("Enter ", key_style), Span::styled("Играть выбранный трек", desc_style)]),
        Line::from(vec![Span::styled("Space / p ", key_style), Span::styled("Пауза / Продолжить", desc_style)]),
        Line::from(vec![Span::styled("h / l ", key_style), Span::styled("Перемотка на 5 секунд назад / вперед", desc_style)]),
        Line::from(vec![Span::styled("+ / - ", key_style), Span::styled("Громкость", desc_style)]),
        Line::from(vec![Span::styled("n ", key_style), Span::styled("Следующий трек", desc_style)]),
        Line::from(vec![Span::styled("R ", key_style), Span::styled("Режим повтора (Выкл -> Все -> Один)", desc_style)]),
        Line::from(vec![Span::styled("r ", key_style), Span::styled("Случайный порядок (Shuffle) [в глобальном режиме]", desc_style)]),
        Line::from(""),

        Line::from(Span::styled("📁 ЛОКАЛЬНАЯ БИБЛИОТЕКА", Style::default().fg(accent_color).add_modifier(Modifier::BOLD))),
        Line::from(vec![Span::styled("Enter ", key_style), Span::styled("Войти в папку / Играть файл", desc_style)]),
        Line::from(vec![Span::styled("Backspace ", key_style), Span::styled("Вернуться на уровень выше", desc_style)]),
        Line::from(vec![Span::styled("f ", key_style), Span::styled("Фильтр файлов по названию", desc_style)]),
        Line::from(vec![Span::styled("r ", key_style), Span::styled("Переименовать выбранный файл", desc_style)]),
        Line::from(vec![Span::styled("d ", key_style), Span::styled("Удалить выбранный файл/папку", desc_style)]),
        Line::from(vec![Span::styled("m ", key_style), Span::styled("Создать новую папку", desc_style)]),
        Line::from(vec![Span::styled("F5 ", key_style), Span::styled("Обновить список файлов", desc_style)]),
        Line::from(""),

        Line::from(Span::styled("⬇️ СКАЧИВАНИЕ", Style::default().fg(accent_color).add_modifier(Modifier::BOLD))),
        Line::from(vec![Span::styled("D ", key_style), Span::styled("Скачать выбранный трек из поиска YouTube", desc_style)]),
        Line::from(""),

        Line::from(Span::styled("Нажмите любую клавишу, чтобы закрыть", Style::default().fg(Color::DarkGray))),
    ];
    
    let block = Block::bordered()
        .title(" Справка ")
        .title_alignment(ratatui::layout::Alignment::Center)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(accent_color))
        .style(Style::default().bg(Color::Rgb(15, 15, 20)))
        .padding(Padding::uniform(1));
        
    f.render_widget(Paragraph::new(text).block(block), area);
}

// ==========================================
// ВИЗУАЛИЗАТОРЫ (без изменений, работают как раньше)
// ==========================================

fn centered_rect(px: u16, py: u16, r: Rect) -> Rect {
    let v = Layout::vertical([
        Constraint::Percentage((100-py)/2), 
        Constraint::Percentage(py), 
        Constraint::Percentage((100-py)/2)
    ]).split(r);
    Layout::horizontal([
        Constraint::Percentage((100-px)/2), 
        Constraint::Percentage(px), 
        Constraint::Percentage((100-px)/2)
    ]).split(v[1])[1]
}

fn draw_dna(canvas: &mut [(u8, Color)], w: usize, h: usize, t: f64, bass: f64, mid: f64, treble: f64) {
    let (cx, cy) = (w as f64 / 2.0, h as f64 / 2.0);
    let radius = 30.0 + bass * 40.0;
    let speed = 2.0 + mid * 4.0;
    let num_pairs = 60;
    let mut prev1: Option<(i32, i32)> = None;
    let mut prev2: Option<(i32, i32)> = None;

    for i in 0..num_pairs {
        let y_ratio = i as f64 / num_pairs as f64;
        let y_pos = (y_ratio - 0.5) * (h as f64) * 1.1;
        let angle = y_ratio * std::f64::consts::PI * 4.0 + t * speed; 
        let x1_3d = radius * angle.cos();
        let z1_3d = radius * angle.sin();
        let x2_3d = radius * (angle + std::f64::consts::PI).cos();
        let z2_3d = radius * (angle + std::f64::consts::PI).sin();
        let fov = 200.0;
        let scale1 = fov / (fov + z1_3d + radius * 2.0);
        let scale2 = fov / (fov + z2_3d + radius * 2.0);
        let px1 = cx + x1_3d * scale1;
        let py1 = cy + y_pos * scale1;
        let px2 = cx + x2_3d * scale2;
        let py2 = cy + y_pos * scale2;
        let flash = treble > 0.6 && i % 4 == 0;
        let col1 = if flash { Color::White } else { Color::Cyan };
        let col2 = if flash { Color::White } else { Color::Magenta };
        draw_line(canvas, w, h, px1 as i32, py1 as i32, px2 as i32, py2 as i32, Color::Rgb(80, 80, 120));
        plot(canvas, w, h, px1 as i32, py1 as i32, col1);
        plot(canvas, w, h, px2 as i32, py2 as i32, col2);
        if let Some((ppx1, ppy1)) = prev1 {
            draw_line(canvas, w, h, ppx1, ppy1, px1 as i32, py1 as i32, Color::Rgb(0, 150, 255));
        }
        if let Some((ppx2, ppy2)) = prev2 {
            draw_line(canvas, w, h, ppx2, ppy2, px2 as i32, py2 as i32, Color::Rgb(255, 0, 150));
        }
        prev1 = Some((px1 as i32, py1 as i32));
        prev2 = Some((px2 as i32, py2 as i32));
    }
}

fn render_aquarium(f: &mut Frame, area: Rect, t: f64, bass: f64, treble: f64) {
    let width = area.width as usize;
    let height = area.height as usize;
    let mut grid = vec![vec![(' ', Color::Reset); width]; height];
    
    for x in 0..width {
        if (x * 7) % 19 == 0 {
            let y_offset = (t * 2.0 + x as f64).sin() * 3.0;
            let max_h = (height as f64 * 0.25 + y_offset).max(0.0) as usize;
            for y in 0..max_h {
                if height > y {
                    let ch = if y % 3 == 0 { '^' } else { '|' };
                    grid[height - 1 - y][x] = (ch, Color::Rgb(0, 150, 50));
                }
            }
        }
    }

    let num_bubbles = 20 + (treble * 60.0) as usize;
    for i in 0..num_bubbles {
        let speed = 1.0 + (i % 5) as f64 * 0.5;
        let start_x = (i * 37) % width;
        let wobble = (t * 3.0 + i as f64 * 1.5).sin() * 2.0;
        let x = ((start_x as f64 + wobble) as i32).rem_euclid(width as i32) as usize;
        let y = height as f64 - ((t * speed * 3.0 + i as f64 * 10.0) % (height as f64 + 10.0));
        if y >= 0.0 && y < height as f64 {
            let ch = if y < height as f64 * 0.3 { 'O' } else { 'o' };
            let col = if treble > 0.6 { Color::White } else { Color::Rgb(150, 200, 255) };
            grid[y as usize][x] = (ch, col);
        }
    }

    let speed_mult = 1.0 + bass * 2.0;
    let fishes = [
        (10.0, 0.2, 3.0, "<º))))><", Color::Rgb(255, 150, 0)),
        (50.0, 0.6, -2.0, "<\\/\\\\<", Color::Rgb(0, 255, 255)),
        (80.0, 0.4, 4.0, "<[XX]<", Color::Rgb(255, 100, 255)),
        (30.0, 0.8, -1.5, "彡(((°<", Color::Rgb(100, 255, 150)),
    ];

    for (i, (start_x, start_y, vx, base_sym, color)) in fishes.iter().enumerate() {
        let travel = t * vx * speed_mult * 2.0;
        let mut x_float = start_x + travel;
        let screen_w = width as f64 + 20.0;
        x_float = x_float.rem_euclid(screen_w) - 10.0; 
        let is_right = *vx > 0.0;
        let sym_str = if is_right { base_sym.to_string() } else { base_sym.chars().rev().collect::<String>() };
        let y_offset = (t * 0.8 + i as f64 * 1.5).sin() * 2.5;
        let y_float = start_y * height as f64 + y_offset;
        let sx = x_float as i32;
        let sy = y_float as i32;
        let is_tail_wag = (t * 8.0).sin() > 0.0;
        for (j, c) in sym_str.chars().enumerate() {
            let px = sx + j as i32;
            if px >= 0 && px < width as i32 && sy >= 0 && sy < height as i32 && c != ' ' {
                let drawn_char = if j == 0 && is_tail_wag {
                    if *vx > 0.0 { '~' } else { '<' } 
                } else { c };
                grid[sy as usize][px as usize] = (drawn_char, *color);
            }
        }
    }

    let mut lines = Vec::new();
    for y in 0..height {
        let mut spans = Vec::new();
        for x in 0..width {
            let (ch, fg) = grid[y][x];
            let wave = (x as f64 * 0.1 + t * 1.5).sin() * 0.5 + 0.5;
            let depth = y as f64 / height as f64;
            let r = (bass * 60.0).min(255.0) as u8;
            let g = (wave * 50.0 + bass * 80.0).min(255.0) as u8;
            let b = (100.0 + (1.0 - depth) * 50.0 + bass * 100.0).min(255.0) as u8;
            let bg_col = Color::Rgb(r, g, b);
            if ch == ' ' {
                spans.push(Span::styled(" ", Style::default().bg(bg_col)));
            } else {
                spans.push(Span::styled(ch.to_string(), Style::default().fg(fg).bg(bg_col)));
            }
        }
        lines.push(Line::from(spans));
    }
    f.render_widget(Paragraph::new(lines), area);
}

fn render_matrix_rain(f: &mut Frame, area: Rect, t: f64, bass: f64, treble: f64) {
    let width = area.width as usize;
    let height = area.height as usize;
    let mut grid = vec![vec![(' ', Color::Reset); width]; height];

    let chars = vec![
        '0', '1', '2', '3', '4', '5', '6', '7', '8', '9',
        'A', 'B', 'C', 'D', 'E', 'F', 'X', 'Y', 'Z',
        '@', '#', '$', '%', '&', '*', '+', '=', '?', '!',
        'ア', 'イ', 'ウ', 'エ', 'オ', 'カ', 'キ', 'ク', 'ケ', 'コ'
    ];

    let speed_mult = 1.0 + bass * 5.0;
    let trail_len = (5.0 + treble * 15.0) as isize;

    for x in 0..width {
        let seed = x as f64 * 1.618;
        let col_speed = (seed.sin() * 0.5 + 0.5) * 2.0 + 1.0;
        let head_y = ((t * speed_mult * col_speed + seed * 10.0) % (height as f64 + 20.0)) as isize;
        let hue = (t * 0.5 + x as f64 * 0.05) % 1.0;
        let r = ((hue * std::f64::consts::PI * 2.0).sin() * 127.0 + 128.0) as u8;
        let g = (((hue + 0.33) * std::f64::consts::PI * 2.0).sin() * 127.0 + 128.0) as u8;
        let b = (((hue + 0.66) * std::f64::consts::PI * 2.0).sin() * 127.0 + 128.0) as u8;

        for y in 0..height {
            let y_isize = y as isize;
            if head_y >= y_isize && head_y - y_isize < trail_len {
                let dist = head_y - y_isize;
                let char_idx = ((x * 17 + y * 31 + (t * 20.0) as usize) % chars.len()) as usize;
                let ch = chars[char_idx];
                let col = if dist == 0 {
                    Color::White
                } else if dist < 3 {
                    Color::Rgb(r, g, b)
                } else {
                    let fade = 1.0 - (dist as f64 / trail_len as f64);
                    Color::Rgb(
                        (r as f64 * fade * 0.5) as u8, 
                        (g as f64 * fade * 0.5) as u8, 
                        (b as f64 * fade * 0.5) as u8
                    )
                };
                grid[y][x] = (ch, col);
            }
        }
    }

    let mut lines = Vec::new();
    for y in 0..height {
        let mut spans = Vec::new();
        for x in 0..width {
            let (ch, col) = grid[y][x];
            spans.push(Span::styled(ch.to_string(), Style::default().fg(col)));
        }
        lines.push(Line::from(spans));
    }
    f.render_widget(Paragraph::new(lines), area);
}

fn render_skull(f: &mut Frame, area: Rect, t: f64, bass: f64, treble: f64) {
    let width = area.width as usize;
    let height = area.height as usize;

    let raw_mask = vec![
        r#"       .-"      "-.       "#,
        r#"      /            \      "#,
        r#"     |              |     "#,
        r#"     |,  .-.  .-.  ,|     "#,
        r#"     | )(__/  \__)( |     "#,
        r#"     |/     /\     \|     "#,
        r#"     (_     ^^     _)     "#,
        r#"      \__|IIIIII|__/      "#,
        r#"       | \IIIIII/ |       "#,
        r#"       \          /       "#,
        r#"      `--------`          "#,
    ];

    let mask_h = raw_mask.len();
    let mask_w = raw_mask[0].len();
    let scale_y = (height as f64 / mask_h as f64).floor() as usize;
    let scale_x = (width as f64 / mask_w as f64).floor() as usize;
    let scale = scale_y.min(scale_x).max(1);
    let scaled_w = mask_w * scale;
    let scaled_h = mask_h * scale;
    let start_x = (width as i32 - scaled_w as i32) / 2;
    let start_y = (height as i32 - scaled_h as i32) / 2;
    let chars = ['0', '1', 'X', 'Z', '#', '$', '%', '&', '@', 'ア', 'カ', 'サ', '█', '▓'];

    let mut lines = Vec::new();
    for y in 0..height {
        let mut spans = Vec::new();
        for x in 0..width {
            let mx = (x as i32 - start_x) / scale as i32;
            let my = (y as i32 - start_y) / scale as i32;

            if mx >= 0 && mx < mask_w as i32 && my >= 0 && my < mask_h as i32 {
                let mask_char = raw_mask[my as usize].chars().nth(mx as usize).unwrap_or(' ');
                if mask_char == ' ' {
                    let rain_y = ((t * 10.0 + x as f64 * 3.5) % height as f64) as usize;
                    if y == rain_y {
                        let char_idx = ((x * 17 + y * 31 + (t * 20.0) as usize) % chars.len()) as usize;
                        spans.push(Span::styled(chars[char_idx].to_string(), Style::default().fg(Color::Rgb(0, 60, 0))));
                    } else {
                        spans.push(Span::raw(" "));
                    }
                } else if mask_char == 'I' {
                    let col = if treble > 0.5 { Color::White } else { Color::Cyan };
                    spans.push(Span::styled("I".to_string(), Style::default().fg(col)));
                } else if mask_char == '^' || mask_char == '_' {
                    let col = if bass > 0.3 { Color::Red } else { Color::Rgb(150, 0, 0) };
                    spans.push(Span::styled(mask_char.to_string(), Style::default().fg(col)));
                } else {
                    let hue = (t * 0.3 + y as f64 * 0.05) % 1.0;
                    let r = ((hue * std::f64::consts::PI * 2.0).sin() * 127.0 + 128.0) as u8;
                    let g = (((hue + 0.33) * std::f64::consts::PI * 2.0).sin() * 127.0 + 128.0) as u8;
                    let b = (((hue + 0.66) * std::f64::consts::PI * 2.0).sin() * 127.0 + 128.0) as u8;
                    let char_to_draw = if treble > 0.7 && (x + y) % 7 == 0 {
                        let char_idx = ((x * 13 + y * 37 + (t * 15.0) as usize) % chars.len()) as usize;
                        chars[char_idx]
                    } else { mask_char };
                    let col = if treble > 0.8 && (x + y) % 2 == 0 { Color::White } else { Color::Rgb(r, g, b) };
                    spans.push(Span::styled(char_to_draw.to_string(), Style::default().fg(col)));
                }
            } else {
                let rain_y = ((t * 10.0 + x as f64 * 3.5) % height as f64) as usize;
                if y == rain_y {
                    let char_idx = ((x * 17 + y * 31 + (t * 20.0) as usize) % chars.len()) as usize;
                    spans.push(Span::styled(chars[char_idx].to_string(), Style::default().fg(Color::Rgb(0, 30, 0))));
                } else {
                    spans.push(Span::raw(" "));
                }
            }
        }
        lines.push(Line::from(spans));
    }
    f.render_widget(Paragraph::new(lines), area);
}

fn draw_sphere(canvas: &mut [(u8, Color)], w: usize, h: usize, t: f64, bass: f64, mid: f64, treble: f64) {
    let (cx, cy) = (w as f64 / 2.0, h as f64 / 2.0);
    let max_r = (cx*cx + cy*cy).sqrt() * 0.4 * (1.0 + bass * 0.6);

    for i in 0..300 {
        let angle = (i as f64 * 2.399) % (std::f64::consts::PI * 2.0) + t * 0.2;
        let dist = (i as f64 * 13.0 + t * 10.0) % (max_r * 4.0);
        let z = max_r * 2.5 - dist;
        let scale = 300.0 / (300.0 + z);
        let px = cx + angle.cos() * dist * scale;
        let py = cy + angle.sin() * dist * scale;
        let col = if i % 4 == 0 { Color::White } else { Color::Rgb(80, 100, 150) };
        plot(canvas, w, h, px as i32, py as i32, col);
    }

    let lat_steps = 16;
    let lon_steps = 24;

    for i in 1..lat_steps {
        let phi = std::f64::consts::PI * (i as f64 / lat_steps as f64);
        let mut prev: Option<(f64, f64, f64)> = None;
        for j in 0..=lon_steps {
            let theta = 2.0 * std::f64::consts::PI * (j as f64 / lon_steps as f64);
            let deform = (theta * 4.0 + t * 3.0).sin() * bass * 0.2 + (phi * 8.0 - t).cos() * mid * 0.1;
            let r = max_r * (1.0 + deform);
            let x = r * phi.sin() * theta.cos();
            let y = r * phi.cos();
            let z = r * phi.sin() * theta.sin();
            let (rx, ry, rz) = rotate_3d(x, y, z, t * 0.5, t * 0.3);
            let scale = 300.0 / (300.0 + rz + max_r);
            let depth = (rz + max_r) / (2.0 * max_r);
            let px = cx + rx * scale;
            let py = cy + ry * scale;
            if let Some((ppx, ppy, _)) = prev {
                let col = if treble > 0.7 && j % 3 == 0 { Color::White } else { get_gradient(1.0 - depth) };
                draw_line(canvas, w, h, ppx as i32, ppy as i32, px as i32, py as i32, col);
            }
            prev = Some((px, py, rz));
        }
    }

    for j in 0..lon_steps {
        let theta = 2.0 * std::f64::consts::PI * (j as f64 / lon_steps as f64);
        let mut prev: Option<(f64, f64)> = None;
        for i in 0..=lat_steps {
            let phi = std::f64::consts::PI * (i as f64 / lat_steps as f64);
            let deform = (theta * 4.0 + t * 3.0).sin() * bass * 0.2 + (phi * 8.0 - t).cos() * mid * 0.1;
            let r = max_r * (1.0 + deform);
            let x = r * phi.sin() * theta.cos();
            let y = r * phi.cos();
            let z = r * phi.sin() * theta.sin();
            let (rx, ry, rz) = rotate_3d(x, y, z, t * 0.5, t * 0.3);
            let scale = 300.0 / (300.0 + rz + max_r);
            let depth = (rz + max_r) / (2.0 * max_r);
            let px = cx + rx * scale;
            let py = cy + ry * scale;
            if let Some((ppx, ppy)) = prev {
                let col = get_gradient(1.0 - depth);
                draw_line(canvas, w, h, ppx as i32, ppy as i32, px as i32, py as i32, col);
            }
            prev = Some((px, py));
        }
    }
}

fn draw_radial(canvas: &mut [(u8, Color)], w: usize, h: usize, t: f64, freqs: &[f64]) {
    let (cx, cy) = (w as f64 / 2.0, h as f64 / 2.0);
    let base_r = 4.0;
    
    for (i, &f) in freqs.iter().enumerate() {
        let angle = (i as f64 / freqs.len() as f64) * std::f64::consts::PI * 2.0 + t * 0.2;
        let len = base_r + f * (h as f64) * 0.9;
        let wave = (t * 5.0 + i as f64).sin() * 0.15;
        let steps = (len / 1.5) as usize;
        for step in 0..steps {
            let ratio = step as f64 / steps as f64;
            let r = base_r + len * ratio;
            let x = cx + (angle + wave * ratio).cos() * r;
            let y = cy + (angle + wave * ratio).sin() * r;
            let col = get_gradient(ratio);
            plot(canvas, w, h, x as i32, y as i32, col);
            if f > 0.3 && step < steps / 2 {
                plot(canvas, w, h, x as i32 + 1, y as i32, col);
                plot(canvas, w, h, x as i32, y as i32 + 1, col);
            }
        }
    }
    
    let core_r = base_r + freqs[0] * 15.0;
    for a in 0..60 {
        let angle = (a as f64 / 60.0) * std::f64::consts::PI * 2.0 + t;
        let x = cx + angle.cos() * core_r;
        let y = cy + angle.sin() * core_r;
        plot(canvas, w, h, x as i32, y as i32, Color::White);
    }
}

fn draw_tunnel(canvas: &mut [(u8, Color)], w: usize, h: usize, t: f64, freqs: &[f64], bass: f64) {
    let (cx, cy) = (w as f64 / 2.0, h as f64 / 2.0);
    let num_rings = 20;
    let segments = 32;
    let speed = 2.0 + bass * 15.0;
    let mut rings = Vec::new();

    for i in 0..num_rings {
        let z = (i as f64 * 30.0 - (t * speed) % 30.0).abs() + 10.0; 
        let scale = 200.0 / z;
        let mut ring_points = Vec::new();
        for j in 0..segments {
            let angle = (j as f64 / segments as f64) * std::f64::consts::PI * 2.0 + t * 1.0;
            let r = 80.0 * scale * (1.0 + freqs[j % 32] * 0.5);
            let x = cx + angle.cos() * r;
            let y = cy + angle.sin() * r;
            ring_points.push((x, y, z));
        }
        rings.push(ring_points);
    }

    for i in 0..num_rings - 1 {
        let depth = rings[i][0].2 / (num_rings as f64 * 30.0);
        let col = get_gradient(depth);
        for j in 0..segments {
            let p1 = rings[i][j];
            let p2 = rings[i][(j + 1) % segments];
            let p3 = rings[i + 1][j];
            draw_line(canvas, w, h, p1.0 as i32, p1.1 as i32, p2.0 as i32, p2.1 as i32, col);
            if j % 2 == 0 {
                draw_line(canvas, w, h, p1.0 as i32, p1.1 as i32, p3.0 as i32, p3.1 as i32, col);
            }
        }
    }

    let core_r = 10.0 + bass * 30.0;
    for a in 0..40 {
        let angle = (a as f64 / 40.0) * std::f64::consts::PI * 2.0;
        let x = cx + angle.cos() * core_r;
        let y = cy + angle.sin() * core_r;
        plot(canvas, w, h, x as i32, y as i32, Color::Rgb(255, 200, 255));
    }
}

fn render_train(f: &mut Frame, area: Rect, t: f64, bass: f64, _mid: f64, treble: f64) {
    let width = area.width as usize;
    let height = area.height as usize;
    let sky_bg = Color::Rgb(5, 5, 20);
    let mut grid = vec![vec![(' ', sky_bg); width]; height];
    let ground_y = height.saturating_sub(1);
    let rail_y = height.saturating_sub(2);
    let sky_height = ground_y.saturating_sub(1);

    let speed = t * 20.0 * (1.0 + bass * 2.0); 
    let offset = (speed as usize) % 4;

    for x in 0..width {
        grid[ground_y][x] = ('_', Color::Rgb(30, 30, 30));
        if (x + offset) % 4 == 0 {
            grid[rail_y][x] = ('|', Color::Rgb(100, 50, 0));
        } else {
            grid[rail_y][x] = ('=', Color::Rgb(100, 100, 100));
        }
    }

    for i in 0..250 {
        let base_x = (i * 37) % width;
        let base_y = (i * 73) % sky_height.max(1);
        let twinkle = (t * 2.5 + i as f64 * 0.5).sin();
        let threshold = if treble > 0.5 { -0.5 } else { 0.3 };
        if twinkle > threshold {
            let (ch, col) = if twinkle > 0.8 {
                ('*', Color::White)
            } else if twinkle > 0.4 {
                ('+', Color::Rgb(200, 200, 255))
            } else {
                ('.', Color::Rgb(100, 100, 150))
            };
            grid[base_y][base_x] = (ch, col);
        }
    }

    let cloud_speed = t * 0.5 * (1.0 + bass * 0.5);
    for y in 0..(sky_height / 2) {
        for x in 0..width {
            let xf = x as f64;
            let yf = y as f64;
            let noise = (xf * 0.05 + cloud_speed).sin() 
                      + (xf * 0.02 - cloud_speed * 0.5).cos()
                      + (yf * 0.1 + cloud_speed * 0.2).sin();
            if noise > 1.3 {
                let ch = if noise > 1.8 { '~' } else if noise > 1.5 { '-' } else { '.' };
                let cloud_col = if bass > 0.6 { Color::Rgb(60, 60, 100) } else { Color::Rgb(30, 30, 50) };
                grid[y][x] = (ch, cloud_col);
            }
        }
    }

    let train = [
        "           (oo) ( ) ( ) ( )",
        "            (  )   (  )  ( )",
        "            (      )  (  )",
        "      __________________ ",
        "     /  __  __  __  __  |",
        "    |  |  ||  ||  ||  | |",
        "    |  |__||__||__||__| |",
        " ___|___________________|_ ",
        "|   |    -       -      | \\",
        "|___|___________________|__|",
        "  O   O    O   O   O    O  O",
    ];

    let train_width = train[0].len();
    let train_x = (width.saturating_sub(train_width) / 2) as i32;
    let train_y = (height.saturating_sub(13)) as i32;

    for (y_idx, row) in train.iter().enumerate() {
        for (x_idx, ch) in row.chars().enumerate() {
            let px = train_x + x_idx as i32;
            let py = train_y + y_idx as i32;
            if px >= 0 && px < width as i32 && py >= 0 && py < height as i32 && ch != ' ' {
                let col = if ch == 'O' { Color::Rgb(255, 200, 0) } 
                          else if ch == '_' || ch == '|' || ch == '/' || ch == '\\' { Color::Rgb(120, 80, 40) } 
                          else { Color::Rgb(180, 180, 180) };
                grid[py as usize][px as usize] = (ch, col);
            }
        }
    }

    let smoke_start_x = train_x + 10;
    let smoke_start_y = train_y;

    for i in 0..40 {
        let phase = (t * 4.0 + i as f64 * 1.5) % 30.0;
        let px = smoke_start_x - (phase * 1.2) as i32;
        let py = smoke_start_y - phase as i32;
        if px >= 0 && px < width as i32 && py >= 0 && py < height as i32 {
            let ch = if phase < 5.0 { 'o' } else if phase < 15.0 { 'O' } else { '@' };
            let col = if bass > 0.4 { Color::White } else { Color::Rgb(150, 150, 150) };
            grid[py as usize][px as usize] = (ch, col);
        }
    }

    let beam_x = train_x + train_width as i32 - 2;
    let beam_y = train_y + 7; 
    
    for i in 0..15 {
        let px = beam_x + i;
        if px >= 0 && px < width as i32 {
            if treble > 0.3 || i % 2 == 0 {
                grid[beam_y as usize][px as usize] = ('-', Color::Rgb(255, 255, 150));
            }
        }
    }

    let mut lines = Vec::new();
    for y in 0..height {
        let mut spans = Vec::new();
        for x in 0..width {
            let (ch, col) = grid[y][x];
            spans.push(Span::styled(ch.to_string(), Style::default().fg(col).bg(sky_bg)));
        }
        lines.push(Line::from(spans));
    }
    f.render_widget(Paragraph::new(lines), area);
}

fn render_eq(f: &mut Frame, app: &App) {
    let accent_color = parse_hex(&app.config.theme.accent_color);
    let area = centered_rect(60, 40, f.area());
    f.render_widget(Clear, area);
    
    let block = Block::bordered()
        .title(" Эквалайзер (E/Esc - закрыть) ")
        .title_alignment(ratatui::layout::Alignment::Center)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(accent_color))
        .style(Style::default().bg(Color::Rgb(10, 10, 15)));
        
    let inner = block.inner(area);
    f.render_widget(block, area);

    let data = app.audio_data.lock().unwrap();
    let mut heights = vec![0; 32];
    
    if data.len() == 32 {
        for (i, &val) in data.iter().enumerate() {
            heights[i] = (val as f64 / 255.0 * inner.height as f64) as u16;
        }
    } else {
        for i in 0..32 {
            let v = ((app.viz_time * 5.0 + i as f64 * 0.5).sin() * 0.5 + 0.5) * inner.height as f64;
            heights[i] = v as u16;
        }
    }

    let mut lines = Vec::new();
    for y in (0..inner.height).rev() {
        let mut spans = Vec::new();
        for x in 0..32 {
            let start_x = (x * inner.width as usize) / 32;
            let end_x = ((x + 1) * inner.width as usize) / 32;
            let bar_width = end_x.saturating_sub(start_x);
            
            if heights[x] > y {
                let ratio = y as f64 / inner.height as f64;
                let color = if ratio > 0.7 { Color::Red } 
                            else if ratio > 0.4 { Color::Yellow } 
                            else { Color::Green };
                for _ in 0..bar_width {
                    spans.push(Span::styled("█", Style::default().fg(color)));
                }
            } else {
                for _ in 0..bar_width {
                    spans.push(Span::raw(" "));
                }
            }
        }
        lines.push(Line::from(spans));
    }
    
    f.render_widget(Paragraph::new(lines), inner);
}

fn draw_line(canvas: &mut [(u8, Color)], w: usize, h: usize, x0: i32, y0: i32, x1: i32, y1: i32, col: Color) {
    let dx = (x1 - x0).abs();
    let dy = -(y1 - y0).abs();
    let sx = if x0 < x1 { 1 } else { -1 };
    let sy = if y0 < y1 { 1 } else { -1 };
    let mut err = dx + dy;
    let mut x = x0; 
    let mut y = y0;
    
    loop {
        plot(canvas, w, h, x, y, col);
        if x == x1 && y == y1 { break; }
        let e2 = 2 * err;
        if e2 >= dy { err += dy; x += sx; }
        if e2 <= dx { err += dx; y += sy; }
    }
}

fn get_gradient(ratio: f64) -> Color {
    let r = if ratio < 0.5 { (ratio * 2.0 * 150.0) as u8 } else { (150.0 + (ratio - 0.5) * 2.0 * 105.0) as u8 };
    let g = if ratio < 0.5 { (ratio * 2.0 * 50.0) as u8 } else { (50.0 + (ratio - 0.5) * 2.0 * 205.0) as u8 };
    let b = if ratio < 0.5 { (255.0 - ratio * 2.0 * 155.0) as u8 } else { (100.0 - (ratio - 0.5) * 2.0 * 100.0) as u8 };
    Color::Rgb(r, g, b)
}

fn plot(c: &mut [(u8, Color)], w: usize, h: usize, x: i32, y: i32, col: Color) { 
    if x>=0 && y>=0 && (x as usize)<w && (y as usize)<h { 
        c[y as usize*w + x as usize] = (1, col); 
    } 
}

fn braille(dx: u8, dy: u8) -> u32 { 
    match (dx, dy) { 
        (0,0)=>0x01,(0,1)=>0x02,(0,2)=>0x04,(1,0)=>0x08,
        (1,1)=>0x10,(1,2)=>0x20,(0,3)=>0x40,(1,3)=>0x80,
        _=>0 
    } 
}

fn rotate_3d(x: f64, y: f64, z: f64, ax: f64, ay: f64) -> (f64, f64, f64) {
    let y1 = y * ax.cos() - z * ax.sin(); 
    let z1 = y * ax.sin() + z * ax.cos();
    (x * ay.cos() + z1 * ay.sin(), y1, -x * ay.sin() + z1 * ay.cos())
}