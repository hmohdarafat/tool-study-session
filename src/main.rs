use std::f64::consts::PI;
use std::io::{self, Stdout, Write, stdout};
use std::time::Duration;

use chrono::{Local, Timelike};
use crossterm::cursor::{Hide, MoveTo, Show};
use crossterm::event::{self, Event, KeyCode};
use crossterm::style::Print;
use crossterm::terminal::{
    self, Clear, ClearType, EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode,
    enable_raw_mode,
};
use crossterm::{ExecutableCommand, QueueableCommand};

const CLOCK_HEIGHT: u16 = 41;
const CLOCK_PADDING_X: u16 = 2;
const CLOCK_PADDING_Y: u16 = 1;
const CELL_ASPECT_RATIO: f64 = 0.5;
const HOUR_MARKER_SCALE: f64 = 0.78;
const MINUTE_LABEL_SCALE: f64 = 0.54;
const MINUTE_DOT_SCALE: f64 = 0.64;

fn main() -> io::Result<()> {
    let mut stdout = stdout();
    enable_raw_mode()?;
    stdout.execute(EnterAlternateScreen)?;
    stdout.execute(Hide)?;

    let result = run(&mut stdout);

    disable_raw_mode()?;
    stdout.execute(Show)?;
    stdout.execute(LeaveAlternateScreen)?;
    result
}

fn run(stdout: &mut Stdout) -> io::Result<()> {
    loop {
        render(stdout)?;

        if event::poll(Duration::from_millis(250))? {
            if let Event::Key(key) = event::read()? {
                match key.code {
                    KeyCode::Char('q') | KeyCode::Esc => break,
                    _ => {}
                }
            }
        }
    }

    Ok(())
}

fn render(stdout: &mut Stdout) -> io::Result<()> {
    let (width, height) = terminal::size()?;
    let frame = build_clock(width, height.saturating_sub(CLOCK_PADDING_Y));
    let frame_width = frame.first().map(|line| line.len() as u16).unwrap_or(0);
    let origin_x = width.saturating_sub(frame_width + CLOCK_PADDING_X);
    let origin_y = CLOCK_PADDING_Y.min(height.saturating_sub(1));

    stdout.queue(Clear(ClearType::All))?;
    stdout.queue(MoveTo(0, 0))?;
    stdout.queue(Print("Analog clock demo"))?;
    stdout.queue(MoveTo(0, 1))?;
    stdout.queue(Print("Press q or Esc to quit"))?;

    for (row, line) in frame.iter().enumerate() {
        let y = origin_y.saturating_add(row as u16);
        if y >= height {
            break;
        }

        stdout.queue(MoveTo(origin_x, y))?;
        stdout.queue(Print(line.as_str()))?;
    }

    stdout.flush()
}

fn build_clock(max_width: u16, max_height: u16) -> Vec<String> {
    let (width, height) = clock_dimensions(max_width, max_height);
    let mut grid = vec![vec![' '; width]; height];
    let center_x = (width as f64 - 1.0) / 2.0;
    let center_y = (height as f64 - 1.0) / 2.0;
    let radius = center_y.min(center_x * CELL_ASPECT_RATIO);
    let radius_x = radius / CELL_ASPECT_RATIO;
    let radius_y = radius;

    for y in 0..height {
        for x in 0..width {
            let dx = ((x as f64 - center_x) * CELL_ASPECT_RATIO) / radius;
            let dy = (y as f64 - center_y) / radius;
            let distance = (dx * dx + dy * dy).sqrt();

            if (distance - 1.0).abs() <= 0.06 {
                grid[y][x] = '.';
            }
        }
    }

    place_minute_ticks(
        &mut grid,
        center_x,
        center_y,
        radius_x,
        radius_y,
        MINUTE_DOT_SCALE,
    );
    place_hour_labels(
        &mut grid,
        center_x,
        center_y,
        radius_x,
        radius_y,
        HOUR_MARKER_SCALE,
    );
    place_minute_labels(
        &mut grid,
        center_x,
        center_y,
        radius_x,
        radius_y,
        MINUTE_LABEL_SCALE,
    );

    let now = Local::now();
    let minute_value = now.minute();
    let minute = minute_value as f64;
    let hour = (now.hour() % 12) as f64 + minute / 60.0;

    let hour_angle = hour / 12.0 * 2.0 * PI;
    let minute_angle = minute / 60.0 * 2.0 * PI;

    draw_hand(
        &mut grid, center_x, center_y, radius_x, radius_y, hour_angle, 0.7,
    );
    draw_hand(
        &mut grid,
        center_x,
        center_y,
        radius_x,
        radius_y,
        minute_angle,
        minute_hand_scale(),
    );
    draw_center(&mut grid, center_x, center_y);

    grid.into_iter()
        .map(|row| row.into_iter().collect::<String>())
        .collect()
}

fn clock_dimensions(max_width: u16, max_height: u16) -> (usize, usize) {
    let mut height = make_odd(CLOCK_HEIGHT.min(max_height.max(11)));
    let preferred_width = preferred_width(height);
    let width = make_odd(preferred_width.min(max_width.max(15)));

    if width < preferred_width {
        height = make_odd(preferred_height(width).min(height));
    }

    (width as usize, height as usize)
}

fn preferred_width(height: u16) -> u16 {
    (((height.saturating_sub(1)) as f64 / CELL_ASPECT_RATIO).round() as u16).saturating_add(1)
}

fn preferred_height(width: u16) -> u16 {
    (((width.saturating_sub(1)) as f64 * CELL_ASPECT_RATIO).round() as u16).saturating_add(1)
}

fn make_odd(value: u16) -> u16 {
    if value % 2 == 0 {
        value.saturating_sub(1)
    } else {
        value
    }
}

