// =====================================================================
// Интерфейс TUI плеера нах
// =====================================================================

// Стандартная библиотека
use ratatui::{
    layout::{Constraint, Layout, Rect}, 
    style::{Color, Modifier, Style}, 
    text::{Line, Span},
    widgets::{Block, Clear, Gauge, List, ListItem, Paragraph, ListState, BorderType, Padding}, 
    Frame,
};

// Внутренние модули проекта
use crate::app::{App, Focus, Mode, LocalItem,SavedItem};
use crate::config::parse_hex;
use crate::viz::*;

/// Верхняя строка статуса: громкость, режим повтора, прогресс скачивания, статус, пауза, играющий трек.
fn render_status_bar(f: &mut Frame, app: &App, area: Rect) {
    let bg_color = if app.config.theme.transparent_panels {
        Color::Reset
    } else {
        parse_hex(&app.config.theme.bg_color)
    };
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

    let chaos_icon = if app.chaos_mode { "🌀 " } else { "" };

    let status_line = Line::from(vec![
        Span::styled(format!(" Vol {}% ", app.volume), Style::default().fg(Color::Yellow)),
        Span::styled("│ ", Style::default().fg(inactive_border)),
        Span::styled(format!(" {} ", repeat_icon), Style::default().fg(Color::Magenta)),
        Span::styled("│ ", Style::default().fg(inactive_border)),
        Span::styled(chaos_icon, Style::default().fg(Color::Red)),
        Span::styled(download_str, Style::default().fg(Color::LightGreen)),
        Span::styled(format!(" {} ", &app.status), Style::default().fg(active_border)),
        Span::styled("│ ", Style::default().fg(inactive_border)),
        Span::styled(format!(" {} ", paused_icon), Style::default().fg(Color::Green)),
        Span::styled(now, Style::default().fg(Color::White).add_modifier(Modifier::BOLD)),
    ]);
    f.render_widget(Paragraph::new(status_line).style(Style::default().bg(bg_color)), area);
}

pub fn draw(f: &mut Frame, app: &App) {
    // Если панели прозрачные — используем Reset (терминальный фон), иначе — тему
    let app_bg = if app.config.theme.transparent_panels {
        Color::Reset
    } else {
        parse_hex(&app.config.theme.bg_color)
    };
    f.render_widget(Block::default().style(Style::default().bg(app_bg)), f.area());

    // Полноэкранный режим — только визуализатор и строка статуса.
    if app.fullscreen_viz {
        let chunks = Layout::vertical([
            Constraint::Min(1),
            Constraint::Length(1),
        ]).split(f.area());

        render_visualizer(f, app, chunks[0]);
        render_status_bar(f, app, chunks[1]);
        
        if app.show_help { render_help(f, app); }
        if app.show_lyrics { render_lyrics(f, app); }
        if app.confirm_delete.is_some() || app.confirm_delete_bookmark.is_some() {
            render_confirm_delete(f, app);
        }
        return;
    }

    // Обычный режим: поиск, основная зона, прогресс-бар, статус.
    let chunks = Layout::vertical([
        Constraint::Length(3),
        Constraint::Min(5),
        Constraint::Length(3),
        Constraint::Length(1),
    ]).split(f.area());

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
        Mode::CreatingBookmarkFolder => (format!(" {} Имя новой папки закладок ", spinner), Style::default().fg(Color::Green)),
        Mode::RenamingBookmarkFolder => (format!(" {} Новое имя папки закладок ", spinner), Style::default().fg(Color::Green)),
        Mode::Normal => (" Нажмите '/' для поиска, '?' для справки ".to_string(), Style::default().fg(text_color)),
    };

    // В строке ввода отображаем тот буфер, который относится к текущему режиму.
    let display_text = match app.mode {
        Mode::FilteringLocal => &app.local_filter,
        Mode::RenamingFile => &app.rename_input,
        Mode::CreatingBookmarkFolder | Mode::RenamingBookmarkFolder => &app.input,
        _ => &app.input,
    };

    let search_block = Block::bordered()
        .title(title)
        .border_type(BorderType::Rounded)
        .border_style(style)
        .padding(Padding::horizontal(1));
    f.render_widget(Paragraph::new(display_text.as_str()).style(Style::default().fg(Color::White)).block(search_block), chunks[0]);

    // Основная зона. Определяем какие панели винды
    let mut vis = Vec::new();
    if app.show_local { vis.push(("local", 25)); }
    if app.show_results { vis.push(("results", 35)); }
    if app.show_saved { vis.push(("saved", 20)); }
    // Визуализатор есть ВСЕГДА, даём ему оставшееся место
    vis.push(("viz", 30)); 

    let total_weight: u32 = vis.iter().map(|v| v.1).sum();
    let constraints: Vec<Constraint> = vis.iter()
        .map(|(_, w)| Constraint::Percentage(((w * 100) / total_weight) as u16))
        .collect();
    
    let main_area = Layout::horizontal(constraints).split(chunks[1]);
    
    let mut idx = 0;
    if app.show_local { render_local(f, app, main_area[idx]); idx += 1; }
    if app.show_results { render_results(f, app, main_area[idx]); idx += 1; }
    if app.show_saved { render_saved(f, app, main_area[idx]); idx += 1; }
    
    // 1. РИСУЕМ ВИЗУАЛИЗАТОР
    render_visualizer(f, app, main_area[idx]);

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

    let viz_rect = if app.show_results {
        Layout::horizontal([
            Constraint::Percentage(25), Constraint::Percentage(40), Constraint::Percentage(35),
        ]).split(chunks[1])[2]
    } else {
        Layout::horizontal([
            Constraint::Percentage(30), Constraint::Percentage(70),
        ]).split(chunks[1])[1]
    };

    if app.ambient_effect {
        render_ambient_effect(f, app, viz_rect);
    }

    // Всплывающие окна рисуются последними — они перекрывают всё остальное.
    if app.show_chaos_info { render_chaos_info(f, app); }
    if app.show_help { render_help(f, app); }
    if app.show_lyrics { render_lyrics(f, app); }
    if app.confirm_delete.is_some() || app.confirm_delete_bookmark.is_some() { render_confirm_delete(f, app); }
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

