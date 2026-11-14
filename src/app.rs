// =====================================================================
// Основной файл со всей логикой
// =====================================================================

// =====================================================================
// ИМПОРТЫ
// =====================================================================

// Стандартная библиотека
use std::{
    fs,
    io,
    path::PathBuf,
    process::Command,
    sync::mpsc,
    time::Duration,
};

// Сторонние крейты
use crossterm::event::{self, Event, KeyCode, KeyEventKind};
use notify_rust::Notification;
use ratatui::{backend::Backend, Terminal};
use serde::{Deserialize, Serialize};

// Внутренние модули проекта
use crate::{
    api,
    config::AppConfig,
    mpv::Mpv,
    ui,
    ytdlp::{self, Track},
};

// =====================================================================
// СТРУКТУРЫ ДЛЯ ЗАКЛАДОК
// =====================================================================

/// Узел дерева закладок. Может содержать вложенные папки и треки
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct BookmarkFolder {
    pub name: String,
    pub folders: Vec<BookmarkFolder>,
    pub tracks: Vec<Track>,
}

/// Обёртка над элементом закладок для удобной работы в UI
/// Хранит не сам объект, а индекс внутри текущей папки, 
#[derive(Clone)]
pub enum SavedItem {
    Folder { index: usize, name: String },
    Track { index: usize, track: Track },
}

// =====================================================================
// СТРУКТУРЫ ДЛЯ CHAOS MODE (5 случайных треков)
// =====================================================================

/// Параметры трека, который играет в режиме хаоса
/// Сохраняются для отображения в окне информации (какой питч, реверс и т.д.)
#[derive(Clone)]
pub struct ChaosTrack {
    pub title: String,
    pub pitch: f64,
    pub reversed: bool,
    pub start_offset: f64,
}

// =====================================================================
// СТРУКТУРЫ ДЛЯ AMBIENT-ЭФФЕКТА (частицы)
// =====================================================================

#[derive(Clone)]
pub struct Particle {
    pub x: f64,        // Нормализованная позиция 0.0..1.0 по X
    pub y: f64,        // Нормализованная позиция 0.0..1.0 по Y
    pub vx: f64,       // Скорость по X (за кадр)
    pub vy: f64,       // Скорость по Y (за кадр)
    pub life: f64,     // 1.0 = только родилась, 0.0 = умерла
    pub decay: f64,    // Насколько уменьшается life за кадр
    pub ch: char,      // Символ (точка, звёздочка, кружок)
    pub phase: f64,    // Фаза для мерцания (синусоида)
}

