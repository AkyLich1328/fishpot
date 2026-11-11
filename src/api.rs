/// Загружает текст песни с lrclib.net.
/// Сначала пытаемся получить синхронизированный (караоке), если нет — обычный текст.
pub fn fetch_lyrics(title: &str) -> Result<Vec<(f64, String)>, String> {
    let query = urlencoding::encode(title);
    let url = format!("https://lrclib.net/api/search?q={}", query);

    // Устанавливаем таймаут 10 секунд, чтобы не зависнуть навсегда при сбое сервера.
    // ureq позволяет задать таймаут только через Agent.
    let agent: ureq::Agent = ureq::AgentBuilder::new()
        .timeout(std::time::Duration::from_secs(10))
        .build();

    let response = agent.get(&url).call().map_err(|e| e.to_string())?;
    let json: serde_json::Value = response.into_json().map_err(|e| e.to_string())?;

    if let Some(arr) = json.as_array() {
        for item in arr {
            if let Some(synced) = item["syncedLyrics"].as_str() {
                if !synced.is_empty() {
                    let parsed = parse_lrc(synced);
                    if !parsed.is_empty() { return Ok(parsed); }
                }
            }
            if let Some(plain) = item["plainLyrics"].as_str() {
                if !plain.is_empty() {
                    // Возвращаем с -1.0 в качестве времени — UI понимает, что это не караоке.
                    return Ok(plain.lines().map(|l| (-1.0, l.to_string())).collect());
                }
            }
        }
    }
    Err("Текст не найден".into())
}

/// Парсит LRC-формат: `[00:12.34] Текст строки`.
/// Возвращает отсортированный по времени вектор (время_в_секундах, текст).
fn parse_lrc(lrc: &str) -> Vec<(f64, String)> {
    // Примерное количество строк для уменьшения реаллокаций.
    let mut result = Vec::with_capacity(lrc.lines().count());
    
    for line in lrc.lines() {
        let line = line.trim();
        if line.is_empty() { continue; }
        
        let mut rest = line;
        let mut times = Vec::new();
        
        // В одной строке может быть несколько таймкодов: [00:12.34][00:15.00] Текст
        while rest.starts_with('[') {
            if let Some(end) = rest.find(']') {
                let time_str = &rest[1..end];
                let parts: Vec<&str> = time_str.split(':').collect();
                if parts.len() == 2 {
                    let min = parts[0].parse::<f64>().unwrap_or(0.0);
                    let sec = parts[1].parse::<f64>().unwrap_or(0.0);
                    times.push(min * 60.0 + sec);
                }
                rest = &rest[end + 1..];
            } else { break; }
        }
        
        if !times.is_empty() {
            let text = rest.trim().to_string();
            for t in times { result.push((t, text.clone())); }
        }
    }
    
    // LRC может содержать строки в произвольном порядке — сортируем.
    result.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
    result
}