///панель локальной библиотеки.
fn render_local(f: &mut Frame, app: &App, area: Rect) {
    let is_active = app.focus == Focus::Local;
    let active_border = parse_hex(&app.config.theme.active_border);
    let inactive_border = parse_hex(&app.config.theme.inactive_border);
    let selected_bg = parse_hex(&app.config.theme.selected_bg);

    let items: Vec<ListItem> = app.local_items.iter().enumerate().map(|(i, item)| {
        let (mut text, color) = match item {
            LocalItem::Dir(p) => (format!("{}", p.file_name().unwrap_or_default().to_string_lossy()), Color::Cyan),
            LocalItem::File(t) => (format!("{}", t.title), Color::White),
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
    let short = if path_str.chars().count() > 15 {
        let chars: Vec<char> = path_str.chars().collect();
        chars[chars.len()-15..].iter().collect::<String>()
    } else {
        path_str.clone()
    };
    let title = format!(" 📁 ...{} ", short);
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
    let area = centered_rect(50, 20, f.area());
    f.render_widget(Clear, area);
    let accent_color = parse_hex(&app.config.theme.accent_color);
    
    let block = Block::bordered()
        .title(" Подтверждение ")
        .title_alignment(ratatui::layout::Alignment::Center)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(accent_color))
        .style(Style::default().bg(Color::Rgb(15, 15, 20)));
    
    let msg = if app.confirm_delete.is_some() {
        "Вы уверены, что хотите удалить файл/папку с диска?"
    } else if let Some(item) = &app.confirm_delete_bookmark {
        match item {
            SavedItem::Folder { name, .. } => {
                // Такой формат Line::from(String) работает
                return f.render_widget(
                    Paragraph::new(vec![
                        Line::from(format!("Удалить папку '{}'", name)),
                        Line::from("и все вложенные в неё треки?"),
                        Line::from(""),
                        Line::from(Span::styled("y - Да  |  n/Esc - Нет", Style::default().fg(Color::Yellow))),
                    ]).block(block).alignment(ratatui::layout::Alignment::Center),
                    area,
                );
            }
            SavedItem::Track { track, .. } => {
                return f.render_widget(
                    Paragraph::new(vec![
                        Line::from("Удалить трек из закладок?"),
                        Line::from(""),
                        Line::from(Span::styled(&track.title, Style::default().fg(Color::Cyan))),
                        Line::from(""),
                        Line::from(Span::styled("y - Да  |  n/Esc - Нет", Style::default().fg(Color::Yellow))),
                    ]).block(block).alignment(ratatui::layout::Alignment::Center),
                    area,
                );
            }
        }
    } else {
        "?"
    };
    
    let text = vec![
        Line::from(msg),
        Line::from(""),
        Line::from(Span::styled("y - Да  |  n/Esc - Нет", Style::default().fg(Color::Yellow))),
    ];
    f.render_widget(
        Paragraph::new(text).block(block).alignment(ratatui::layout::Alignment::Center),
        area,
    );
}

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
        8 => " 3D Океан ",
        9 => " Черная дыра ",
        10 => " Плазма ",
        11 => " Спирограф ",
        12 => " Варп-прыжок ",
        13 => " Слияние визуализаторов ",
        14 => " Снежный Барс ",
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
    
    let data = app.audio_data.lock().unwrap();
    let freqs: Vec<f64> = if data.len() == 32 {
        data.iter().map(|&x| x as f64 / 255.0).collect()
    } else {
        (0..32).map(|i| {
            let t = app.viz_time;
            let f = i as f64;
            ((t * 5.0 + f * 0.3).sin() * 0.5 + 0.5) * ((t * 2.0 + f * 0.1).cos() * 0.5 + 0.5)
        }).collect()
    };

    let bass: f64 = freqs[0..8].iter().sum::<f64>() / 8.0;
    let mid: f64 = freqs[8..20].iter().sum::<f64>() / 12.0;
    let treble: f64 = freqs[20..32].iter().sum::<f64>() / 12.0;

    // Визуализаторы на основе символов (не Брайля) отрисовываются сразу
    if app.viz_mode == 7 { render_train(f, inner, app.viz_time, bass, mid, treble); return; }
    if app.viz_mode == 6 { render_aquarium(f, inner, app.viz_time, bass, treble); return; }
    if app.viz_mode == 3 { render_matrix_rain(f, inner, app.viz_time, bass, treble); return; }
    if app.viz_mode == 4 { render_skull(f, inner, app.viz_time, bass, treble); return; }

    let w = inner.width as usize * 2; 
    let h = inner.height as usize * 4;
    let mut canvas = vec![(0u8, Color::Reset); w * h];

    // --- ЛОГИКА РЕЖИМА СЛИЯНИЯ ---
    if app.viz_mode == 13 {
        let viz_list = [0, 1, 2, 8, 9, 10, 11, 12,14];
        let cycle_time = 15.0; 
        let transition_time = 4.0; // Переход теперь чуть быстрее
        
        let total_time = app.viz_time;
        let index = (total_time / cycle_time) as usize % viz_list.len();
        let next_index = (index + 1) % viz_list.len();
        let time_in_cycle = total_time % cycle_time;
        
        if time_in_cycle > (cycle_time - transition_time) {
            // Прогресс перехода от 0.0 до 1.0
            let mut progress = (time_in_cycle - (cycle_time - transition_time)) / transition_time;
            
            // ТЕХНО-ФИШКА 1: Если бьет бас, переход ускоряется 
            if bass > 0.6 {
                progress = (progress * 1.5).min(1.0);
            }
            
            let mode_a = viz_list[index];
            let mode_b = viz_list[next_index];
            
            let mut canvas_a = vec![(0u8, Color::Reset); w * h];
            let mut canvas_b = vec![(0u8, Color::Reset); w * h];
            
            draw_mode(mode_a, &mut canvas_a, w, h, app, &freqs, bass, mid, treble);
            draw_mode(mode_b, &mut canvas_b, w, h, app, &freqs, bass, mid, treble);
            
            // Каждые несколько кадров картинка "дергается" в сторону
            let glitch_x = if (app.tick / 2) % 10 == 0 {
                ((app.tick * 13) % 10) as i32 - 5 // Сдвиг от -5 до +5
            } else {
                0
            };

            for y in 0..h {
                for x in 0..w {
                    // Применяем глитч-сдвиг (с защитой от выхода за границы)
                    let src_x = (x as i32 - glitch_x).clamp(0, w as i32 - 1) as usize;
                    let idx = y * w + src_x;

                    let (on_a, col_a) = canvas_a[idx];
                    let (on_b, col_b) = canvas_b[idx];
                    
                    // Если оба визуализатора в этой точке активны
                    if on_a > 0 && on_b > 0 {
                        let mixed = blend_techno(col_a, col_b, progress, x, y);
                        canvas[y * w + x] = (1, mixed);
                    } else if on_a > 0 {
                        // Старый визуализатор распадается на пиксели
                        if progress < (0.9 - (x as f64 / w as f64) * 0.3) { // Волна распада слева направо
                            canvas[y * w + x] = (1, fade_color(col_a, 1.0 - progress));
                        }
                    } else if on_b > 0 {
                        // Новый визуализатор собирается из шума
                        if progress > (x as f64 / w as f64) * 0.3 {
                            canvas[y * w + x] = (1, fade_color(col_b, progress));
                        }
                    }
                }
            }
        } else {
            let mode_a = viz_list[index];
            draw_mode(mode_a, &mut canvas, w, h, app, &freqs, bass, mid, treble);
        }
    } else {
        // --- ОБЫЧНЫЕ РЕЖИМЫ ---
        match app.viz_mode {
            0 => draw_sphere(&mut canvas, w, h, app.viz_time, bass, mid, treble),
            1 => draw_radial(&mut canvas, w, h, app.viz_time, &freqs),
            2 => draw_tunnel(&mut canvas, w, h, app.viz_time, &freqs, bass),
            5 => draw_dna(&mut canvas, w, h, app.viz_time, bass, mid, treble),
            8 => draw_ocean(&mut canvas, w, h, app.viz_time, bass, mid),
            9 => draw_black_hole(&mut canvas, w, h, app.viz_time, bass, mid, treble),
            10 => draw_plasma(&mut canvas, w, h, app.viz_time, bass, mid, treble),
            11 => draw_spirograph(&mut canvas, w, h, app.viz_time, bass, mid, treble),
            12 => draw_warp(&mut canvas, w, h, app.viz_time, bass, treble),
            14 => draw_cat(&mut canvas, w, h, app.viz_time, bass, mid, treble),
            _ => {}
        }
    }

    // Конвертация в Брайль
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
            spans.push(if has { Span::styled(char::from_u32(code).unwrap_or(' ').to_string(), Style::default().fg(col)) } else { Span::raw(" ") });
        }
        lines.push(Line::from(spans));
    }
    f.render_widget(Paragraph::new(lines), inner);
}