impl Particle {
    pub fn spawn(energy: f64) -> Self {
        let r = || {
            use std::sync::atomic::{AtomicU64, Ordering};
            static STATE: AtomicU64 = AtomicU64::new(0x9E3779B97F4A7C15);
            let x = STATE.fetch_add(0x9E3779B97F4A7C15, Ordering::Relaxed);
            let x = x.wrapping_mul(0x9E3779B97F4A7C15) ^ (x >> 32);
            (x >> 11) as f64 / (1u64 << 53) as f64
        };

        let chars = ['·', '∙', '⋅', '•', '°', '○', '｡'];
        let ch = chars[(r() * chars.len() as f64) as usize % chars.len()];

        let x = 1.0 + r() * 0.1;
        let y = r(); 
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

// =====================================================================
// ENUM'Ы ДЛЯ УПРАВЛЕНИЯ
// =====================================================================

/// Режим повтора трека:
/// - None: по окончании трека ничего не включается (если плейлист кончился)
/// - All: по окончании трека зацикливаем весь список
/// - One: зацикливаем текущий трек
#[derive(PartialEq, Clone, Copy, Debug)]
pub enum RepeatMode { None, All, One }

/// Глобальный режим работы приложения.
/// Определяет, что происходит при нажатии клавиш: обычная навигация или ввод текста.
#[derive(PartialEq, Clone, Copy)]
pub enum Mode {
    Normal,           // Обычное управление (горячие клавиши)
    Searching,        // Ввод поискового запроса
    Seeking,          // Ввод времени для перемотки (mm:ss)
    Importing,        // Ввод URL плейлиста
    CreatingDir,      // Ввод имени новой локальной папки
    FilteringLocal,   // Ввод фильтра для локальных файлов
    RenamingFile,     // Ввод нового имени локального файла
    CreatingBookmarkFolder,  // Ввод имени новой папки в закладках
    RenamingBookmarkFolder,  // Ввод нового имени папки в закладках
}

// =====================================================================
// ФОКУС И НАВИГАЦИЯ
// =====================================================================

/// Какая панель сейчас в фокусе. Tab переключает между ними
#[derive(PartialEq, Clone, Copy)]
pub enum Focus { Results, Local, Saved }

impl Focus {
    /// Возвращает следующую панель
    fn next(self, show_res: bool, show_loc: bool, show_sav: bool) -> Self {
        let mut current = self;
        for _ in 0..3 {
            current = match current {
                Focus::Local => Focus::Results,    
                Focus::Results => Focus::Saved,    
                Focus::Saved => Focus::Local,      
            };
            let is_visible = match current {
                Focus::Results => show_res,
                Focus::Local => show_loc,
                Focus::Saved => show_sav,
            };
            if is_visible {
                return current;
            }
        }
        self // Если все скрыты, оставляем как есть
    }
}

// =====================================================================
// ЛОКАЛЬНАЯ БИБЛИОТЕКА
// =====================================================================

/// Элемент локальной папки: либо подпапка, либо аудиофайл
#[derive(Debug)]
pub enum LocalItem { Dir(PathBuf), File(Track) }

// =====================================================================
// Основная структура
// =====================================================================
pub struct App {
    // =====================================================================
    // 1. СОСТОЯНИЕ ИНТЕРФЕЙСА
    // =====================================================================
    pub mode: Mode,               // Что сейчас делает пользователь (обычный режим, поиск, ввод и т.д.)
    pub focus: Focus,             // Какая панель в фокусе (Local / Results / Saved)
    pub input: String,            // Буфер строки ввода (для поиска, перемотки, создания папки)
    pub status: String,           // Текст в нижнем статус-баре ("Поиск...", "Скачано: ...", и т.д.)
    pub show_help: bool,          // Открыто ли окно справки (?)
    pub tick: usize,              // Счётчик кадров (для анимации спиннера загрузки)

    // Видимость панелей (переключается клавишами 1, 2, 3)
    pub show_results: bool,       // Панель YouTube включена?
    pub show_local: bool,         // Панель Local включена?
    pub show_saved: bool,         // Панель Закладки включена?
    pub fullscreen_viz: bool,     // Полноэкранный режим визуализатора (F)

    // =====================================================================
    // 2. ВОСПРОИЗВЕДЕНИЕ
    // =====================================================================
    pub current: Option<Track>,   // Трек, который играет прямо сейчас (для подсветки в списках)
    pub paused: bool,             // На паузе?
    pub volume: i64,              // Громкость 0..150
    pub time_pos: f64,            // Текущая позиция в секундах (синхронизируется из mpv)
    pub duration: f64,            // Длительность текущего трека (синхронизируется из mpv)
    pub repeat_mode: RepeatMode,  // Режим повтора: None / All / One
    pub shuffle: bool,            // Случайный порядок (Shuffle)
    pub last_eof: bool,           // Флаг, что предыдущий трек уже завершился (чтобы не было двойного триггера)
    pub mpv: Option<Mpv>,         // Обёртка над mpv (None, если mpv не запустился)

    // =====================================================================
    // 3. ПОИСК НА YOUTUBE
    // =====================================================================
    pub results: Vec<Track>,      // Результаты последнего поиска
    pub selected: usize,          // Индекс выбранного элемента в results
    // =====================================================================
    // 4. ЛОКАЛЬНАЯ БИБЛИОТЕКА
    // =====================================================================
    pub current_dir: PathBuf,             // Текущая открытая папка
    pub local_items: Vec<LocalItem>,      // Содержимое папки (подпапки + файлы)
    pub local_selected: usize,            // Индекс выбранного элемента
    pub local_filter: String,             // Текст фильтра (когда нажата 'f')
    pub rename_input: String,             // Новое имя файла (в режиме переименования)
    pub file_to_rename: Option<PathBuf>,  // Какой файл сейчас переименовываем
    pub confirm_delete: Option<PathBuf>,  // Ожидает ли подтверждения удаление файла/папки

    // =====================================================================
    // 5. ЗАКЛАДКИ (ДЕРЕВО)
    // =====================================================================
    pub saved_tree: BookmarkFolder,           // Всё дерево закладок (папки + треки)
    pub saved_path: Vec<usize>,               // Стек индексов: где мы сейчас внутри дерева
    pub saved_selected: usize,                // Индекс выбранного элемента в текущей папке
    pub saved_file_path: PathBuf,             // Путь к bookmarks.json (для автосохранения)
    pub bookmark_index_to_rename: Option<usize>,   // Индекс папки для переименования
    pub confirm_delete_bookmark: Option<SavedItem>, // Ожидает ли подтверждения удаление закладки

    // =====================================================================
    // 6. КАРАОКЕ (ТЕКСТ ПЕСНИ)
    // =====================================================================
    pub show_lyrics: bool,                          // Открыто ли окно текста
    pub lyrics_data: Option<Vec<(f64, String)>>,    // Распарсенный текст: (время, строка)
    pub lyrics_scroll: Option<u16>,                 // Ручная прокрутка (None = авто-скролл)

    // =====================================================================
    // 7. ВИЗУАЛИЗАТОР + AMBIENT
    // =====================================================================
    pub viz_mode: usize,             // Номер текущего визуализатора 
    pub viz_time: f64,               // Время анимации (растёт каждый кадр)
    pub audio_data: std::sync::Arc<std::sync::Mutex<Vec<u8>>>,  // Спектр от cava (32 байта)
    pub ambient_effect: bool,        // Включён ли ambient-эффект 
    pub ambient_particles: Vec<Particle>,  // Летающие частицы-искры

    // =====================================================================
    // 8. CHAOS MODE (5 треков одновременно)
    // =====================================================================
    pub chaos_mode: bool,                        // Активен ли режим хаоса
    pub show_chaos_info: bool,                   // Показано ли окно с информацией
    pub chaos_tracks: Vec<ChaosTrack>,           // Что играет сейчас (для отображения)
    pub chaos_start: Option<std::time::Instant>, // Момент запуска (для таймера)

    // =====================================================================
    // 9. ФОНОВЫЕ ЗАДАЧИ (каналы mpsc)
    // =====================================================================
    pub search_rx: Option<mpsc::Receiver<Result<Vec<Track>, String>>>,  // Результаты поиска
    pub import_rx: Option<mpsc::Receiver<crate::ytdlp::DownloadEvent>>, // Прогресс импорта плейлиста
    pub download_rx: Option<(String, mpsc::Receiver<crate::ytdlp::DownloadEvent>)>, // Прогресс скачивания трека
    pub lyrics_rx: Option<mpsc::Receiver<Vec<(f64, String)>>>,          // Загрузка текста
    pub download_percent: Option<f64>,           // Текущий % скачивания (для статус-бара)

    // =====================================================================
    // 10. КОНФИГ
    // =====================================================================
    pub config: AppConfig,           // Настройки из ~/.config/fishpot/config.toml
}

impl App {
   pub fn new(initial_files: Vec<PathBuf>) -> Self {
        // =================================================================
        // 1. ЗАГРУЗКА КОНФИГА И ЗАВИСИМОСТЕЙ
        // =================================================================
        let config = AppConfig::load();
        let ambient_effect = config.ambient_effect;
        let mpv = Mpv::new().ok(); // Если mpv не запустится, плеер просто не будет играть

        // =================================================================
        // 2. ОБРАБОТКА АРГУМЕНТОВ КОМАНДНОЙ СТРОКИ 
        // =================================================================
        // По умолчанию открываем папку из конфига, фокус на Local
        let mut current_dir = PathBuf::from(&config.download_dir);
        let mut initial_play: Option<PathBuf> = None;
        let start_focus = Focus::Local;

        if let Some(first) = initial_files.first() {
            if first.is_dir() { current_dir = first.clone(); } 
            else if first.is_file() {
                if let Some(parent) = first.parent() {
                    current_dir = parent.to_path_buf();
                }
                initial_play = Some(first.clone());
            }
        }

        // Создаём директорию, если её нет 
        fs::create_dir_all(&current_dir).unwrap_or_default();

        // =================================================================
        // 3. ЗАПУСК CAVA 
        // =================================================================
        let audio_data = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let audio_clone = std::sync::Arc::clone(&audio_data);
        
        std::thread::spawn(move || {
            let conf = "[general]\nbars = 32\n[output]\nmethod = raw\nraw_target = /dev/stdout\ndata_format = binary\nbar_delimiter = 0\n";
            let _ = fs::write("/tmp/cava_fishpot.conf", conf);
            
            if let Ok(mut child) = Command::new("cava")
                .arg("-p").arg("/tmp/cava_fishpot.conf")
                .stdout(std::process::Stdio::piped())
                .stderr(std::process::Stdio::null()) // Скрываем ошибки cava, чтобы не ломать TUI
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
                        break; // Cava упала или была убита
                    }
                }
            }
        });

