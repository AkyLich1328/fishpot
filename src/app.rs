use std::{io, sync::mpsc, time::Duration, process::Command, path::PathBuf, fs};
use crossterm::event::{self, Event, KeyCode, KeyEventKind};
use ratatui::{backend::Backend, Terminal};
use notify_rust::Notification;
use crate::{mpv::Mpv, ui, ytdlp::{self, Track}, api, config::AppConfig};

/// Частица для ambient-эффекта: летит по экрану, тускнеет со временем.
#[derive(Clone)]
pub struct Particle {
    pub x: f64,       // нормализованная позиция 0.0..1.0
    pub y: f64,
    pub vx: f64,      // скорость в нормализованных единицах за кадр
    pub vy: f64,
    pub life: f64,    // 1.0 = только родилась, 0.0 = умерла
    pub decay: f64,   // насколько уменьшается life за кадр
    pub ch: char,
    pub phase: f64,
}

impl Particle {
    /// Спавн частицы-искры. Вылетает из правой части экрана (от визуализатора) и летит влево.
    pub fn spawn(energy: f64) -> Self {
        let r = || {
            use std::sync::atomic::{AtomicU64, Ordering};
            static STATE: AtomicU64 = AtomicU64::new(0x9E3779B97F4A7C15);
            let x = STATE.fetch_add(0x9E3779B97F4A7C15, Ordering::Relaxed);
            let x = x.wrapping_mul(0x9E3779B97F4A7C15) ^ (x >> 32);
            (x >> 11) as f64 / (1u64 << 53) as f64
        };

        // Мелкие точки, как в Брайле
        let chars = ['·', '∙', '⋅', '•', '°', '○', '｡'];
        let ch = chars[(r() * chars.len() as f64) as usize % chars.len()];

        // Стартуем из правой части экрана (где обычно визуализатор)
        let x = 1.0 + r() * 0.1; 
        let y = r();

        // Летят влево
        let vx = -(0.002 + energy * 0.008);
        let vy = (r() - 0.5) * 0.003;

        Self {
            x, y, vx, vy,
            life: 1.0,
            decay: 0.002 + r() * 0.003, 
            ch,
            phase: r() * std::f64::consts::PI * 2.0,
        }
    }
}

/// Режим повтора трека:
/// - None: по окончании трека ничего не включается (если плейлист кончился)
/// - All: по окончании трека зацикливаем весь список
/// - One: зацикливаем текущий трек
#[derive(PartialEq, Clone, Copy, Debug)]
pub enum RepeatMode { None, All, One }

/// Режим ввода. Влияет на то, что показывается в строке ввода и как обрабатываются клавиши.
#[derive(PartialEq, Clone, Copy)]
pub enum Mode { 
    Normal,          // Обычное управление
    Searching,       // Ввод поискового запроса
    Seeking,         // Ввод времени для перемотки (mm:ss)
    Importing,       // Ввод URL плейлиста
    CreatingDir,     // Ввод имени новой папки
    FilteringLocal,  // Ввод фильтра для локальных файлов
    RenamingFile,    // Ввод нового имени файла
}

/// Какая панель сейчас в фокусе. Tab переключает между ними.
#[derive(PartialEq, Clone, Copy)]
pub enum Focus { Results, Local }

impl Focus {
    fn next(self) -> Self { 
        match self { Focus::Results => Focus::Local, Focus::Local => Focus::Results } 
    }
}

/// Элемент локальной папки: либо подпапка, либо аудиофайл.
#[derive(Debug)]
pub enum LocalItem { Dir(PathBuf), File(Track) }

pub struct App {
    pub mode: Mode, 
    pub focus: Focus, 
    pub input: String,
    pub results: Vec<Track>, 
    pub selected: usize,
    pub local_items: Vec<LocalItem>, 
    pub local_selected: usize,
    pub current_dir: PathBuf,
    pub current: Option<Track>,
    pub status: String, 
    pub volume: i64, 
    pub paused: bool, 
    pub show_help: bool,
    pub time_pos: f64, 
    pub duration: f64, 
    pub mpv: Option<Mpv>,
    
    // Каналы для фоновых задач (поиск, импорт, скачивание, текст песни)
    pub search_rx: Option<mpsc::Receiver<Result<Vec<Track>, String>>>,
    pub import_rx: Option<mpsc::Receiver<crate::ytdlp::DownloadEvent>>,
    pub download_rx: Option<(String, mpsc::Receiver<crate::ytdlp::DownloadEvent>)>,
    pub lyrics_rx: Option<mpsc::Receiver<Vec<(f64, String)>>>,
    