/// Окно с текстом песни 
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
        
        // Если вручную скроллил — используем его позицию. Иначе авто-центрирование.
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

/// Окно справки
fn render_help(f: &mut Frame, app: &App) {
    let accent_color = parse_hex(&app.config.theme.accent_color);
    let active_border = parse_hex(&app.config.theme.active_border);
    
    // Делаем окно чуть шире для двух колонок
    let area = centered_rect(85, 85, f.area()); 
    f.render_widget(Clear, area);
    
    let block = Block::bordered()
        .title(" Справка ")
        .title_alignment(ratatui::layout::Alignment::Center)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(accent_color))
        .style(Style::default().bg(Color::Rgb(15, 15, 20)))
        .padding(Padding::uniform(1));

    let inner = block.inner(area);
    f.render_widget(block, area);

    // Разделяем внутреннее пространство на две колонки и подвал
    let v_chunks = Layout::vertical([
        Constraint::Min(10), // Место для колонок
        Constraint::Length(2), // Место для "Нажмите любую клавишу"
    ]).split(inner);

    let h_chunks = Layout::horizontal([
        Constraint::Percentage(50),
        Constraint::Percentage(50)
    ]).split(v_chunks[0]);

    let key_style = Style::default().fg(active_border).add_modifier(Modifier::BOLD);
    let desc_style = Style::default().fg(Color::Gray);
    let head_style = Style::default().fg(accent_color).add_modifier(Modifier::BOLD);

    // --- ЛЕВАЯ КОЛОНКА ---
    let left_text = vec![
        Line::from(Span::styled("  НАВИГАЦИЯ И ПАНЕЛИ", head_style)),
        Line::from(vec![Span::styled("Tab ", key_style), Span::styled("Сменить фокус", desc_style)]),
        Line::from(vec![Span::styled("1/2/3 ", key_style), Span::styled("Local/YouTube/Закладки", desc_style)]),
        Line::from(vec![Span::styled("j/k (↑/↓) ", key_style), Span::styled("Листать список", desc_style)]),
        Line::from(vec![Span::styled("Enter ", key_style), Span::styled("Играть / Войти в папку", desc_style)]),
        Line::from(vec![Span::styled("Backspc ", key_style), Span::styled("На уровень выше", desc_style)]),
        Line::from(""),

        Line::from(Span::styled("  ВОСПРОИЗВЕДЕНИЕ", head_style)),
        Line::from(vec![Span::styled("Space/p ", key_style), Span::styled("Пауза / Продолжить", desc_style)]),
        Line::from(vec![Span::styled("h/l ", key_style), Span::styled("Перемотка -5/+5 сек", desc_style)]),
        Line::from(vec![Span::styled("s ", key_style), Span::styled("Перемотка на время", desc_style)]),
        Line::from(vec![Span::styled("+/- ", key_style), Span::styled("Громкость", desc_style)]),
        Line::from(vec![Span::styled("n ", key_style), Span::styled("Следующий трек", desc_style)]),
        Line::from(vec![Span::styled("r ", key_style), Span::styled("Shuffle (случайно)", desc_style)]),
        Line::from(vec![Span::styled("R ", key_style), Span::styled("Режим повтора", desc_style)]),
        Line::from(""),

        Line::from(Span::styled("  ПОИСК И СКАЧИВАНИЕ", head_style)),
        Line::from(vec![Span::styled("/ ", key_style), Span::styled("Поиск на YouTube", desc_style)]),
        Line::from(vec![Span::styled("i ", key_style), Span::styled("Скачать плейлист по URL", desc_style)]),
        Line::from(vec![Span::styled("D ", key_style), Span::styled("Скачать трек в MP3", desc_style)]),
    ];

    // --- ПРАВАЯ КОЛОНКА ---
    let right_text = vec![
        Line::from(Span::styled("  ЛОКАЛЬНАЯ БИБЛИОТЕКА", head_style)),
        Line::from(vec![Span::styled("m ", key_style), Span::styled("Создать папку", desc_style)]),
        Line::from(vec![Span::styled("f ", key_style), Span::styled("Фильтр файлов", desc_style)]),
        Line::from(vec![Span::styled("r ", key_style), Span::styled("Переименовать файл", desc_style)]),
        Line::from(vec![Span::styled("d ", key_style), Span::styled("Удалить файл/папку", desc_style)]),
        Line::from(vec![Span::styled("F5 ", key_style), Span::styled("Обновить список", desc_style)]),
        Line::from(""),

        Line::from(Span::styled("  ЗАКЛАДКИ", head_style)),
        Line::from(vec![Span::styled("Enter ", key_style), Span::styled("Войти в папку / Играть", desc_style)]),
        Line::from(vec![Span::styled("a ", key_style), Span::styled("Добавить из YouTube", desc_style)]),
        Line::from(vec![Span::styled("m ", key_style), Span::styled("Создать папку", desc_style)]),
        Line::from(vec![Span::styled("r ", key_style), Span::styled("Переименовать папку", desc_style)]),
        Line::from(vec![Span::styled("d ", key_style), Span::styled("Удалить", desc_style)]),
        Line::from(""),

        Line::from(Span::styled("  ЭФФЕКТЫ И ВИЗУАЛИЗАЦИЯ", head_style)),
        Line::from(vec![Span::styled("V ", key_style), Span::styled("Сменить визуализатор", desc_style)]),
        Line::from(vec![Span::styled("F ", key_style), Span::styled("Полный экран", desc_style)]),
        Line::from(vec![Span::styled("T ", key_style), Span::styled("Текст песни (караоке)", desc_style)]),
        Line::from(vec![Span::styled("C ", key_style), Span::styled("Chaos Mode (5 треков)", desc_style)]),
    ];

    let left_para = Paragraph::new(left_text).block(Block::default().padding(Padding::horizontal(1)));
    let right_para = Paragraph::new(right_text).block(Block::default().padding(Padding::horizontal(1)));

    f.render_widget(left_para, h_chunks[0]);
    f.render_widget(right_para, h_chunks[1]);

    // Подвал с подсказкой
    let footer = Paragraph::new("Нажмите любую клавишу, чтобы закрыть")
        .style(Style::default().fg(Color::DarkGray))
        .alignment(ratatui::layout::Alignment::Center);
    f.render_widget(footer, v_chunks[1]);
}