        // =================================================================
        // 4. ЗАГРУЗКА СОХРАНЁННЫХ ДАННЫХ (История и Закладки)
        // =================================================================
        let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
        
        // Дерево закладок (папки + треки)
        let saved_file_path = home.join(".config").join("fishpot").join("bookmarks.json");
        let saved_tree: BookmarkFolder = fs::read_to_string(&saved_file_path)
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_else(|| BookmarkFolder {
                name: "Root".to_string(),
                folders: Vec::new(),
                tracks: Vec::new(),
            });

        // =================================================================
        // 5. СОЗДАНИЕ СТРУКТУРЫ APP
        // =================================================================
        let mut app = Self {
            // --- 1. Состояние интерфейса ---
            mode: Mode::Normal,               // Начинаем в обычном режиме
            focus: start_focus,               // Фокус на Local (или на переданном файле)
            input: String::new(),             // Пустая строка ввода
            status: "Ready".into(),           // Стартовый текст в статус-баре
            show_help: false,                 // Окно справки закрыто
            tick: 0,                          // Счётчик кадров = 0
            show_results: true,               // YouTube включён
            show_local: true,                 // Local включён
            show_saved: false,                // Закладки пока скрыты (чтобы не загромождать экран)
            fullscreen_viz: false,            // Не в полном экране

            // --- 2. Воспроизведение ---
            current: None,                    // Ничего не играет
            paused: false,                    // Не на паузе
            volume: 100,                      // Громкость 100%
            time_pos: 0.0,                    // Позиция 0
            duration: 0.0,                    // Длительность 0
            repeat_mode: RepeatMode::None,    // Повтор выключен
            shuffle: false,                   // Shuffle выключен
            last_eof: false,                  // Трек ещё не заканчивался
            mpv,                              // Оборачиваем mpv (может быть None, если не запустился)

            // --- 3. Поиск на YouTube ---
            results: Vec::new(),              // Пустые результаты поиска
            selected: 0,                      // Курсор на первом элементе

            // --- 4. Локальная библиотека ---
            current_dir,                      // Путь из конфига или CLI
            local_items: Vec::new(),          // Список файлов загрузится позже (refresh_local)
            local_selected: 0,                // Курсор на первом элементе
            local_filter: String::new(),      // Фильтр пустой
            rename_input: String::new(),      // Буфер переименования пустой
            file_to_rename: None,             // Ничего не переименовываем
            confirm_delete: None,             // Нет запроса на удаление

            // --- 5. Закладки ---
            saved_tree,                       // Дерево загружено из bookmarks.json
            saved_path: Vec::new(),           // Находимся в корне дерева
            saved_selected: 0,                // Курсор на первом элементе
            saved_file_path,                  // Путь к bookmarks.json (для сохранения)
            bookmark_index_to_rename: None,   // Ничего не переименовываем
            confirm_delete_bookmark: None,    // Нет запроса на удаление

            // --- 6. Караоке ---
            show_lyrics: false,               // Окно текста закрыто
            lyrics_data: None,                // Текст не загружен
            lyrics_scroll: None,              // Авто-скролл включён
            lyrics_rx: None,                  // Нет активной загрузки текста

            // --- 7. Визуализатор + Ambient ---
            viz_mode: config.viz_mode,        // Визуализатор из конфига (обычно 0)
            viz_time: 0.0,                    // Время анимации начинается с 0
            audio_data,                       // Arc<Mutex<Vec<u8>>> — общий буфер для cava
            ambient_effect,                   // Включён ли ambient (из конфига)
            ambient_particles: Vec::new(),    // Частиц ещё нет

            // --- 8. Chaos Mode ---
            chaos_mode: false,                // Режим хаоса выключен
            show_chaos_info: false,           // Окно с информацией закрыто
            chaos_tracks: Vec::new(),         // Нет играющих треков
            chaos_start: None,                // Таймер не запущен

            // --- 9. Фоновые задачи (каналы mpsc) ---
            search_rx: None,                  // Нет активного поиска
            import_rx: None,                  // Нет активного импорта
            download_rx: None,                // Нет активного скачивания
            download_percent: None,           // Прогресс неизвестен

            // --- 10. Конфиг ---
            config,                           // Настройки из config.toml
        };
        // Загружаем файлы из текущей директории
        app.refresh_local();