    pub tick: usize, 
    pub shuffle: bool,
    pub viz_time: f64, 
    pub viz_mode: usize, 
    pub audio_data: std::sync::Arc<std::sync::Mutex<Vec<u8>>>,
    pub search_history: Vec<String>, 
    pub history_idx: Option<usize>,
    pub show_lyrics: bool, 
    pub lyrics_data: Option<Vec<(f64, String)>>, 
    pub lyrics_scroll: Option<u16>,
    pub show_results: bool,
    pub fullscreen_viz: bool,
    pub repeat_mode: RepeatMode,
    pub last_eof: bool,
    pub show_eq: bool,
    pub download_percent: Option<f64>,
    pub config: AppConfig,
    pub local_filter: String,
    pub rename_input: String,
    pub file_to_rename: Option<PathBuf>,
    pub confirm_delete: Option<PathBuf>,

    //Ambient эффект
    pub ambient_effect: bool,
    pub ambient_particles: Vec<Particle>,
}

impl App {
    pub fn new(initial_files: Vec<PathBuf>) -> Self {
        let config = AppConfig::load();
        let ambient_effect = config.ambient_effect;
        let mpv = Mpv::new().ok();

        // - передан файл - переходим в его папку и сразу запускаем
        // - передана папка - переходим в неё
        // - ничего не передано - используем папку из конфига
        let mut current_dir = PathBuf::from(&config.download_dir);
        let mut initial_play: Option<PathBuf> = None;
        let mut start_focus = Focus::Results;

        if let Some(first) = initial_files.first() {
            start_focus = Focus::Local;
            if first.is_dir() {
                current_dir = first.clone();
            } else if first.is_file() {
                if let Some(parent) = first.parent() {
                    current_dir = parent.to_path_buf();
                }
                initial_play = Some(first.clone());
            }
        }

        fs::create_dir_all(&current_dir).unwrap_or_default();

        // Канал для получения спектра частот от cava.
        let audio_data = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let audio_clone = std::sync::Arc::clone(&audio_data);
        std::thread::spawn(move || {
            let conf = "[general]\nbars = 32\n[output]\nmethod = raw\nraw_target = /dev/stdout\ndata_format = binary\nbar_delimiter = 0\n";
            std::fs::write("/tmp/cava_fishpot.conf", conf).unwrap_or_default();
            
            if let Ok(mut child) = Command::new("cava")
                .arg("-p").arg("/tmp/cava_fishpot.conf")
                .stdout(std::process::Stdio::piped())
                .stderr(std::process::Stdio::null())
                .spawn() 
            {
                let mut out = child.stdout.take().unwrap();
                let mut buf = [0u8; 32];
                loop {
                    use std::io::Read;
                    if out.read_exact(&mut buf).is_ok() { 
                        let mut d = audio_clone.lock().unwrap(); 
                        d.clear(); 
                        d.extend_from_slice(&buf); 
                    } else { 
                        break; 
                    }
                }
            }
        });

        let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
        let history_path = home.join(".config").join("fishpot").join("history.json");
        let search_history: Vec<String> = fs::read_to_string(history_path)
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default();

        let mut app = Self {
            mode: Mode::Normal, 
            focus: start_focus,           // ← Results по умолчанию, Local если передан файл/папка
            input: String::new(),
            results: Vec::new(), selected: 0,
            local_items: Vec::new(), local_selected: 0, 
            current_dir,                  // ← уже вычисленная (файл / папка / дефолт)
            current: None, status: "Ready".into(), volume: 100, paused: false, 
            show_help: false, time_pos: 0.0, duration: 0.0,
            mpv, search_rx: None, import_rx: None, download_rx: None,
            tick: 0, shuffle: false,
            viz_time: 0.0, viz_mode: config.viz_mode, audio_data,
            search_history, history_idx: None,
            show_lyrics: false, lyrics_data: None, lyrics_scroll: None, lyrics_rx: None,
            show_results: true,
            fullscreen_viz: false,
            repeat_mode: RepeatMode::None,
            last_eof: false,
            show_eq: false,
            download_percent: None,
            config,
            local_filter: String::new(),
            rename_input: String::new(),
            file_to_rename: None,
            confirm_delete: None,
            ambient_effect,
            ambient_particles: Vec::new(),
        };
        app.refresh_local();

        // Если передан файл — выделяем его в списке и запускаем
        if let Some(path) = initial_play {
            let title = path.file_stem().unwrap_or_default().to_string_lossy().to_string();
            let track = Track {
                id: title.clone(),
                title,
                duration: Some(0.0),
                local_path: Some(path.to_string_lossy().to_string()),
            };

            // Ставим курсор на этот файл в левом списке
            for (i, item) in app.local_items.iter().enumerate() {
                if let LocalItem::File(t) = item {
                    if t.local_path.as_deref() == path.to_str() {
                        app.local_selected = i;
                        break;
                    }
                }
            }

            let _ = app.play_track(track);
        }

        if let Some(mpv) = &app.mpv { let _ = mpv.set_volume(app.volume); }
        app
    }