fn place_hour_labels(
    grid: &mut [Vec<char>],
    center_x: f64,
    center_y: f64,
    radius_x: f64,
    radius_y: f64,
    scale: f64,
) {
    for hour in 1..=12 {
        let angle = hour as f64 / 12.0 * 2.0 * PI;
        let label = hour.to_string();
        place_text(
            grid, center_x, center_y, radius_x, radius_y, scale, angle, &label,
        );
    }
}

fn place_minute_labels(
    grid: &mut [Vec<char>],
    center_x: f64,
    center_y: f64,
    radius_x: f64,
    radius_y: f64,
    scale: f64,
) {
    for minute in (0..60).step_by(5) {
        let angle = minute as f64 / 60.0 * 2.0 * PI;
        let label = minute.to_string();
        place_text(
            grid, center_x, center_y, radius_x, radius_y, scale, angle, &label,
        );
    }
}

fn place_minute_ticks(
    grid: &mut [Vec<char>],
    center_x: f64,
    center_y: f64,
    radius_x: f64,
    radius_y: f64,
    scale: f64,
) {
    for minute in 0..60 {
        if minute % 5 == 0 {
            continue;
        }

        let angle = minute as f64 / 60.0 * 2.0 * PI;
        let (x, y) = polar_to_grid(center_x, center_y, radius_x, radius_y, scale, angle);
        plot_hand_point(grid, x, y, minute_tick_glyph(minute));
    }
}

fn minute_hand_scale() -> f64 {
    MINUTE_DOT_SCALE
}

fn minute_tick_glyph(_minute: u32) -> char {
    '.'
}

fn place_text(
    grid: &mut [Vec<char>],
    center_x: f64,
    center_y: f64,
    radius_x: f64,
    radius_y: f64,
    scale: f64,
    angle: f64,
    text: &str,
) {
    let (x, y) = polar_to_grid(center_x, center_y, radius_x, radius_y, scale, angle);
    let text_len = text.chars().count() as isize;
    let start_x = x as isize - (text_len.saturating_sub(1) / 2);
    let y = y as isize;

    for (idx, ch) in text.chars().enumerate() {
        let px = start_x + idx as isize;
        if y >= 0 && (y as usize) < grid.len() && px >= 0 && (px as usize) < grid[y as usize].len()
        {
            grid[y as usize][px as usize] = ch;
        }
    }
}

fn draw_hand(
    grid: &mut [Vec<char>],
    center_x: f64,
    center_y: f64,
    radius_x: f64,
    radius_y: f64,
    angle: f64,
    scale: f64,
) {
    let start_x = center_x.round() as isize;
    let start_y = center_y.round() as isize;
    let (tip_x, tip_y) = polar_to_grid(center_x, center_y, radius_x, radius_y, scale, angle);
    let tip = hand_tip_glyph(angle);
    let body = hand_body_glyph(angle);

    draw_line(grid, start_x, start_y, tip_x as isize, tip_y as isize, body);
    overwrite_hand_point(grid, tip_x, tip_y, tip);
}

fn hand_tip_glyph(angle: f64) -> char {
    let octant = ((angle / (PI / 4.0)).round() as i32).rem_euclid(8);
    match octant {
        0 => '^',
        1 => '/',
        2 => '>',
        3 => '\\',
        4 => 'v',
        5 => '/',
        6 => '<',
        _ => '\\',
    }
}

fn hand_body_glyph(angle: f64) -> char {
    let octant = ((angle / (PI / 4.0)).round() as i32).rem_euclid(8);
    match octant {
        0 | 4 => '|',
        2 | 6 => '-',
        1 | 5 => '/',
        _ => '\\',
    }
}

fn plot_hand_point(grid: &mut [Vec<char>], x: usize, y: usize, glyph: char) {
    if y < grid.len() && x < grid[y].len() && grid[y][x] == ' ' {
        grid[y][x] = glyph;
    }
}

fn draw_line(
    grid: &mut [Vec<char>],
    mut x0: isize,
    mut y0: isize,
    x1: isize,
    y1: isize,
    glyph: char,
) {
    let dx = (x1 - x0).abs();
    let sx = if x0 < x1 { 1 } else { -1 };
    let dy = -(y1 - y0).abs();
    let sy = if y0 < y1 { 1 } else { -1 };
    let mut err = dx + dy;

    loop {
        if x0 >= 0 && y0 >= 0 {
            plot_hand_point(grid, x0 as usize, y0 as usize, glyph);
        }

        if x0 == x1 && y0 == y1 {
            break;
        }

        let err2 = err * 2;
        if err2 >= dy {
            err += dy;
            x0 += sx;
        }
        if err2 <= dx {
            err += dx;
            y0 += sy;
        }
    }
}

fn overwrite_hand_point(grid: &mut [Vec<char>], x: usize, y: usize, glyph: char) {
    if y < grid.len() && x < grid[y].len() {
        grid[y][x] = glyph;
    }
}

fn draw_center(grid: &mut [Vec<char>], center_x: f64, center_y: f64) {
    let x = center_x.round() as usize;
    let y = center_y.round() as usize;
    if y < grid.len() && x < grid[y].len() {
        grid[y][x] = '+';
    }
}

fn polar_to_grid(
    center_x: f64,
    center_y: f64,
    radius_x: f64,
    radius_y: f64,
    scale: f64,
    angle: f64,
) -> (usize, usize) {
    let adjusted = angle - PI / 2.0;
    let x = center_x + radius_x * scale * adjusted.cos();
    let y = center_y + radius_y * scale * adjusted.sin();
    (x.round().max(0.0) as usize, y.round().max(0.0) as usize)
}