        // =================================================================
        // 6. АВТОЗАПУСК (Если файл был передан в CLI)
        // =================================================================
        if let Some(path) = initial_play {
            let title = path.file_stem().unwrap_or_default().to_string_lossy().to_string();
            let track = Track {
                id: title.clone(),
                title,
                duration: Some(0.0),
                local_path: Some(path.to_string_lossy().to_string()),
            };

            // Ставим курсор на этот файл в левом списке (чтобы было видно, что играет)
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

        // Устанавливаем начальную громкость в mpv
        if let Some(mpv) = &app.mpv { 
            let _ = mpv.set_volume(app.volume); 
        }
        
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

    /// Обработка нажатия клавиш. Возвращает true, если нужно выйти из приложения
    fn handle_key(&mut self, code: KeyCode) -> io::Result<bool> {
        // =====================================================================
        // УРОВЕНЬ 1: ОКНА ПОДТВЕРЖДЕНИЯ (Блокируют всё остальное)
        // Пока висит окно "Удалить? (y/n)", никакие другие клавиши не работают
        // =====================================================================
        
        //Для локальных файлов
        if let Some(path) = &self.confirm_delete {
            match code {
                KeyCode::Char('y') | KeyCode::Char('Y') => {
                    let p = path.clone();
                    self.confirm_delete = None;
                    
                    let result = if p.is_dir() { fs::remove_dir_all(&p) } else { fs::remove_file(&p) };
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
                _ => return Ok(false), // Игнорируем другие клавиши
            }
        }

        //Для закладок
        if let Some(item) = &self.confirm_delete_bookmark {
            match code {
                KeyCode::Char('y') | KeyCode::Char('Y') => {
                    let item = item.clone();
                    self.confirm_delete_bookmark = None;
                    
                    match item {
                        SavedItem::Folder { index, .. } => { self.get_current_folder_mut().folders.remove(index); }
                        SavedItem::Track { index, .. } => { self.get_current_folder_mut().tracks.remove(index); }
                    }
                    self.save_bookmarks();
                    
                    // Корректируем курсор, если удалили последний элемент в списке
                    let n = self.get_saved_items().len();
                    if self.saved_selected >= n && n > 0 {
                        self.saved_selected = n - 1;
                    }
                    self.status = "Удалено из закладок".into();
                    return Ok(false);
                }
                KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Esc => {
                    self.confirm_delete_bookmark = None;
                    self.status = "Отменено".into();
                    return Ok(false);
                }
                _ => return Ok(false),
            }
        }

        // =====================================================================
        // УРОВЕНЬ 2: ВСПЛЫВАЮЩИЕ ОКНА (Справка, Текст, Chaos)
        // Они перехватывают управление, чтобы буквы не срабатывали как команды.
        // =====================================================================
        if self.show_chaos_info {
            if code == KeyCode::Esc { 
                self.show_chaos_info = false; 
                return Ok(false); 
            }
        }

        if self.show_help { 
            self.show_help = false; 
            return Ok(false); 
        }

        if self.show_lyrics {
            match code {
                KeyCode::Esc | KeyCode::Char('T') => { self.show_lyrics = false; return Ok(false); }
                KeyCode::Up | KeyCode::Char('k') => { self.lyrics_scroll = Some(self.lyrics_scroll.unwrap_or(0).saturating_sub(1)); return Ok(false); }
                KeyCode::Down | KeyCode::Char('j') => { self.lyrics_scroll = Some(self.lyrics_scroll.unwrap_or(0) + 1); return Ok(false); }
                KeyCode::Char('a') => { self.lyrics_scroll = None; return Ok(false); }
                _ => {} 
            }
        }

        // =====================================================================
        // УРОВЕНЬ 3: РЕЖИМЫ ВВОДА (Поиск, перемотка, переименование и т.д.)
        // Если мы печатаем текст, глобальные горячие клавиши не должны работать.
        // =====================================================================
        if self.mode != Mode::Normal {
            match code {
                KeyCode::Enter => {
                    let input = self.input.trim().to_string();
                    if !input.is_empty() {
                        match self.mode {
                            Mode::Searching => {
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
                                match fs::create_dir_all(&new_dir) {
                                    Ok(_) => {
                                        self.status = format!("Папка '{}' создана", input);
                                        self.refresh_local();
                                    }
                                    Err(e) => {
                                        self.status = format!("Ошибка создания папки: {}", e);
                                    }
                                }
                                self.mode = Mode::Normal;
                            }
                            Mode::CreatingBookmarkFolder => {
                                self.get_current_folder_mut().folders.push(BookmarkFolder {
                                    name: input, folders: Vec::new(), tracks: Vec::new(),
                                });
                                self.save_bookmarks();
                                self.status = "Папка закладок создана".into();
                                self.mode = Mode::Normal;
                            }
                            Mode::RenamingBookmarkFolder => {
                                if let Some(idx) = self.bookmark_index_to_rename.take() {
                                    self.get_current_folder_mut().folders[idx].name = input;
                                    self.save_bookmarks();
                                }
                                self.mode = Mode::Normal;
                            }
                            Mode::RenamingFile => {
                                if let Some(old_path) = self.file_to_rename.take() {
                                    let ext = old_path.extension().and_then(|e| e.to_str()).unwrap_or("");
                                    let new_name = if ext.is_empty() { input } else { format!("{}.{}", input, ext) };
                                    if let Err(e) = fs::rename(&old_path, self.current_dir.join(new_name)) {
                                        self.status = format!("Ошибка: {}", e);
                                    } else {
                                        self.status = "Переименовано".into();
                                    }
                                }
                                self.mode = Mode::Normal;
                                self.refresh_local();
                            }
                            _ => {}
                        }
                    } else { 
                        self.mode = Mode::Normal; 
                    }
                }
                KeyCode::Esc => { 
                    self.mode = Mode::Normal; 
                    if self.mode == Mode::FilteringLocal { self.local_filter.clear(); self.refresh_local(); }
                }
                KeyCode::Char(c) => {
                    if self.mode == Mode::FilteringLocal {
                        self.local_filter.push(c);
                        self.refresh_local();
                    } else {
                        self.input.push(c);
                    }
                }
                KeyCode::Backspace => {
                    if self.mode == Mode::FilteringLocal {
                        self.local_filter.pop();
                        self.refresh_local();
                    } else {
                        self.input.pop();
                    }
                }
                _ => {}
            }
            return Ok(false); // Завершаем обработку, чтобы не сработали глобальные клавиши
        }

        // =====================================================================
        // УРОВЕНЬ 4: КОНТЕКСТНЫЕ ДЕЙСТВИЯ (Зависят от фокуса: Local или Saved)
        // ВАЖНО: Эти блоки идут ДО глобальных, чтобы перехватывать 'd', 'r', 'f'
        // =====================================================================
        match code {
            // --- Действия в панели ЗАКЛАДОК ---
            KeyCode::Char('m') if self.focus == Focus::Saved => {
                self.mode = Mode::CreatingBookmarkFolder;
                self.input.clear();
            }
            KeyCode::Char('r') if self.focus == Focus::Saved => {
                if let Some(SavedItem::Folder { index, name }) = self.get_saved_items().get(self.saved_selected) {
                    self.mode = Mode::RenamingBookmarkFolder;
                    self.bookmark_index_to_rename = Some(*index);
                    self.input = name.clone();
                }
            }
            KeyCode::Char('d') if self.focus == Focus::Saved => {
                if let Some(item) = self.get_saved_items().get(self.saved_selected).cloned() {
                    match &item {
                        SavedItem::Folder { name, .. } => self.status = format!("Удалить папку '{}'? (y/n)", name),
                        SavedItem::Track { track, .. } => self.status = format!("Удалить трек '{}'? (y/n)", track.title),
                    }
                    self.confirm_delete_bookmark = Some(item);
                }
            }
            KeyCode::Backspace if self.focus == Focus::Saved => {
                if !self.saved_path.is_empty() {
                    self.saved_path.pop();
                    self.saved_selected = 0;
                }
            }

            // --- Действия в панели ЛОКАЛЬНЫХ ФАЙЛОВ ---
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
                            self.status = "Удалить папку и всё содержимое? (y/n)".into();
                        }
                    }
                }
            }
            KeyCode::Char('f') if self.focus == Focus::Local => {
                self.mode = Mode::FilteringLocal;
                self.local_filter.clear();
                self.refresh_local();
                self.status = "Фильтр файлов...".into();
            }
            KeyCode::Backspace if self.focus == Focus::Local => {
                if let Some(parent) = self.current_dir.parent() { 
                    self.current_dir = parent.to_path_buf(); 
                    self.refresh_local(); 
                }
            }