    /// Перечитывает текущую директорию с учётом фильтра `local_filter`.
    /// В этой шляпе идут папки а потом уже файлы
    pub fn refresh_local(&mut self) {
        self.local_items.clear();
        if let Ok(entries) = fs::read_dir(&self.current_dir) {
            let mut dirs = Vec::new();
            let mut files = Vec::new();
            let filter_lower = self.local_filter.to_lowercase();

            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() { 
                    dirs.push(LocalItem::Dir(path)); 
                } else if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
                    if ["mp3", "webm", "m4a", "flac", "wav", "ogg", "opus", "aac", "wma", "alac", "aiff", "m4b"].contains(&ext) {
                        let title = path.file_stem().unwrap_or_default().to_string_lossy().to_string();
                        if filter_lower.is_empty() || title.to_lowercase().contains(&filter_lower) {
                            files.push(LocalItem::File(Track { 
                                id: title.clone(), 
                                title, 
                                duration: Some(0.0), 
                                local_path: Some(path.to_string_lossy().to_string()) 
                            }));
                        }
                    }
                }
            }
            dirs.sort_by(|a, b| { 
                let a = match a { LocalItem::Dir(p) => p.file_name().unwrap_or_default(), _ => std::ffi::OsStr::new("") }; 
                let b = match b { LocalItem::Dir(p) => p.file_name().unwrap_or_default(), _ => std::ffi::OsStr::new("") }; 
                a.cmp(b) 
            });
            files.sort_by(|a, b| { 
                let a = match a { LocalItem::File(t) => &t.title, _ => "" }; 
                let b = match b { LocalItem::File(t) => &t.title, _ => "" }; 
                a.cmp(b) 
            });
            