fn render_saved(f: &mut Frame, app: &App, area: Rect) {
    let is_active = app.focus == Focus::Saved;
    let active_border = parse_hex(&app.config.theme.active_border);
    let inactive_border = parse_hex(&app.config.theme.inactive_border);
    let selected_bg = parse_hex(&app.config.theme.selected_bg);

    let items = app.get_saved_items();
    let list_items: Vec<ListItem> = items.iter().enumerate().map(|(i, item)| {
        let (text, base_color) = match item {
            SavedItem::Folder { name, .. } => (format!("{}", name), Color::Cyan),
            SavedItem::Track { track, .. } => {
                let mark = if track.local_path.is_some() { "" } else { "" };
                let is_playing = app.current.as_ref().map(|c| c.id == track.id).unwrap_or(false);
                let play = if is_playing { "▶ " } else { "" };
                (format!("{}{}{}", play, mark, track.title), Color::White)
            }
        };

        let is_playing = match item {
            SavedItem::Track { track, .. } => app.current.as_ref().map(|c| c.id == track.id).unwrap_or(false),
            _ => false,
        };
        //стиль залупа
        let style = if i == app.saved_selected { 
            Style::default().bg(selected_bg).fg(Color::White) 
        } else if is_playing {
            Style::default().fg(Color::LightGreen) // Зеленая подсветка играющего
        } else { 
            Style::default().fg(base_color) 
        };
        
        ListItem::new(text).style(style)
    }).collect();

    let mut cur = &app.saved_tree;
    let mut names = Vec::new();
    for &i in &app.saved_path {
        names.push(cur.folders[i].name.clone());
        cur = &cur.folders[i];
    }
    let path_str = names.join(" / ");
    let title = if path_str.is_empty() { " Закладки ".to_string() } else { format!(" 🔖 {} ", path_str) };

    let mut state = ListState::default();
    if !list_items.is_empty() { state.select(Some(app.saved_selected)); }
    
    let block = Block::bordered()
        .title(title)
        .title_alignment(ratatui::layout::Alignment::Center)
        .border_type(BorderType::Rounded)
        .border_style(if is_active { Style::default().fg(active_border) } else { Style::default().fg(inactive_border) })
        .padding(Padding::horizontal(1));
        
    f.render_stateful_widget(List::new(list_items).block(block), area, &mut state);
}