            // =================================================================
            // УРОВЕНЬ 5: ГЛОБАЛЬНЫЕ ГОРЯЧИЕ КЛАВИШИ (Работают всегда)
            // =================================================================
            KeyCode::Char('q') => {
                if self.chaos_mode { self.stop_chaos(); }
                return Ok(true);
            }
            KeyCode::Char('?') => self.show_help = true,
            KeyCode::F(5) => { self.refresh_local(); self.status = "Обновлено".into(); }
            
            // Панели (1, 2, 3)
            KeyCode::Char('1') => { self.show_local = !self.show_local; self.ensure_focus_visible(); }
            KeyCode::Char('2') => { self.show_results = !self.show_results; self.ensure_focus_visible(); }
            KeyCode::Char('3') => { self.show_saved = !self.show_saved; self.ensure_focus_visible(); }

            // Поиск и ввод
            KeyCode::Char('/') => { self.mode = Mode::Searching; self.input.clear();}
            KeyCode::Char('s') => { self.mode = Mode::Seeking; self.input.clear(); }
            KeyCode::Char('i') => { self.mode = Mode::Importing; self.input.clear(); }
            KeyCode::Char('m') => { self.mode = Mode::CreatingDir; self.input.clear(); }
            
            // Воспроизведение
            KeyCode::Tab => self.focus = self.focus.next(self.show_results, self.show_local, self.show_saved),
            KeyCode::Char('p') | KeyCode::Char(' ') => { if let Some(mpv) = &self.mpv { let _ = mpv.set_pause(!self.paused); } }
            KeyCode::Left | KeyCode::Char('h') => { if let Some(mpv) = &self.mpv { let _ = mpv.seek_rel(-5); } }
            KeyCode::Right | KeyCode::Char('l') => { if let Some(mpv) = &self.mpv { let _ = mpv.seek_rel(5); } }
            KeyCode::Char('+') | KeyCode::Char('=') => { self.volume = (self.volume + 5).min(150); if let Some(mpv) = &self.mpv { let _ = mpv.set_volume(self.volume); } }
            KeyCode::Char('-') => { self.volume = (self.volume - 5).max(0); if let Some(mpv) = &self.mpv { let _ = mpv.set_volume(self.volume); } }
            KeyCode::Char('n') => self.play_next()?,
            
