use std::io::{BufRead, BufReader};
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::mpsc;

use serde::{Deserialize, Serialize};

#[derive(Debug)]
pub enum DownloadEvent {
    Progress(f64),
    Finished(String),
    Error(String),
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Track {
    pub id: String,
    pub title: String,
    #[serde(default)]
    pub duration: Option<f64>,
    #[serde(default)]
    pub local_path: Option<String>,
}

impl Track {
    pub fn url(&self) -> String { format!("https://www.youtube.com/watch?v={}", self.id) }

    pub fn play_target(&self) -> String {
        if let Some(path) = &self.local_path {
            if Path::new(path).exists() { return path.clone(); }
        }
        self.url()
    }

}

pub fn search(query: &str, n: usize) -> Result<Vec<Track>, String> {
    let output = Command::new("yt-dlp")
        .args([&format!("ytsearch{n}:{query}"), "--dump-json", "--flat-playlist", "--no-warnings"])
        .output()
        .map_err(|e| e.to_string())?;

    let mut tracks = Vec::new();
    for line in String::from_utf8_lossy(&output.stdout).lines() {
        if let Ok(t) = serde_json::from_str::<Track>(line) {
            tracks.push(t);
        }
    }
    Ok(tracks)
}

fn parse_progress(line: &str, tx: &mpsc::Sender<DownloadEvent>) -> bool {
    for part in line.split_whitespace() {
        if part.ends_with('%') {
            let clean = part.trim_end_matches('%').trim_start_matches(|c: char| !c.is_ascii_digit() && c != '.');
            if let Ok(pct) = clean.parse::<f64>() {
                let _ = tx.send(DownloadEvent::Progress(pct));
                return true;
            }
        }
    }
    false
}

fn run_ytdlp(args: &[&str], tx: &mpsc::Sender<DownloadEvent>) -> Result<String, String> {
    let mut child = Command::new("yt-dlp")
        .args(args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| e.to_string())?;

    let stderr = child.stderr.take().ok_or("Нет stderr")?;
    let stdout = child.stdout.take().ok_or("Нет stdout")?;

    // Читаем stderr (там прогресс и ошибки) в отдельном потоке.
    let tx_stderr = tx.clone();
    let stderr_thread = std::thread::spawn(move || {
        let reader = BufReader::new(stderr);
        for line in reader.lines().flatten() {
            parse_progress(&line, &tx_stderr);
        }
    });

    // Читаем stdout (там финальный путь к файлу) в отдельном потоке.
    let tx_stdout = tx.clone();
    let stdout_thread = std::thread::spawn(move || {
        let reader = BufReader::new(stdout);
        let mut last_path = String::new();
        for line in reader.lines().flatten() {
            if parse_progress(&line, &tx_stdout) {
                continue;
            }
            let trimmed = line.trim();
            if !trimmed.is_empty() && !trimmed.starts_with('[') {
                last_path = trimmed.to_string();
            }
        }
        last_path
    });

    let status = child.wait().map_err(|e| e.to_string())?;
    let _ = stderr_thread.join();
    let final_path = stdout_thread.join().unwrap_or_default();

    if status.success() {
        Ok(final_path)
    } else {
        Err("yt-dlp завершился с ошибкой".into())
    }
}

pub fn download(track: &Track, dir: &Path, tx: mpsc::Sender<DownloadEvent>) {
    if let Err(e) = std::fs::create_dir_all(dir) {
        let _ = tx.send(DownloadEvent::Error(format!("Не удалось создать папку: {e}")));
        return;
    }
    let template = format!("{}/%(title)s.%(ext)s", dir.display());

    let args = [
        "--newline", "-f", "ba/b", "-x", "--audio-format", "mp3", "--audio-quality", "0",
        "-o", &template, "--print", "after_move:filepath", &track.url()
    ];

    match run_ytdlp(&args, &tx) {
        Ok(path) => { let _ = tx.send(DownloadEvent::Finished(path)); }
        Err(e) => { let _ = tx.send(DownloadEvent::Error(e)); }
    }
}

pub fn download_playlist(url: &str, dir: &Path, tx: mpsc::Sender<DownloadEvent>) {
    if let Err(e) = std::fs::create_dir_all(dir) {
        let _ = tx.send(DownloadEvent::Error(format!("Не удалось создать папку: {e}")));
        return;
    }
    let template = format!("{}/%(playlist_title)s/%(playlist_index)02d - %(title)s.%(ext)s", dir.display());

    let args = [
        "--newline", "-f", "ba/b", "-x", "--audio-format", "mp3", "--audio-quality", "0",
        "-o", &template, "--yes-playlist", url
    ];

    match run_ytdlp(&args, &tx) {
        Ok(_) => { let _ = tx.send(DownloadEvent::Finished("Плейлист успешно скачан".into())); }
        Err(e) => { let _ = tx.send(DownloadEvent::Error(e)); }
    }
}