// ==========================================
// ВИЗУАЛИЗАТОРЫ 
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


/// Вызывает нужную функцию рисования на основе индекса режима
fn draw_mode(mode: usize, canvas: &mut [(u8, Color)], w: usize, h: usize, app: &App, freqs: &[f64], bass: f64, mid: f64, treble: f64) {
    match mode {
        0 => draw_sphere(canvas, w, h, app.viz_time, bass, mid, treble),
        1 => draw_radial(canvas, w, h, app.viz_time, freqs),
        2 => draw_tunnel(canvas, w, h, app.viz_time, freqs, bass),
        8 => draw_ocean(canvas, w, h, app.viz_time, bass, mid),
        9 => draw_black_hole(canvas, w, h, app.viz_time, bass, mid, treble),
        10 => draw_plasma(canvas, w, h, app.viz_time, bass, mid, treble),
        11 => draw_spirograph(canvas, w, h, app.viz_time, bass, mid, treble),
        12 => draw_warp(canvas, w, h, app.viz_time, bass, treble),
        14 => draw_cat(canvas, w, h, app.viz_time, bass, mid, treble),
        _ => {}
    }
}

// Плавно затухает цвет (уменьшает яркость RGB)
fn fade_color(color: Color, factor: f64) -> Color {
    match color {
        Color::Rgb(r, g, b) => Color::Rgb(
            (r as f64 * factor).clamp(0.0, 255.0) as u8,
            (g as f64 * factor).clamp(0.0, 255.0) as u8,
            (b as f64 * factor).clamp(0.0, 255.0) as u8,
        ),
        c => c, // Если цвет не RGB (например, Reset), возвращаем как есть
    }
}