            self.local_items.extend(dirs);
            self.local_items.extend(files);
        }
        // Если индекс выбранного элемента вышел за границы после фильтрации — сбрасываем.
        if self.local_selected >= self.local_items.len() {
            self.local_selected = self.local_items.len().saturating_sub(1);
        }
    }

    /// Обрабатывает окончание трека. Вызывается из run(), когда mpv прислал eof.
    fn handle_track_end(&mut self) {
        self.last_eof = true;
        match self.repeat_mode {
            RepeatMode::One => {
                if let Some(t) = &self.current {
                    let _ = self.play_track(t.clone());
                }
            }
            _ => {
                self.play_next_auto();
            }
        }
    }

    /// Автоматически переходит к следующему треку (используется при eof и repeat=All).
    /// Ищет текущий трек по ID, чтобы корректно работало даже если пользователь
    /// прокрутил список в сторону.
    fn play_next_auto(&mut self) {
        let from_local = self.current.as_ref().map(|t| t.local_path.is_some()).unwrap_or(false);

        if from_local {
            let current_id = self.current.as_ref().map(|t| &t.id);
            let pos = self.local_items.iter().position(|item| match item {
                LocalItem::File(t) => Some(&t.id) == current_id,
                _ => false,
            }).unwrap_or(0);

            let mut next_idx = pos + 1;
            if next_idx >= self.local_items.len() {
                if self.repeat_mode == RepeatMode::All { 
                    next_idx = 0; 
                } else { 
                    if let Some(mpv) = &self.mpv { let _ = mpv.set_pause(true); }
                    self.status = "Конец списка".into();
                    return; 
                }
            }
            
            // Пропускаем папки, идём только по файлам.
            let start_idx = next_idx;
            loop {
                if let LocalItem::File(t) = &self.local_items[next_idx] {
                    self.local_selected = next_idx; 
                    let t_clone = t.clone();
                    let _ = self.play_track(t_clone);
                    break;
                }
                next_idx = (next_idx + 1) % self.local_items.len();
                if next_idx == start_idx { break; }
            }
        } else {
            if !self.results.is_empty() {
                let current_id = self.current.as_ref().map(|t| &t.id);
                let pos = self.results.iter().position(|t| Some(&t.id) == current_id).unwrap_or(self.selected);

                let mut next_idx = pos + 1;
                if next_idx >= self.results.len() {
                    if self.repeat_mode == RepeatMode::All { 
                        next_idx = 0; 
                    } else { 
                        if let Some(mpv) = &self.mpv { let _ = mpv.set_pause(true); }
                        self.status = "Конец списка".into();
                        return; 
                    }
                }
                self.selected = next_idx; 
                let t_clone = self.results[next_idx].clone();
                let _ = self.play_track(t_clone);
            }
        }
    }

    pub fn run<B: Backend>(&mut self, terminal: &mut Terminal<B>) -> io::Result<()> {
        loop {
            self.tick += 1;

            // Синхронизируем состояние с mpv.
            if let Some(mpv) = &self.mpv {
                let s = mpv.status.lock().unwrap();
                self.time_pos = s.time_pos; 
                self.duration = s.duration; 
                self.paused = s.paused;
                
                // Трек закончился, если mpv прислал eof-reached ИЛИ время подошло к концу (фолбэк).
                let is_ended = s.eof_reached || (self.duration > 0.0 && self.time_pos >= self.duration - 0.5);
                
                if is_ended && !self.last_eof && !self.paused {
                    drop(s); // Отпускаем мьютекс до вызова handle_track_end, чтобы не было дедлока.
                    self.handle_track_end();
                } else if !is_ended {
                    self.last_eof = false;
                }
            }
            
            if !self.paused && self.current.is_some() { self.viz_time += 0.05; }
            if self.ambient_effect && !self.fullscreen_viz {
                self.update_ambient();
            }

            // Обработка поиска.
            if let Some(rx) = &self.search_rx {
                if let Ok(res) = rx.try_recv() {
                    match res { 
                        Ok(r) => { self.results = r; self.selected = 0; self.status = format!("Найдено: {}", self.results.len()); }, 
                        Err(e) => self.status = e 
                    }
                    self.search_rx = None; self.mode = Mode::Normal;
                }
            }
            
            // Собираем события в Vec, чтобы отпустить borrow self.import_rx
            // до вызова self.handle_download_event (который требует &mut self).
            let mut pending_import_events = Vec::new();
            if let Some(rx) = &self.import_rx {
                while let Ok(ev) = rx.try_recv() {
                    pending_import_events.push(ev);
                }
            }
            for ev in pending_import_events {
                self.handle_download_event(ev, true);
                if self.import_rx.is_none() { break; }
            }

            // То же для одиночного скачивания.
            let mut pending_download_events = Vec::new();
            if let Some((_id, rx)) = &self.download_rx {
                while let Ok(ev) = rx.try_recv() {
                    pending_download_events.push(ev);
                }
            }
            for ev in pending_download_events {
                self.handle_download_event(ev, false);
                if self.download_rx.is_none() { break; }
            }
            
            // Обработка загрузки текста песни.
            if let Some(rx) = &self.lyrics_rx {
                if let Ok(data) = rx.try_recv() { 
                    self.lyrics_data = Some(data); 
                    self.lyrics_rx = None; 
                }
            }

            terminal.draw(|f| ui::draw(f, self))?;

            // Ждём событие 50мс — это даёт ~20 FPS для анимации, но не грузит CPU.
            if event::poll(Duration::from_millis(50))? {
                if let Event::Key(key) = event::read()? {
                    if key.kind != KeyEventKind::Press { continue; }
                    if self.handle_key(key.code)? { return Ok(()); }
                }
            }
        }
    }

    /// Извлекает общий код обработки событий загрузки. 
    /// `is_import == true` — это плейлист, иначе одиночный трек.
    fn handle_download_event(&mut self, event: crate::ytdlp::DownloadEvent, is_import: bool) {
        match event {
            crate::ytdlp::DownloadEvent::Progress(pct) => {
                self.download_percent = Some(pct);
            }
            crate::ytdlp::DownloadEvent::Finished(msg) => {
                if is_import {
                    self.status = msg;
                    Notification::new().summary("Готово").body(&self.status).show().ok();
                } else {
                    self.status = format!("Скачано: {}", msg);
                }
                self.refresh_local();
                if is_import { self.import_rx = None; } else { self.download_rx = None; }
                self.download_percent = None;
            }
            crate::ytdlp::DownloadEvent::Error(e) => {
                self.status = format!("Ошибка: {}", e);
                if is_import { self.import_rx = None; } else { self.download_rx = None; }
                self.download_percent = None;
            }
        }
    }

    /// Обработка нажатия клавиш. Возвращает true, если нужно выйти из приложения.
    fn handle_key(&mut self, code: KeyCode) -> io::Result<bool> {
        // 1. Если открыто окно подтверждения удаления — обрабатываем только y/n.
        if let Some(path) = &self.confirm_delete {
            match code {
                KeyCode::Char('y') | KeyCode::Char('Y') => {
                    let p = path.clone();
                    self.confirm_delete = None;
                    
                    let result = if p.is_dir() {
                        fs::remove_dir_all(&p)
                    } else {
                        fs::remove_file(&p)
                    };

                    if let Err(e) = result {
                        self.status = format!("Ошибка удаления: {}", e);
                    } else {
                        self.status = "Успешно удалено".into();
                        self.refresh_local();
                    }
                    return Ok(false);
                }
                KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Esc => {
                    self.confirm_delete = None;
                    self.status = "Отменено".into();
                    return Ok(false);
                }
                _ => return Ok(false),
            }
        }

        // 2. Обработка режимов ввода (фильтр, переименование).
        if self.mode == Mode::FilteringLocal || self.mode == Mode::RenamingFile {
            match code {
                KeyCode::Enter => {
                    if self.mode == Mode::RenamingFile {
                        if let Some(old_path) = self.file_to_rename.take() {
                            let ext = old_path.extension().and_then(|e| e.to_str()).unwrap_or("");
                            let new_name = if ext.is_empty() { 
                                self.rename_input.clone() 
                            } else { 
                                format!("{}.{}", self.rename_input, ext) 
                            };
                            let new_path = self.current_dir.join(new_name);
                            if let Err(e) = fs::rename(&old_path, &new_path) {
                                self.status = format!("Ошибка: {}", e);
                            } else {
                                self.status = "Переименовано".into();
                            }
                        }
                    }
                    self.mode = Mode::Normal;
                    self.refresh_local();
                }
                KeyCode::Esc => {
                    if self.mode == Mode::FilteringLocal { self.local_filter.clear(); }
                    self.mode = Mode::Normal;
                    self.refresh_local();
                }
                KeyCode::Backspace => {
                    if self.mode == Mode::FilteringLocal {
                        self.local_filter.pop();
                        self.refresh_local();
                    } else {
                        self.rename_input.pop();
                    }
                }
                KeyCode::Char(c) => {
                    if self.mode == Mode::FilteringLocal {
                        self.local_filter.push(c);
                        self.refresh_local();
                    } else {
                        self.rename_input.push(c);
                    }
                }
                _ => {}
            }
            return Ok(false);
        }

        // 3. Всплывающие окна — любая клавиша закрывает их.
        if self.show_help { self.show_help = false; return Ok(false); }

        if self.show_eq {
            match code {
                KeyCode::Esc | KeyCode::Char('e') | KeyCode::Char('E') => { self.show_eq = false; return Ok(false); }
                _ => {} 
            }
        }

        if self.show_lyrics {
            match code {
                KeyCode::Esc | KeyCode::Char('L') => { self.show_lyrics = false; return Ok(false); }
                KeyCode::Up | KeyCode::Char('k') => { 
                    self.lyrics_scroll = Some(self.lyrics_scroll.unwrap_or(0).saturating_sub(1)); 
                    return Ok(false); 
                }
                KeyCode::Down | KeyCode::Char('j') => { 
                    self.lyrics_scroll = Some(self.lyrics_scroll.unwrap_or(0) + 1); 
                    return Ok(false); 
                }
                KeyCode::Char('a') => { 
                    self.lyrics_scroll = None; 
                    return Ok(false); 
                }
                _ => {} 
            }
        }

        // 4. Обработка строки ввода (поиск, перемотка и т.д.).
        if self.mode != Mode::Normal {
            match code {
                KeyCode::Enter => {
                    let input = self.input.trim().to_string();
                    if !input.is_empty() {
                        match self.mode {
                            Mode::Searching => {
                                if !self.search_history.contains(&input) {
                                    self.search_history.insert(0, input.clone());
                                    if self.search_history.len() > 20 { self.search_history.truncate(20); }
                                    let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
                                    let path = home.join(".config").join("yt-music-tui").join("history.json");
                                    if let Some(parent) = path.parent() {
                                        let _ = fs::create_dir_all(parent);
                                    }
                                    let _ = fs::write(path, serde_json::to_string(&self.search_history).unwrap_or_default());
                                }
                                self.status = "Поиск YouTube...".into();
                                let (tx, rx) = mpsc::channel();
                                std::thread::spawn(move || { let _ = tx.send(ytdlp::search(&input, 20)); });
                                self.search_rx = Some(rx);
                            }
                            Mode::Seeking => {
                                if let Some(secs) = parse_time(&input) {
                                    if secs >= 0.0 && secs <= self.duration { 
                                        if let Some(mpv) = &self.mpv { let _ = mpv.seek(secs); } 
                                    }
                                }
                                self.mode = Mode::Normal;
                            }
                            Mode::Importing => {
                                self.status = "Скачивание плейлиста...".into();
                                let (tx, rx) = mpsc::channel();
                                let dir = self.current_dir.clone();
                                std::thread::spawn(move || { ytdlp::download_playlist(&input, &dir, tx); });
                                self.import_rx = Some(rx);
                                self.download_percent = Some(0.0);
                            }
                            Mode::CreatingDir => {
                                let new_dir = self.current_dir.join(&input);
                                fs::create_dir_all(&new_dir).unwrap_or_default();
                                self.status = format!("Папка '{}' создана", input);
                                self.refresh_local();
                                self.mode = Mode::Normal;
                            }
                            _ => {}
                        }
                    } else { self.mode = Mode::Normal; }
                }
                KeyCode::Esc => self.mode = Mode::Normal,
                KeyCode::Up if self.mode == Mode::Searching => {
                    if !self.search_history.is_empty() {
                        let idx = match self.history_idx { 
                            Some(i) => i.saturating_sub(1), 
                            None => self.search_history.len() - 1 
                        };
                        self.history_idx = Some(idx);
                        self.input = self.search_history[idx].clone();
                    }
                }
                KeyCode::Down if self.mode == Mode::Searching => {
                    if let Some(idx) = self.history_idx {
                        if idx + 1 < self.search_history.len() {
                            self.history_idx = Some(idx + 1);
                            self.input = self.search_history[idx + 1].clone();
                        } else { 
                            self.history_idx = None; 
                            self.input.clear(); 
                        }
                    }
                }
                KeyCode::Char(c) => { self.input.push(c); self.history_idx = None; }
                KeyCode::Backspace => { self.input.pop(); }
                _ => {}
            }
            return Ok(false);
        }

        // 5. Основные горячие клавиши.
        match code {
            // Локальные действия (приоритетнее, если фокус на папке).
            KeyCode::Char('r') | KeyCode::Char('R') if self.focus == Focus::Local => {
                if let Some(LocalItem::File(t)) = self.local_items.get(self.local_selected) {
                    self.mode = Mode::RenamingFile;
                    self.file_to_rename = Some(PathBuf::from(t.local_path.as_ref().unwrap()));
                    self.rename_input = t.title.clone();
                }
            }
            KeyCode::Char('d') | KeyCode::Char('D') if self.focus == Focus::Local => {
                if let Some(item) = self.local_items.get(self.local_selected) {
                    match item {
                        LocalItem::File(t) => {
                            self.confirm_delete = Some(PathBuf::from(t.local_path.as_ref().unwrap()));
                            self.status = "Удалить файл? (y/n)".into();
                        }
                        LocalItem::Dir(p) => {
                            self.confirm_delete = Some(p.clone());
                            self.status = "Удалить папку и всё её содержимое? (y/n)".into();
                        }
                    }
                }
            }
            KeyCode::Char('f') | KeyCode::Char('F') if self.focus == Focus::Local => {
                self.mode = Mode::FilteringLocal;
                self.local_filter.clear();
                self.refresh_local();
                self.status = "Фильтр файлов...".into();
            }

            // Глобальные действия.
            KeyCode::Char('q') => return Ok(true),
            KeyCode::Char('?') => self.show_help = true,
            KeyCode::F(5) => { self.refresh_local(); self.status = "Обновлено".into(); }
            KeyCode::Char('/') => { self.mode = Mode::Searching; self.input.clear(); self.history_idx = None; }
            KeyCode::Char('s') => { self.mode = Mode::Seeking; self.input.clear(); }
            KeyCode::Char('i') => { self.mode = Mode::Importing; self.input.clear(); }
            KeyCode::Char('m') => { self.mode = Mode::CreatingDir; self.input.clear(); }
            KeyCode::Char('r') => { self.shuffle = !self.shuffle; self.status = format!("Shuffle: {}", self.shuffle); }
            KeyCode::Char('V') => { 
                self.viz_mode = (self.viz_mode + 1) % 8; 
                self.status = format!("Визуализация: {}", self.viz_mode); 
            }

            KeyCode::Char('A') => {
                self.ambient_effect = !self.ambient_effect;
                self.status = format!("Ambient: {}", if self.ambient_effect { "Вкл" } else { "Выкл" });
            }

            KeyCode::Tab => self.focus = self.focus.next(),
            KeyCode::Char('p') | KeyCode::Char(' ') => { 
                if let Some(mpv) = &self.mpv { let _ = mpv.set_pause(!self.paused); } 
            }
            KeyCode::Left | KeyCode::Char('h') => { 
                if let Some(mpv) = &self.mpv { let _ = mpv.seek_rel(-5); } 
            }
            KeyCode::Right | KeyCode::Char('l') => { 
                if let Some(mpv) = &self.mpv { let _ = mpv.seek_rel(5); } 
            }
            KeyCode::Char('+') | KeyCode::Char('=') => { 
                self.volume = (self.volume + 5).min(150); 
                if let Some(mpv) = &self.mpv { let _ = mpv.set_volume(self.volume); } 
            }
            KeyCode::Char('-') => { 
                self.volume = (self.volume - 5).max(0); 
                if let Some(mpv) = &self.mpv { let _ = mpv.set_volume(self.volume); } 
            }
            KeyCode::Char('n') => self.play_next()?,
            KeyCode::Char('e') | KeyCode::Char('E') => { self.show_eq = !self.show_eq; }
            KeyCode::Char('L') => { self.show_lyrics = !self.show_lyrics; self.lyrics_scroll = None; }
            
            KeyCode::Char('D') => {
                if self.focus == Focus::Results && self.download_rx.is_none() {
                    if let Some(t) = self.results.get(self.selected).cloned() {
                        if t.local_path.is_none() {
                            let track_id = t.id.clone();
                            let dir = self.current_dir.clone();
                            self.status = format!("Скачивание: {}...", t.title);
                            let (tx, rx) = mpsc::channel();
                            std::thread::spawn(move || { ytdlp::download(&t, &dir, tx); });
                            self.download_rx = Some((track_id, rx));
                        }
                    }
                }
            }
            KeyCode::Backspace if self.focus == Focus::Local => {
                if let Some(parent) = self.current_dir.parent() { 
                    self.current_dir = parent.to_path_buf(); 
                    self.refresh_local(); 
                }
            }
            KeyCode::Char('y') | KeyCode::Char('Y') => { 
                self.show_results = !self.show_results; 
                self.status = format!("Панель YouTube: {}", if self.show_results { "Вкл" } else { "Выкл" });
            }
            KeyCode::Char('f') | KeyCode::Char('F') => { 
                self.fullscreen_viz = !self.fullscreen_viz; 
                self.status = format!("Полный экран: {}", if self.fullscreen_viz { "Вкл" } else { "Выкл" });
            }
            KeyCode::Char('R') => {
                self.repeat_mode = match self.repeat_mode {
                    RepeatMode::None => RepeatMode::All,
                    RepeatMode::All => RepeatMode::One,
                    RepeatMode::One => RepeatMode::None,
                };
                self.status = format!("Повтор: {:?}", self.repeat_mode);
            }
            _ => self.handle_list(code)?,
        }
        Ok(false)
    }

    /// Обработка навигации по спискам (стрелки, Enter).
    fn handle_list(&mut self, code: KeyCode) -> io::Result<()> {
        match code {
            KeyCode::Down | KeyCode::Char('j') => {
                match self.focus {
                    Focus::Results => if !self.results.is_empty() { 
                        self.selected = (self.selected + 1) % self.results.len(); 
                    },
                    Focus::Local => if !self.local_items.is_empty() { 
                        self.local_selected = (self.local_selected + 1) % self.local_items.len(); 
                    },
                }
            }
            KeyCode::Up | KeyCode::Char('k') => {
                match self.focus {
                    Focus::Results => self.selected = self.selected.saturating_sub(1),
                    Focus::Local => self.local_selected = self.local_selected.saturating_sub(1),
                }
            }
            KeyCode::Enter => {
                if self.focus == Focus::Results {
                    if let Some(t) = self.results.get(self.selected).cloned() { self.play_track(t)?; }
                } else {
                    if let Some(item) = self.local_items.get(self.local_selected) {
                        match item {
                            LocalItem::Dir(path) => { 
                                self.current_dir = path.clone(); 
                                self.refresh_local(); 
                            }
                            LocalItem::File(track) => self.play_track(track.clone())?,
                        }
                    }
                }
            }
            _ => {}
        }
        Ok(())
    }

    /// Запускает воспроизведение трека и параллельно подгружает текст песни.
    fn play_track(&mut self, t: Track) -> io::Result<()> {
        if let Some(mpv) = &self.mpv {
            mpv.play(&t.play_target())?;
            Notification::new().summary("Сейчас играет").body(&t.title).show().ok();
        }
        self.current = Some(t.clone());
        
        // Сбрасываем старый текст, чтобы UI показал "Загрузка..." вместо устаревших строк.
        self.lyrics_data = None;
        self.lyrics_scroll = None; 
        
        // Всегда грузим текст асинхронно — пользователь откроет его по L.
        let title = t.title.clone();
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || { 
            let _ = tx.send(api::fetch_lyrics(&title)
                .unwrap_or_else(|_| vec![(-1.0, "Текст не найден".to_string())])); 
        });
        self.lyrics_rx = Some(rx);
        
        Ok(())
    }

    /// Переключает на следующий трек вручную (по клавише n).
    /// Если включён shuffle — выбирает случайный индекс.
    fn play_next(&mut self) -> io::Result<()> {
        if self.focus == Focus::Results && !self.results.is_empty() {
            let mut idx = if self.shuffle { 
                rand::random::<usize>() % self.results.len() 
            } else { 
                (self.selected + 1) % self.results.len() 
            };
            if idx == self.selected && self.results.len() > 1 { 
                idx = (idx + 1) % self.results.len(); 
            }
            self.selected = idx;
            self.play_track(self.results[idx].clone())?;
        } else if self.focus == Focus::Local && !self.local_items.is_empty() {
            let mut idx = if self.shuffle { 
                rand::random::<usize>() % self.local_items.len() 
            } else { 
                (self.local_selected + 1) % self.local_items.len() 
            };
            if idx == self.local_selected && self.local_items.len() > 1 { 
                idx = (idx + 1) % self.local_items.len(); 
            }
            self.local_selected = idx;
            if let LocalItem::File(t) = &self.local_items[idx] {
                let t_clone = t.clone();
                self.play_track(t_clone)?;
            }
        }
        Ok(())
    }
    
    /// Обновляет физику частиц и спавнит новые по басу/энергии.
    fn update_ambient(&mut self) {
        let (bass, energy) = {
            let data = self.audio_data.lock().unwrap();
            if data.len() == 32 {
                let b = data[0..8].iter().map(|&x| x as f64 / 255.0).sum::<f64>() / 8.0;
                let e = data.iter().map(|&x| x as f64 / 255.0).sum::<f64>() / 32.0;
                (b, e)
            } else {
                let t = self.viz_time;
                ((t * 5.0).sin() * 0.3 + 0.4, 0.5)
            }
        };

        let spawn = (1.0 + bass * 8.0 + energy * 4.0) as usize;
        for _ in 0..spawn {
            self.ambient_particles.push(Particle::spawn(bass));
        }

        for p in &mut self.ambient_particles {
            p.x += p.vx;
            p.y += p.vy;
            p.phase += 0.02; 
            
            // Легкое покачивание (волна)
            p.vy += (p.phase).sin() * 0.0001;

            p.life -= p.decay;

            // Если улетела за левый край или умерла — она удалится
        }

        self.ambient_particles.retain(|p| p.life > 0.0 && p.x > -0.1);

        let cap = self.config.theme.ambient_count.max(50);
        if self.ambient_particles.len() > cap {
            self.ambient_particles.drain(0..self.ambient_particles.len() - cap);
        }
    }

}

/// Парсит время из строки. Поддерживает форматы `mm:ss` и просто секунды.
fn parse_time(input: &str) -> Option<f64> {
    let parts: Vec<&str> = input.split(':').collect();
    match parts.len() { 
        1 => parts[0].parse::<f64>().ok(), 
        2 => Some(parts[0].parse::<f64>().ok()? * 60.0 + parts[1].parse::<f64>().ok()?), 
        _ => None 
    }
}