            // Переключатели режимов
            KeyCode::Char('r') => { self.shuffle = !self.shuffle; self.status = format!("Shuffle: {}", self.shuffle); }
            KeyCode::Char('R') => {
                self.repeat_mode = match self.repeat_mode {
                    RepeatMode::None => RepeatMode::All,
                    RepeatMode::All => RepeatMode::One,
                    RepeatMode::One => RepeatMode::None,
                };
                self.status = format!("Повтор: {:?}", self.repeat_mode);
            }

            // Показывание текстов
            KeyCode::Char('T') => { self.show_lyrics = !self.show_lyrics; self.lyrics_scroll = None; }
            // Полный экран визуализатора
            KeyCode::Char('f') | KeyCode::Char('F') => { self.fullscreen_viz = !self.fullscreen_viz; self.status = format!("Полный экран: {}", self.fullscreen_viz); }
            // Смена визуализатора
            KeyCode::Char('V') => { self.viz_mode = (self.viz_mode + 1) % 15; self.status = format!("Визуализация: {}", self.viz_mode); }
            // Хаос мод
            KeyCode::Char('C') => { if self.chaos_mode { self.stop_chaos(); } else { self.start_chaos(); } }

            // Добавление в закладки (только если фокус на YouTube)
            KeyCode::Char('a') => {
                if self.focus == Focus::Results {
                    if let Some(t) = self.results.get(self.selected).cloned() {
                        if !self.get_current_folder().tracks.iter().any(|x| x.id == t.id) {
                            self.get_current_folder_mut().tracks.push(t);
                            self.save_bookmarks();
                            self.status = "Добавлено в текущую папку закладок".into();
                        } else { 
                            self.status = "Уже есть в этой папке".into(); 
                        }
                    }
                }
            }

            // Скачивание (работает в любой панели, где есть выделенный трек)
            KeyCode::Char('D') => {
                if self.download_rx.is_none() {
                    let to_dl = match self.focus {
                        Focus::Results => self.results.get(self.selected).cloned(),
                        Focus::Saved => match self.get_saved_items().get(self.saved_selected) {
                            Some(SavedItem::Track { track, .. }) => Some(track.clone()),
                            _ => None,
                        },
                        Focus::Local => self.current.clone(),
                    };
                    if let Some(t) = to_dl {
                        if t.local_path.is_none() {
                            let id = t.id.clone();
                            let dir = self.current_dir.clone();
                            self.status = format!("Скачивание: {}...", t.title);
                            let (tx, rx) = mpsc::channel();
                            std::thread::spawn(move || { ytdlp::download(&t, &dir, tx); });
                            self.download_rx = Some((id, rx));
                            self.download_percent = Some(0.0);
                        } else {
                            self.status = "Уже скачано".into();
                        }
                    }
                }
            }

