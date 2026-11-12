use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use ratatui::style::Color;

/// Цветовая схема интерфейса. Все цвета хранятся строками в HEX-формате,
/// чтобы пользователь мог легко менять их в config.toml без пересборки.
#[derive(Serialize, Deserialize, Clone)]
pub struct Theme {
    pub bg_color: String,
    pub active_border: String,
    pub inactive_border: String,
    pub text_color: String,
    pub accent_color: String,
    pub selected_bg: String,
    // NEW — прозрачность и ambient
    #[serde(default)]
    pub transparent_panels: bool,      // Local/Results без фона — просвечивает терминал
    #[serde(default)]
    pub transparent_visualizer: bool,  // визуализатор без фона
    #[serde(default = "default_ambient_count")]
    pub ambient_count: usize,          // сколько частиц ambient-эффекта
}

fn default_ambient_count() -> usize { 200 }

#[derive(Serialize, Deserialize, Clone)]
pub struct AppConfig {
    pub download_dir: String,
    pub viz_mode: usize,
    pub theme: Theme,
    pub ambient_effect: bool,
}

impl Default for AppConfig {
    /// Дефолтные настройки при первом запуске. Используется тема Tokyo Night.
    fn default() -> Self {
        let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
        let download_dir = home.join("Music").join("fishpot").to_string_lossy().to_string();
        
        Self {
            download_dir,
            viz_mode: 0,
            ambient_effect: false,
            theme: Theme {
                bg_color: "#1a1b26".to_string(),
                active_border: "#7aa2f7".to_string(),
                inactive_border: "#3b4252".to_string(),
                text_color: "#c0caf5".to_string(),
                accent_color: "#bb9af7".to_string(),
                selected_bg: "#283250".to_string(),
                transparent_panels: false,
                transparent_visualizer: false,
                ambient_count: 200,
            }
        }
    }
}

impl AppConfig {
    /// Загружает конфиг из `~/.config/yt-music-tui/config.toml`.
    /// Если файла нет — создаёт дефолтный и сразу сохраняет его на диск,
    /// чтобы пользователь мог его редактировать.
    pub fn load() -> Self {
        let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
        let config_path = home.join(".config").join("fishpot").join("config.toml");

        // Если файл существует и парсится — используем его.
        if let Ok(content) = std::fs::read_to_string(&config_path) {
            if let Ok(config) = toml::from_str(&content) {
                return config;
            }
        }

        // Иначе — создаём дефолтный.
        let default_config = Self::default();
        if let Some(parent) = config_path.parent() {
            if let Err(e) = std::fs::create_dir_all(parent) {
                eprintln!("Не удалось создать папку конфига: {e}");
            }
        }
        match toml::to_string_pretty(&default_config) {
            Ok(toml_str) => {
                if let Err(e) = std::fs::write(&config_path, toml_str) {
                    eprintln!("Не удалось сохранить конфиг: {e}");
                }
            }
            Err(e) => eprintln!("Ошибка сериализации конфига: {e}"),
        }

        default_config
    }
}

/// Преобразует HEX-строку (`#rrggbb`) в `Color::Rgb` для ratatui.
/// Если строка некорректна — возвращает `Reset` (терминальный цвет по умолчанию).
pub fn parse_hex(hex: &str) -> Color {
    let hex = hex.trim_start_matches('#');
    if hex.len() == 6 {
        if let (Ok(r), Ok(g), Ok(b)) = (
            u8::from_str_radix(&hex[0..2], 16),
            u8::from_str_radix(&hex[2..4], 16),
            u8::from_str_radix(&hex[4..6], 16),
        ) {
            return Color::Rgb(r, g, b);
        }
    }
    Color::Reset
}