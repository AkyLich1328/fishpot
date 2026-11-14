use std::{
    io::{self, BufRead, BufReader, Write},
    os::unix::net::UnixStream,
    path::PathBuf,
    process::{Child, Command, Stdio},
    sync::{Arc, Mutex},
    thread,
    time::Duration,
};
use serde_json::json;

/// Состояние плеера. Читается из UI, пишется фоновым потоком из mpv.
pub struct PlayerStatus {
    pub time_pos: f64,
    pub duration: f64,
    pub paused: bool,
    pub eof_reached: bool,
}

pub struct Mpv {
    child: Child,
    stream: Arc<Mutex<UnixStream>>,
    socket_path: PathBuf,
    pub status: Arc<Mutex<PlayerStatus>>,
}

impl Mpv {
    pub fn new() -> io::Result<Self> {
        // Уникальный путь сокета для каждого процесса — чтобы несколько экземпляров не конфликтовали.
        let socket_path = std::env::temp_dir().join(format!("fishpot-{}.sock", std::process::id()));
        let _ = std::fs::remove_file(&socket_path);

        let log_file = std::fs::File::create("/tmp/fishpot.log")?;
        let log_err = log_file.try_clone()?;

        // ВАЖНО: делаем child мутабельным, чтобы при ошибке мы могли его убить.
        let mut child = Command::new("mpv")
            .args([
                "--no-video", "--force-window=no", "--idle=yes",
                "--audio-device=auto",
                "--ytdl-format=bestaudio/best",
                // android-клиент реже всего отдаёт 403 Forbidden для YouTube.
                "--ytdl-raw-options=extractor-args=youtube:player_client=android",
                &format!("--input-ipc-server={}", socket_path.display()),
            ])
            .stdin(Stdio::null())
            .stdout(Stdio::from(log_file))
            .stderr(Stdio::from(log_err))
            .spawn()?;

        // Хелпер: убивает mpv и чистит сокет, возвращая исходную ошибку.
        // Определяем внутри new(), чтобы он не захватывал окружение.
        fn cleanup(child: &mut Child, socket_path: &PathBuf, e: io::Error) -> io::Error {
            let _ = child.kill();
            let _ = child.wait(); // обязательно wait, чтобы не остался зомби
            let _ = std::fs::remove_file(socket_path);
            e
        }

        // Ждём, пока mpv создаст сокет, но не бесконечно.
        let mut attempts = 0;
        let stream = loop {
            if let Ok(s) = UnixStream::connect(&socket_path) { break s; }
            attempts += 1;
            if attempts > 100 {
                let e = io::Error::new(
                    io::ErrorKind::TimedOut,
                    "Не удалось подключиться к mpv IPC-сокету",
                );
                return Err(cleanup(&mut child, &socket_path, e));
            }
            thread::sleep(Duration::from_millis(40));
        };

        let stream = Arc::new(Mutex::new(stream));
        let status = Arc::new(Mutex::new(PlayerStatus {
            time_pos: 0.0, duration: 0.0, paused: false, eof_reached: false,
        }));

        // Клонируем сокет для фонового потока. При ошибке — убиваем mpv.
        let stream_clone = match stream.lock().unwrap().try_clone() {
            Ok(s) => s,
            Err(e) => return Err(cleanup(&mut child, &socket_path, e)),
        };

        let status_clone = Arc::clone(&status);
        thread::spawn(move || {
            let reader = BufReader::new(stream_clone);
            for line in reader.lines().flatten() {
                if let Ok(v) = serde_json::from_str::<serde_json::Value>(&line) {
                    let mut s = status_clone.lock().unwrap();
                    match v["event"].as_str() {
                        Some("property-change") => {
                            match v["name"].as_str().unwrap_or("") {
                                "time-pos" => s.time_pos = v["data"].as_f64().unwrap_or(0.0),
                                "duration" => s.duration = v["data"].as_f64().unwrap_or(0.0),
                                "pause" => s.paused = v["data"].as_bool().unwrap_or(false),
                                "eof-reached" => s.eof_reached = v["data"].as_bool().unwrap_or(false),
                                _ => {}
                            }
                        }
                        Some("end-file") => s.eof_reached = true,
                        Some("start-file") => s.eof_reached = false,
                        _ => {}
                    }
                }
            }
        });

        let mpv = Self { child, stream, socket_path, status };

        // Подписываемся на изменения свойств — mpv будет присылать события при их изменении.
        for (id, prop) in &[(1, "time-pos"), (2, "duration"), (3, "pause"), (4, "eof-reached")] {
            let _ = mpv.send(json!({"command": ["observe_property", id, prop]}));
        }

        Ok(mpv)
    }

    pub fn send(&self, cmd: serde_json::Value) -> io::Result<()> {
        let mut s = self.stream.lock().unwrap();
        s.write_all((serde_json::to_string(&cmd)? + "\n").as_bytes())
    }

    pub fn play(&self, target: &str) -> io::Result<()> {
        self.send(json!({"command": ["loadfile", target, "replace"]}))
    }
    pub fn set_pause(&self, paused: bool) -> io::Result<()> {
        self.send(json!({"command": ["set_property", "pause", paused]}))
    }
    pub fn set_volume(&self, vol: i64) -> io::Result<()> {
        self.send(json!({"command": ["set_property", "volume", vol]}))
    }
    pub fn seek(&self, seconds: f64) -> io::Result<()> {
        self.send(json!({"command": ["seek", seconds, "absolute"]}))
    }
    pub fn seek_rel(&self, seconds: i64) -> io::Result<()> {
        self.send(json!({"command": ["seek", seconds, "relative"]}))
    }
}

impl Drop for Mpv {
    fn drop(&mut self) {
        // Отправляем quit, ждём завершения, удаляем сокет — чтобы не оставлять мусор.
        let _ = self.send(json!({"command": ["quit"]}));
        let _ = self.child.wait();
        let _ = std::fs::remove_file(&self.socket_path);
    }
}