//Частички на фоне
fn render_ambient_effect(f: &mut Frame, app: &App, skip: Rect) {
    let area = f.area();
    let buf = f.buffer_mut();

    for p in &app.ambient_particles {
        let x = area.x + (p.x * area.width as f64) as u16;
        let y = area.y + (p.y * area.height as f64) as u16;

        if x >= area.x + area.width || y >= area.y + area.height { continue; }
        if skip.contains((x, y).into()) { continue; }

        if let Some(cell) = buf.cell_mut((x, y)) {
            if cell.symbol() != " " { continue; }

            let ratio = 1.0 - p.life;
            let mut color = get_gradient(ratio);
            
            if let Color::Rgb(r, g, b) = color {
                let v = (p.life * 0.7).clamp(0.0, 1.0);
                color = Color::Rgb((r as f64 * v) as u8, (g as f64 * v) as u8, (b as f64 * v) as u8);
            }

            cell.set_symbol(&p.ch.to_string());
            cell.set_fg(color);
        }
    }
}

fn render_chaos_info(f: &mut Frame, app: &App) {
    // Окно в центре экрана
    let area = centered_rect(75, 40, f.area());
    f.render_widget(Clear, area);
    
    let block = Block::bordered()
        .title(" 🌀 CHAOS MODE (C — стоп, Esc — скрыть) ")
        .title_alignment(ratatui::layout::Alignment::Center)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(Color::Rgb(255, 80, 80)))
        .style(Style::default().bg(Color::Rgb(20, 5, 10))); // Тёмно-красный фон
        
    let inner = block.inner(area);
    f.render_widget(block, area);

    let mut lines = Vec::new();
    let elapsed = app.chaos_start.map(|s| s.elapsed().as_secs_f64()).unwrap_or(0.0);
    let time_str = format!("{:02}:{:02}", (elapsed / 60.0) as u64, (elapsed % 60.0) as u64);
    
    lines.push(Line::from(vec![
        Span::styled("5 треков играют одновременно ", Style::default().fg(Color::Gray)),
        Span::styled(format!("| Время: {}", time_str), Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
    ]));
    lines.push(Line::from(""));

    for (i, track) in app.chaos_tracks.iter().enumerate() {
        let reverse_str = if track.reversed { "REV" } else { "FWD" };
        let reverse_col = if track.reversed { Color::Magenta } else { Color::Green };
        
        let line = Line::from(vec![
            Span::styled(format!(" {}. ", i + 1), Style::default().fg(Color::LightRed)),
            Span::styled(format!("{:<25.25}", track.title), Style::default().fg(Color::White)),
            Span::styled(format!(" | Pitch: {:.2}x ", track.pitch), Style::default().fg(Color::Cyan)),
            Span::styled(format!("| {} ", reverse_str), Style::default().fg(reverse_col)),
            Span::styled(format!("| Старт: {:.0}s", track.start_offset), Style::default().fg(Color::Blue)),
        ]);
        lines.push(line);
    }

    lines.push(Line::from(""));

    f.render_widget(
        Paragraph::new(lines)
            .block(Block::default().padding(Padding::horizontal(1)))
            .alignment(ratatui::layout::Alignment::Left), 
        inner
    );
}