            // Если ничего не подошло — передаем в функцию навигации по спискам
            _ => self.handle_list(code)?,
        }
        Ok(false)
    }
    /// Обработка навигации по спискам (стрелки, Enter).
    fn handle_list(&mut self, code: KeyCode) -> io::Result<()> {
        match code {
            KeyCode::Down | KeyCode::Char('j') => match self.focus {
                Focus::Results => if !self.results.is_empty() { self.selected = (self.selected + 1) % self.results.len(); },
                Focus::Local   => if !self.local_items.is_empty() { self.local_selected = (self.local_selected + 1) % self.local_items.len(); },
                Focus::Saved   => { let n = self.get_saved_items().len(); if n > 0 { self.saved_selected = (self.saved_selected + 1) % n; } },
            },
            KeyCode::Up | KeyCode::Char('k') => match self.focus {
                Focus::Results => self.selected = self.selected.saturating_sub(1),
                Focus::Local   => self.local_selected = self.local_selected.saturating_sub(1),
                Focus::Saved   => self.saved_selected = self.saved_selected.saturating_sub(1),
            },
            KeyCode::Enter => match self.focus {
                Focus::Results => {
                    if let Some(t) = self.results.get(self.selected).cloned() { self.play_track(t)?; }
                }
                Focus::Local => {
                    if let Some(item) = self.local_items.get(self.local_selected) {
                        match item {
                            LocalItem::Dir(p) => { self.current_dir = p.clone(); self.refresh_local(); }
                            LocalItem::File(t) => self.play_track(t.clone())?,
                        }
                    }
                }
                Focus::Saved => {
                    if let Some(item) = self.get_saved_items().get(self.saved_selected).cloned() {
                        match item {
                            SavedItem::Folder { index, .. } => {
                                self.saved_path.push(index);
                                self.saved_selected = 0;
                            }
                            SavedItem::Track { track, .. } => self.play_track(track)?,
                        }
                    }
                }
            },
            _ => {}
        }
        Ok(())
    }

    /// Запускает воспроизведение трека и параллельно подгружает текст песни
    fn play_track(&mut self, t: Track) -> io::Result<()> {
        if let Some(mpv) = &self.mpv {
            mpv.play(&t.play_target())?;
            Notification::new().summary("Сейчас играет").body(&t.title).show().ok();
        }
        self.current = Some(t.clone());
        
        // Сбрасываем старый текст, чтобы UI показал "Загрузка..." вместо устаревших строк.
        self.lyrics_data = None;
        self.lyrics_scroll = None; 
        
        let title = t.title.clone();
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || { 
            let _ = tx.send(api::fetch_lyrics(&title)
                .unwrap_or_else(|_| vec![(-1.0, "Текст не найден".to_string())])); 
        });
        self.lyrics_rx = Some(rx);
        
        Ok(())
    }

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
            // Начинаем поиск со следующего элемента
            let mut idx = if self.shuffle { 
                rand::random::<usize>() % self.local_items.len() 
            } else { 
                (self.local_selected + 1) % self.local_items.len() 
            };
            
            let start_idx = idx;
            loop {
                // Если нашли файл — играем и выходим из цикла
                if let LocalItem::File(t) = &self.local_items[idx] {
                    self.local_selected = idx;
                    let t_clone = t.clone();
                    self.play_track(t_clone)?;
                    break;
                }
                
                // Иначе идем дальше (или берем случайный при shuffle)
                idx = if self.shuffle {
                    rand::random::<usize>() % self.local_items.len()
                } else {
                    (idx + 1) % self.local_items.len()
                };
                
                // Защита от бесконечного цикла (если в папке одни папки)
                if idx == start_idx {
                    self.status = "Нет аудиофайлов в текущей папке".into();
                    break;
                }
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
            
            // Легкое покачивание 
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

    /// Рекурсивно собирает аудиофайлы из папки и всех вложенных
    fn collect_audio_files(dir: &std::path::Path, files: &mut Vec<PathBuf>, limit: usize) {
        if files.len() >= limit { return; }
        if let Ok(entries) = fs::read_dir(dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    Self::collect_audio_files(&path, files, limit);
                } else if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
                    let ext_lower = ext.to_lowercase();
                    if ["mp3", "flac", "wav", "ogg", "m4a", "opus", "aac"].contains(&ext_lower.as_str()) {
                        files.push(path);
                        if files.len() >= limit { return; }
                    }
                }
            }
        }
    }

    fn start_chaos(&mut self) {
        let mut files = Vec::new();
        Self::collect_audio_files(&self.current_dir, &mut files, 200);

        if files.len() < 5 {
            self.status = "Нужно минимум 5 аудиофайлов для хаоса".into();
            return;
        }

        if let Some(mpv) = &self.mpv { let _ = mpv.set_pause(true); }

        use rand::seq::SliceRandom;
        files.shuffle(&mut rand::thread_rng());
        let chosen: Vec<PathBuf> = files.into_iter().take(5).collect();

        // Генерируем параметры и заполняем UI
        let mut chaos_tracks = Vec::new();
        let mut track_params = Vec::new();

        for file in chosen {
            let pitch: f64 = 0.7 + rand::random::<f64>() * 0.8;
            let reverse = rand::random::<bool>();
            let start = rand::random::<f64>() * 30.0;
            
            chaos_tracks.push(ChaosTrack {
                title: file.file_stem().unwrap_or_default().to_string_lossy().to_string(),
                pitch,
                reversed: reverse,
                start_offset: start,
            });
            track_params.push((file, pitch, reverse, start));
        }

        self.chaos_tracks = chaos_tracks;
        self.chaos_start = Some(std::time::Instant::now());
        self.chaos_mode = true;
        self.show_chaos_info = true; // <--- Показываем окно
        self.status = "🌀 CHAOS MODE ACTIVATED".into();

        let pid = std::process::id();
        let base_volume = (self.volume as f64 * 0.8).clamp(20.0, 150.0);

        std::thread::spawn(move || {
            for (i, (file, pitch, reverse, start)) in track_params.into_iter().enumerate() {
                let temp_path = std::env::temp_dir().join(format!("fishpot_chaos_{}_{}.wav", pid, i));

                let mut filter = format!(
                    "asetrate=44100*{:.2},aresample=44100,atempo={:.4},dynaudnorm",
                    pitch, 1.0 / pitch
                );
                if reverse {
                    filter.push_str(",areverse");
                }

                let ffmpeg_ok = std::process::Command::new("ffmpeg")
                    .args(["-y", "-i", &file.to_string_lossy(), "-af", &filter, "-vn", "-ac", "2", "-ar", "44100", &temp_path.to_string_lossy()])
                    .stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null())
                    .status().map(|s| s.success()).unwrap_or(false);

                if !ffmpeg_ok { continue; }

                // Добавляем --loop=inf, чтобы треки играли бесконечно, пока не выключим хаос
                let _ = std::process::Command::new("mpv")
                    .args([
                        "--no-video",
                        &format!("--volume={:.0}", base_volume),
                        "--audio-device=auto",
                        "--loop=inf", // <--- Зацикливаем
                        &format!("--start={:.1}", start),
                        &temp_path.to_string_lossy(),
                    ])
                    .stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null())
                    .spawn();
            }
        });
    }

    fn stop_chaos(&mut self) {
        let _ = std::process::Command::new("pkill")
            .args(["-f", "fishpot_chaos_"])
            .spawn();

        if let Ok(entries) = fs::read_dir(std::env::temp_dir()) {
            for entry in entries.flatten() {
                if entry.file_name().to_string_lossy().starts_with("fishpot_chaos_") {
                    let _ = fs::remove_file(entry.path());
                }
            }
        }
        self.chaos_mode = false;
        self.show_chaos_info = false;
        self.chaos_tracks.clear();
        self.chaos_start = None;
        self.status = "Chaos mode: OFF".into();
    }


    fn ensure_focus_visible(&mut self) {
        let is_visible = match self.focus {
            Focus::Results => self.show_results,
            Focus::Local => self.show_local,
            Focus::Saved => self.show_saved,
        };
        if !is_visible {
            self.focus = Focus::Results; // Если скрыли текущую, перекидываем на YouTube
            if !self.show_results { self.focus = Focus::Local; }
            if !self.show_local && !self.show_results { self.focus = Focus::Saved; }
        }
    }
    

    pub fn get_current_folder(&self) -> &BookmarkFolder {
        let mut cur = &self.saved_tree;
        for &i in &self.saved_path { cur = &cur.folders[i]; }
        cur
    }

    pub fn get_current_folder_mut(&mut self) -> &mut BookmarkFolder {
        let mut cur = &mut self.saved_tree;
        for &i in &self.saved_path { cur = &mut cur.folders[i]; }
        cur
    }

    pub fn get_saved_items(&self) -> Vec<SavedItem> {
        let cur = self.get_current_folder();
        let mut v = Vec::new();
        for (i, f) in cur.folders.iter().enumerate() {
            v.push(SavedItem::Folder { index: i, name: f.name.clone() });
        }
        for (i, t) in cur.tracks.iter().enumerate() {
            v.push(SavedItem::Track { index: i, track: t.clone() });
        }
        v
    }

    fn save_bookmarks(&self) {
        if let Some(p) = self.saved_file_path.parent() { let _ = fs::create_dir_all(p); }
        let _ = fs::write(&self.saved_file_path, serde_json::to_string_pretty(&self.saved_tree).unwrap_or_default());
    }

}

impl Drop for App {
    fn drop(&mut self) {
        // 1. Убиваем все mpv-процессы, которые играют
        let _ = std::process::Command::new("pkill")
            .args(["-f", "fishpot_chaos_"])
            .status();

        // 2. Удаляем временные WAV-файлы из за хаоса, чтобы не засорять /tmp
        if let Ok(entries) = fs::read_dir(std::env::temp_dir()) {
            for entry in entries.flatten() {
                if entry.file_name().to_string_lossy().starts_with("fishpot_chaos_") {
                    let _ = fs::remove_file(entry.path());
                }
            }
        }
    }
}

/// Парсит время из строки
fn parse_time(input: &str) -> Option<f64> {
    let parts: Vec<&str> = input.split(':').collect();
    match parts.len() { 
        1 => parts[0].parse::<f64>().ok(), 
        2 => Some(parts[0].parse::<f64>().ok()? * 60.0 + parts[1].parse::<f64>().ok()?), 
        _ => None 
    }
}

