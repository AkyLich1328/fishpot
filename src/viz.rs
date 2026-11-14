// =====================================================================
// Файл с визуализаторами и их побочными функциями
// =====================================================================

use ratatui::{
    layout::Rect,
    style::{Color, Style},
    text::{Line, Span},
    widgets::Paragraph,
    Frame,
};


//===========================

//===Поезд====
pub fn render_train(f: &mut Frame, area: Rect, t: f64, bass: f64, _mid: f64, treble: f64) {
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

//===Аквариум с шакальными рыбками====
pub fn render_aquarium(f: &mut Frame, area: Rect, t: f64, bass: f64, treble: f64) {
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

//===Варп прыжок====
pub fn draw_warp(canvas: &mut [(u8, Color)], w: usize, h: usize, t: f64, bass: f64, treble: f64) {
    let (cx, cy) = (w as f64 / 2.0, h as f64 / 2.0);
    let max_z = 150.0;
    let fov = 300.0; 
    
    let speed = 20.0 + bass * 180.0; 


    let num_stars = 400;
    for i in 0..num_stars {
        // Распределяем звезды по спирали в цилиндре
        let theta = (i as f64 * 2.39996) % (std::f64::consts::PI * 2.0);
        let r_3d = (i as f64 % 100.0) / 50.0 + 0.1; // Радиус от центра 
        
        let x_3d = r_3d * theta.cos();
        let y_3d = r_3d * theta.sin();

        let offset = (i as f64 * 13.7) % max_z;
        let z = (max_z - (t * speed + offset) % max_z).max(1.0);

        let scale = fov / z;
        let px = cx + x_3d * scale;
        let py = cy + y_3d * scale * 0.5;

        let prev_z = (z + speed * 0.05).min(max_z); 
        let prev_scale = fov / prev_z;
        let prev_px = cx + x_3d * prev_scale;
        let prev_py = cy + y_3d * prev_scale * 0.5;

        let col = if z < 15.0 {
            if treble > 0.5 && i % 3 == 0 { Color::White } else { Color::Rgb(150, 255, 255) }
        } else if z < 50.0 {
            Color::Rgb(100, 150, 255)
        } else if z < 100.0 {
            Color::Rgb(50, 50, 200)
        } else {
            Color::Rgb(20, 20, 80)
        };

        draw_line(canvas, w, h, prev_px as i32, prev_py as i32, px as i32, py as i32, col);

        // 2. ГЕНЕРАЦИЯ ИСКР
        if i % 5 == 0 {
            for j in 0..3 {
                let spark_seed = i as f64 + j as f64 * 13.0;
                
                let spark_z = (max_z - (t * speed * 1.5 + spark_seed * 17.0) % max_z).max(1.0);
                let spark_scale = fov / spark_z;
                
                let angle_offset = (spark_seed * 10.0).fract() * std::f64::consts::PI * 2.0;
                let dist_offset = (spark_seed * 3.0).fract() * 0.2; 
                
                let sx_3d = x_3d + angle_offset.cos() * dist_offset;
                let sy_3d = y_3d + angle_offset.sin() * dist_offset;

                let spark_px = cx + sx_3d * spark_scale;
                let spark_py = cy + sy_3d * spark_scale * 0.5;

                // Искры видны только вблизи
                if spark_z < 80.0 {
                    let spark_col = if spark_z < 20.0 { Color::White } else { Color::Rgb(100, 200, 255) };
                    plot(canvas, w, h, spark_px as i32, spark_py as i32, spark_col);
                    plot(canvas, w, h, spark_px as i32 + 1, spark_py as i32, spark_col);
                    plot(canvas, w, h, spark_px as i32, spark_py as i32 + 1, spark_col);
                }
            }
        }
    }

    //центр.вспышка
    let core_radius = 2.0 + bass * 8.0;
    for y in (cy - core_radius) as i32..=(cy + core_radius) as i32 {
        for x in (cx - core_radius) as i32..=(cx + core_radius) as i32 {
            let dist = ((x as f64 - cx).powi(2) + (y as f64 - cy).powi(2)).sqrt();
            if dist < core_radius {
                // Чем ближе к центру, тем ярче
                let col = if dist < core_radius * 0.5 { Color::White } else { Color::Rgb(150, 200, 255) };
                plot(canvas, w, h, x, y, col);
            }
        }
    }
}

//===Туннель====
pub fn draw_tunnel(canvas: &mut [(u8, Color)], w: usize, h: usize, t: f64, freqs: &[f64], bass: f64) {
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

//===Океан====
pub fn draw_ocean(canvas: &mut [(u8, Color)], w: usize, h: usize, t: f64, bass: f64, mid: f64) {
    let cx = w as f64 / 2.0;
    let cy = h as f64 * 0.35; // Линия горизонта чуть выше середины
    let fov = 400.0; // Угол обзора (фокусное расстояние)

    let max_z = 40.0; // Глубина океана

    let mut prev_row: Vec<Option<(i32, i32)>> = vec![None; 80];

    for z_step in (0..40).rev() {
        let z = z_step as f64 + 1.0; 
        
        let scale = fov / z;

        let mut current_row: Vec<Option<(i32, i32)>> = vec![None; 80];
        for x_step in 0..80 {
            let x = (x_step as f64 - 40.0) * 0.5; 

            // Вычисляем высоту волны (шум)
            // Басы делают волны огромными, миды — более частыми
            let wave_freq = 0.5 + mid * 0.5;
            let wave = (x * wave_freq + t * 1.5).sin() * (z * 0.2 - t * 2.0).cos();
            
            let amplitude = (10.0 + bass * 40.0) * scale * 0.2; 
            let y_offset = wave * amplitude;

            let px = cx + x * scale;
            let py = cy + (scale * 1.5) + y_offset;

            let px_i = px as i32;
            let py_i = py as i32;

            // Проверка границ
            if px_i >= 0 && px_i < w as i32 && py_i >= 0 && py_i < h as i32 {
                current_row[x_step] = Some((px_i, py_i));
                
                // Цвет: чем ближе (меньше Z), тем ярче. Градиент от синего к фиолетовому
                let depth_ratio = z / max_z;
                let col = get_gradient(1.0 - depth_ratio);

                if x_step > 0 {
                    if let Some((prev_px, prev_py)) = current_row[x_step - 1] {
                        draw_line(canvas, w, h, prev_px, prev_py, px_i, py_i, col);
                    }
                }

                if let Some((prev_px, prev_py)) = prev_row[x_step] {
                    let col_z = if bass > 0.6 { Color::Rgb(200, 255, 255) } else { col };
                    draw_line(canvas, w, h, prev_px, prev_py, px_i, py_i, col_z);
                }
            }
        }
        prev_row = current_row;
    }
}

//===Плазма====
pub fn draw_plasma(canvas: &mut [(u8, Color)], w: usize, h: usize, t: f64, bass: f64, _mid: f64, _treble: f64) {
    for y in 0..h {
        for x in 0..w {
            let xf = x as f64 * 0.02;
            let yf = y as f64 * 0.04; 

            // Формула плазмы
            let v1 = (xf * 2.0 + t).sin();
            let v2 = (yf * 2.0 - t).cos();
            let v3 = ((xf + yf) * 1.5 + t * 1.5).sin();
            let v4 = ((xf.powi(2) + yf.powi(2)).sqrt() * 3.0 - t * 2.0).cos();
            
            // Суммируем и нормализуем от -4..4 в 0..1
            let plasma = (v1 + v2 + v3 + v4 + 4.0) / 8.0;
            
            let final_v = (plasma * (1.0 + bass * 2.0)).fract().abs();
            
            let col = get_gradient(final_v);
            plot(canvas, w, h, x as i32, y as i32, col);
        }
    }
}

//===Спинограф====
pub fn draw_spirograph(canvas: &mut [(u8, Color)], w: usize, h: usize, t: f64, bass: f64, _mid: f64, treble: f64) {
    let (cx, cy) = (w as f64 / 2.0, h as f64 / 2.0);
    
    let aspect = 0.5; 
    let k = 7.0; 
    let r = 1.0;
    let big_r = r * k; 
    let d = big_r * (0.3 + treble * 0.4); 

    let max_r = (w.min(h) as f64) * 0.4 * (1.0 + bass * 0.2);
    let scale = max_r / big_r;

    let rotation = t * 0.2;
    
    let mut prev: Option<(i32, i32)> = None;
    let steps = 1000; 

    for i in 0..=steps {
        let angle = (i as f64 / steps as f64) * std::f64::consts::PI * 2.0;
        
        let x = (big_r - r) * angle.cos() + d * (((big_r - r) / r) * angle).cos();
        let y = (big_r - r) * angle.sin() - d * (((big_r - r) / r) * angle).sin();

        let rx = x * rotation.cos() - y * rotation.sin();
        let ry = x * rotation.sin() + y * rotation.cos();

        let px = cx + rx * scale;
        let py = cy + ry * scale * aspect;

        let col = get_gradient((i as f64 / steps as f64 + t * 0.1) % 1.0);

        if let Some((ppx, ppy)) = prev {
            if i % 2 == 0 {
                draw_line(canvas, w, h, ppx, ppy, px as i32, py as i32, col);
            }
        }
        prev = Some((px as i32, py as i32));

        if i % 8 == 0 {
            let seed = i as f64 * 1.618;
            
            let life = (t * 0.5 + seed).fract();
            
            let emit_angle = (seed * 10.0) % (std::f64::consts::PI * 2.0);
            
            let dist = life * 30.0; 
            
            let p_x = px + emit_angle.cos() * dist;
            let p_y = py + emit_angle.sin() * dist * aspect;
            
            let fade = 1.0 - life;
            
            if let Color::Rgb(r_col, g_col, b_col) = col {
                let p_col = Color::Rgb(
                    (r_col as f64 * fade) as u8,
                    (g_col as f64 * fade) as u8,
                    (b_col as f64 * fade) as u8,
                );
                
                plot(canvas, w, h, p_x as i32, p_y as i32, p_col);
                

                if life < 0.3 {
                    plot(canvas, w, h, p_x as i32 + 1, p_y as i32, p_col);
                    plot(canvas, w, h, p_x as i32, p_y as i32 + 1, p_col);
                }
            }
        }
    }
}

//===3Д Сфера====
pub fn draw_sphere(canvas: &mut [(u8, Color)], w: usize, h: usize, t: f64, bass: f64, mid: f64, treble: f64) {
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

//===ДНК====
pub fn draw_dna(canvas: &mut [(u8, Color)], w: usize, h: usize, t: f64, bass: f64, mid: f64, treble: f64) {
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

//===Матрица====
pub fn render_matrix_rain(f: &mut Frame, area: Rect, t: f64, bass: f64, treble: f64) {
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

//===Череп====
pub fn render_skull(f: &mut Frame, area: Rect, t: f64, bass: f64, treble: f64) {
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

//===Черная дыра====
pub fn draw_black_hole(canvas: &mut [(u8, Color)], w: usize, h: usize, t: f64, bass: f64, mid: f64, treble: f64) {
    let (cx, cy) = (w as f64 / 2.0, h as f64 / 2.0);
    let event_horizon = 5.0 + bass * 25.0; 
    
    let r_max = (w.max(h) as f64) * 0.6;
    let r_min = event_horizon;
    
    // 1. В(от)сасывание
    let num_disk = 1200;
    for i in 0..num_disk {
        let base_angle = (i as f64 * 2.399) % (std::f64::consts::PI * 2.0);
        
        let offset = (i as f64 * 0.013) % 1.0;
        let speed = 0.2 + mid * 0.5;
        let p = (t * speed + offset) % 1.0;
        
        let r = r_max - (r_max - r_min) * p;
        let swirl = (1.0 - p) * 10.0 * t;
        let angle = base_angle + swirl;

        let tilt = 0.35;
        let x = cx + angle.cos() * r;
        let y = cy + angle.sin() * r * tilt;
        

        let p_prev = (p - 0.03).max(0.0);
        let r_prev = r_max - (r_max - r_min) * p_prev;
        let swirl_prev = (1.0 - p_prev) * 10.0 * t;
        let angle_prev = base_angle + swirl_prev;
        let x_prev = cx + angle_prev.cos() * r_prev;
        let y_prev = cy + angle_prev.sin() * r_prev * tilt;

        let col = if p > 0.9 {
            Color::White
        } else if p > 0.6 {
            Color::Rgb(255, 200, 100) // Желтый
        } else if p > 0.3 {
            Color::Rgb(255, 100, 50)  // Оранжевый
        } else {
            Color::Rgb(150, 50, 150)  // Фиолетовый
        };

        draw_line(canvas, w, h, x_prev as i32, y_prev as i32, x as i32, y as i32, col);
    }

    // 2. выброс нахуй от центра
    let num_ejecta = 300;
    for i in 0..num_ejecta {
        let angle = (i as f64 * 3.14) % (std::f64::consts::PI * 2.0);

        let offset = (i as f64 * 0.02) % 1.0;
        let speed = 0.5 + treble * 1.5;
        let p = (t * speed + offset) % 1.0;
        
        let r = r_min + p * r_max * 0.5;
        
        let angle_warped = angle + p * 2.0;
        let x = cx + angle_wobble(angle_warped, p) * r;
        let y = cy + angle_wobble(angle_warped + 1.57, p) * r;

        let p_prev = (p - 0.05).max(0.0);
        let r_prev = r_min + p_prev * r_max * 0.5;
        let angle_prev = angle + p_prev * 2.0;
        let x_prev = cx + angle_wobble(angle_prev, p_prev) * r_prev;
        let y_prev = cy + angle_wobble(angle_prev + 1.57, p_prev) * r_prev;

        let col = if p < 0.2 {
            Color::Rgb(100, 255, 255) // Ярко-голубой у центра
        } else {
            Color::Rgb(50, 100, 200)  // Затухающий синий
        };

        draw_line(canvas, w, h, x_prev as i32, y_prev as i32, x as i32, y as i32, col);
    }

    // 3. горизонт дыри
    for y in (cy - event_horizon) as i32..=(cy + event_horizon) as i32 {
        for x in (cx - event_horizon) as i32..=(cx + event_horizon) as i32 {
            let dist = ((x as f64 - cx).powi(2) + (y as f64 - cy).powi(2)).sqrt();
            if dist < event_horizon {
                plot(canvas, w, h, x, y, Color::Rgb(0, 0, 0));
            }
        }
    }
}

//===Радиальные волны====
pub fn draw_radial(canvas: &mut [(u8, Color)], w: usize, h: usize, t: f64, freqs: &[f64]) {
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

//===Снежный Барс ЧАТ ГПТ РУЛИТС==== 
pub fn draw_cat(canvas: &mut [(u8, Color)], w: usize, h: usize, t: f64, bass: f64, _mid: f64, treble: f64) {
    let (cx, cy) = (w as f64 / 2.0, h as f64 - 20.0); 

    // 🎯 Коэффициент масштаба. 
    // 0.5 — очень маленький, 1.0 — средний (как сейчас), 1.5 — огромный.
    let scale = 0.8; 

    // 1. Фон: Вьюга и снег на полу
    for i in 0..300 {
        let speed = 15.0 + (i % 5) as f64 * 10.0;
        let x = (i as f64 * 37.0 - t * speed) % w as f64;
        let x = if x < 0.0 { x + w as f64 } else { x }; 
        let y = (i as f64 * 23.0 + t * 5.0) % h as f64;
        let col = if i % 3 == 0 { Color::White } else { Color::Rgb(100, 150, 200) };
        plot(canvas, w, h, x as i32, y as i32, col);
    }

    for x in 0..w as i32 {
        let wave = ((x as f64 * 0.05 + t * 1.5).sin() * 3.0).abs();
        let floor_y = (h as f64 - 4.0 - wave) as i32;
        for y in floor_y..h as i32 {
            if y >= 0 {
                let col = if y == floor_y { Color::White } else { Color::Rgb(150, 180, 220) };
                plot(canvas, w, h, x, y, col);
            }
        }
    }

    let bounce = (t * 3.0).sin() * 1.0 + bass * 3.0;
    
    // Ограничиваем циклы, чтобы не рисовать лишнего
    let cat_w = (100.0 * scale) as i32; 
    let cat_h = (120.0 * scale) as i32;

    for y in -cat_h..cat_h {
        for x in -cat_w..cat_w {
            // Применяем масштаб и коррекцию Брайля (Y сжимается в 2 раза)
            let draw_y = (cy + (y as f64 * scale) * 0.5 + bounce) as i32; 
            let draw_x = (cx + x as f64 * scale) as i32;

            // ==========================================
            // СЛОЙ 1: ТЕЛО
            // ==========================================
            let body_cy = 35.0; // Сдвинуто вниз
            let body_rx = 45.0;
            let body_ry = 40.0;
            let body_eq = (x as f64).powi(2) / (body_rx * body_rx) + ((y as f64 - body_cy).powi(2)) / (body_ry * body_ry);
            
            if body_eq <= 1.0 && y > -5 {
                let shade = if (y as f64) > body_cy { Color::Rgb(160, 160, 170) } else { Color::Rgb(220, 220, 230) };
                let col = if (x * 13 + y * 29).abs() % 47 < 3 { Color::Rgb(140, 140, 150) } else { shade };
                plot(canvas, w, h, draw_x, draw_y, col);
            }

            // ==========================================
            // СЛОЙ 2: ГОЛОВА (Круглая)
            // ==========================================
            let head_cy = -25.0; 
            let head_rx = 55.0;
            let head_ry = 45.0;
            
            let head_eq = (x as f64).powi(2) / (head_rx * head_rx) + (y as f64 - head_cy).powi(2) / (head_ry * head_ry);
            
            if head_eq <= 1.0 {
                let is_spot = (x * 19 + y * 37).abs() % 59 < 4;
                let shade = if (x as f64) > 10.0 && (y as f64) > head_cy { 
                    Color::Rgb(170, 170, 180) 
                } else if head_eq > 0.85 {
                    Color::Rgb(180, 180, 190) 
                } else {
                    Color::Rgb(250, 250, 255) 
                };
                let col = if is_spot { Color::Rgb(200, 200, 210) } else { shade };
                plot(canvas, w, h, draw_x, draw_y, col);
            }

            // ==========================================
            // СЛОЙ 3: УШИ (Правильные треугольники)
            // ==========================================
            // Уши четко над головой
            if y > -85 && y < -40 {
                let progress = (y as f64 - (-85.0)) / 45.0; 
                let ear_w = progress * 15.0; 
                
                // Левое
                let ear_cx_l = -35.0;
                if (x as f64) > ear_cx_l - ear_w && (x as f64) < ear_cx_l + ear_w {
                    let is_inner = (x as f64) > ear_cx_l - ear_w + 3.0 && (x as f64) < ear_cx_l + ear_w - 3.0 && progress > 0.2;
                    let col = if is_inner { Color::Rgb(255, 160, 190) } else { Color::Rgb(200, 200, 210) };
                    plot(canvas, w, h, draw_x, draw_y, col);
                }
                // Правое
                let ear_cx_r = 35.0;
                if (x as f64) > ear_cx_r - ear_w && (x as f64) < ear_cx_r + ear_w {
                    let is_inner = (x as f64) > ear_cx_r - ear_w + 3.0 && (x as f64) < ear_cx_r + ear_w - 3.0 && progress > 0.2;
                    let col = if is_inner { Color::Rgb(255, 160, 190) } else { Color::Rgb(200, 200, 210) };
                    plot(canvas, w, h, draw_x, draw_y, col);
                }
            }

            // ==========================================
            // СЛОЙ 4: ГЛАЗА (Большие и круглые)
            // ==========================================
            let eye_y = head_cy - 5.0;
            let eye_r = 12.0;
            
            let dist_l = ((x as f64 + 22.0).powi(2) + ((y as f64 - eye_y) * 2.0).powi(2)).sqrt();
            if dist_l <= eye_r {
                let col = if dist_l > eye_r - 2.0 { Color::Rgb(50, 50, 60) } else { Color::Rgb(10, 10, 15) };
                plot(canvas, w, h, draw_x, draw_y, col);
                // Блики
                if x > -27 && x < -17 && (y as f64) > eye_y - 5.0 && (y as f64) < eye_y + 2.0 { plot(canvas, w, h, draw_x, draw_y, Color::White); }
                if x > -15 && x < -12 && (y as f64) > eye_y + 4.0 && (y as f64) < eye_y + 7.0 { plot(canvas, w, h, draw_x, draw_y, Color::White); }
            }
            
            let dist_r = ((x as f64 - 22.0).powi(2) + ((y as f64 - eye_y) * 2.0).powi(2)).sqrt();
            if dist_r <= eye_r {
                let col = if dist_r > eye_r - 2.0 { Color::Rgb(50, 50, 60) } else { Color::Rgb(10, 10, 15) };
                plot(canvas, w, h, draw_x, draw_y, col);
                // Блики
                if x > 17 && x < 27 && (y as f64) > eye_y - 5.0 && (y as f64) < eye_y + 2.0 { plot(canvas, w, h, draw_x, draw_y, Color::White); }
                if x > 12 && x < 15 && (y as f64) > eye_y + 4.0 && (y as f64) < eye_y + 7.0 { plot(canvas, w, h, draw_x, draw_y, Color::White); }
            }

            // ==========================================
            // СЛОЙ 5: НОС
            // ==========================================
            let nose_y = (head_cy + 10.0) as i32;
            if y >= nose_y && y <= nose_y + 5 && x >= -5 && x <= 5 {
                let width = 5 - (y - nose_y);
                if (x as i32).abs() <= width {
                    let col = if y == nose_y || x == -width || x == width { Color::Rgb(200, 70, 90) } else { Color::Rgb(255, 120, 150) };
                    plot(canvas, w, h, draw_x, draw_y, col);
                }
            }

            // ==========================================
            // СЛОЙ 6: ЛАПА
            // ==========================================
            let paw_scale = 1.0 + bass * 0.8; // Плавное увеличение
            let paw_rx = 25.0 * paw_scale; 
            let paw_ry = 16.0 * paw_scale; 
            let paw_cy = 45.0 - (bass * 10.0); 
            
            let dist_paw = (x as f64).powi(2) / (paw_rx * paw_rx) + ((y as f64 - paw_cy).powi(2)) / (paw_ry * paw_ry);
            
            if dist_paw <= 1.0 {
                if dist_paw > 0.85 {
                    plot(canvas, w, h, draw_x, draw_y, Color::Rgb(70, 70, 80)); // Контур
                } else {
                    let col = if bass > 0.5 { Color::Rgb(255, 255, 255) } else { Color::Rgb(230, 230, 240) };
                    plot(canvas, w, h, draw_x, draw_y, col);
                }
            }

            let pad_color = if treble > 0.5 { Color::Rgb(255, 50, 120) } else { Color::Rgb(120, 50, 70) };
            
            // Подушечки
            if ((x as f64).powi(2) / 2.5 + (y as f64 - (paw_cy + 3.0)).powi(2)) <= 30.0 {
                plot(canvas, w, h, draw_x, draw_y, pad_color);
            }
            for i in 0..4 {
                let px = -12.0 + i as f64 * 8.0;
                let py = paw_cy - 6.0 - (i as f64 - 1.5).powi(2) * 1.0;
                if ((x as f64 - px).powi(2) + (y as f64 - py).powi(2)) <= 9.0 {
                    plot(canvas, w, h, draw_x, draw_y, pad_color);
                }
            }
        }
    }
}

///Вспомогательные функции
pub fn plot(c: &mut [(u8, Color)], w: usize, h: usize, x: i32, y: i32, col: Color) { 
    if x>=0 && y>=0 && (x as usize)<w && (y as usize)<h { 
        c[y as usize*w + x as usize] = (1, col); 
    } 
}

pub fn draw_line(canvas: &mut [(u8, Color)], w: usize, h: usize, x0: i32, y0: i32, x1: i32, y1: i32, col: Color) {
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

pub fn get_gradient(ratio: f64) -> Color {
    let r = if ratio < 0.5 { (ratio * 2.0 * 150.0) as u8 } else { (150.0 + (ratio - 0.5) * 2.0 * 105.0) as u8 };
    let g = if ratio < 0.5 { (ratio * 2.0 * 50.0) as u8 } else { (50.0 + (ratio - 0.5) * 2.0 * 205.0) as u8 };
    let b = if ratio < 0.5 { (255.0 - ratio * 2.0 * 155.0) as u8 } else { (100.0 - (ratio - 0.5) * 2.0 * 100.0) as u8 };
    Color::Rgb(r, g, b)
}

pub fn braille(dx: u8, dy: u8) -> u32 { 
    match (dx, dy) { 
        (0,0)=>0x01,(0,1)=>0x02,(0,2)=>0x04,(1,0)=>0x08,
        (1,1)=>0x10,(1,2)=>0x20,(0,3)=>0x40,(1,3)=>0x80,
        _=>0 
    } 
}

pub fn rotate_3d(x: f64, y: f64, z: f64, ax: f64, ay: f64) -> (f64, f64, f64) {
    let y1 = y * ax.cos() - z * ax.sin(); 
    let z1 = y * ax.sin() + z * ax.cos();
    (x * ay.cos() + z1 * ay.sin(), y1, -x * ay.sin() + z1 * ay.cos())
}

pub fn angle_wobble(angle: f64, p: f64) -> f64 {
    angle.cos() * (1.0 + (p * 10.0).sin() * 0.2)
}