fn blend_techno(c1: Color, c2: Color, progress: f64, x: usize, _y: usize) -> Color {
    let (r1, g1, b1) = match c1 { Color::Rgb(r, g, b) => (r, g, b), _ => (0, 0, 0) };
    let (r2, g2, b2) = match c2 { Color::Rgb(r, g, b) => (r, g, b), _ => (0, 0, 0) };

    // Базовая интерполяция
    let mut r = (r1 as f64 * (1.0 - progress) + r2 as f64 * progress) as i32;
    let mut g = (g1 as f64 * (1.0 - progress) + g2 as f64 * progress) as i32;
    let mut b = (b1 as f64 * (1.0 - progress) + b2 as f64 * progress) as i32;

    // Хроматическая аберрация: на пике перехода (progress ~ 0.5) 
    // каналы R и B "разъезжаются" в стороны
    let aberration = ((std::f64::consts::PI * progress).sin() * 30.0) as i32;
    
    // Сдвигаем красный канал вправо, синий влево (зависит от координаты X)
    let shift = (x as i32 % 10) - 5; 
    if aberration > 0 {
        r = (r + aberration + shift).clamp(0, 255);
        b = (b - aberration + shift).clamp(0, 255);
    }

    // Неоновый буст: делаем цвета более "кислотными"
    let max_c = r.max(g).max(b);
    if max_c > 100 {
        // Подмешиваем магнету и циан
        let is_transition_peak = progress > 0.4 && progress < 0.6;
        if is_transition_peak {
            r = (r as f64 * 1.5).min(255.0) as i32; // Красный/Магента
            g = (g as f64 * 0.5) as i32;             // Гасим зеленый
            b = (b as f64 * 1.5).min(255.0) as i32; // Синий/Циан
        }
    }

    Color::Rgb(r as u8, g as u8, b as u8)
}

