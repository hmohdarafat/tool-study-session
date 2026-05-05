use std::f64::consts::PI;
use std::io::{self, Stdout, Write, stdout};
use std::process::Command;
use std::time::Duration;

use chrono::{Local, Timelike};
use crossterm::cursor::{Hide, MoveTo, Show};
use crossterm::event::{
    self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, MouseButton, MouseEventKind,
};
use crossterm::style::Print;
use crossterm::terminal::{
    self, Clear, ClearType, EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode,
    enable_raw_mode,
};
use crossterm::{ExecutableCommand, QueueableCommand};

const CLOCK_HEIGHT: u16 = 37;
const CLOCK_PADDING_X: u16 = 2;
const CELL_ASPECT_RATIO: f64 = 0.5;
const HOUR_MARKER_SCALE: f64 = 0.8;
const MINUTE_LABEL_SCALE: f64 = 0.6;
const MINUTE_DOT_SCALE: f64 = 0.72;
const MINUTE_HAND_SCALE: f64 = MINUTE_DOT_SCALE;
const FONT_BUTTON_Y_PADDING: u16 = 1;
const MIN_FONT_SIZE: i32 = 6;

struct AppState {
    font: FontSetting,
}

struct FontSetting {
    schema: String,
    family: String,
    size: i32,
}

struct FontButtons {
    minus_x: u16,
    plus_x: u16,
    y: u16,
}

fn main() -> io::Result<()> {
    let mut stdout = stdout();
    let mut state = AppState {
        font: current_font_setting().unwrap_or(FontSetting {
            schema: "org.gnome.desktop.interface".to_string(),
            family: "Ubuntu Sans Mono".to_string(),
            size: 13,
        }),
    };

    enable_raw_mode()?;
    stdout.execute(EnterAlternateScreen)?;
    stdout.execute(Hide)?;
    stdout.execute(EnableMouseCapture)?;

    let result = run(&mut stdout, &mut state);

    disable_raw_mode()?;
    stdout.execute(DisableMouseCapture)?;
    stdout.execute(Show)?;
    stdout.execute(LeaveAlternateScreen)?;
    result
}

fn run(stdout: &mut Stdout, state: &mut AppState) -> io::Result<()> {
    loop {
        let buttons = render(stdout, state)?;

        if event::poll(Duration::from_millis(250))? {
            match event::read()? {
                Event::Key(key) => match key.code {
                    KeyCode::Char('q') | KeyCode::Esc => break,
                    KeyCode::Char('-') => adjust_font_size(state, -1)?,
                    KeyCode::Char('+') | KeyCode::Char('=') => adjust_font_size(state, 1)?,
                    _ => {}
                },
                Event::Mouse(mouse) => {
                    if matches!(mouse.kind, MouseEventKind::Down(MouseButton::Left)) {
                        if mouse.row == buttons.y {
                            if (buttons.minus_x..buttons.minus_x + 3).contains(&mouse.column) {
                                adjust_font_size(state, -1)?;
                            } else if (buttons.plus_x..buttons.plus_x + 3).contains(&mouse.column) {
                                adjust_font_size(state, 1)?;
                            }
                        }
                    }
                }
                _ => {}
            }
        }
    }

    Ok(())
}

fn render(stdout: &mut Stdout, state: &AppState) -> io::Result<FontButtons> {
    let (width, height) = terminal::size()?;
    let frame = build_clock(width, height);
    let frame_width = frame.first().map(|line| line.len() as u16).unwrap_or(0);
    let origin_x = width.saturating_sub(frame_width + CLOCK_PADDING_X);
    let origin_y: u16 = 0;
    let buttons = FontButtons {
        minus_x: 0,
        plus_x: 4,
        y: height.saturating_sub(FONT_BUTTON_Y_PADDING),
    };

    stdout.queue(Clear(ClearType::All))?;

    for (row, line) in frame.iter().enumerate() {
        let y = origin_y.saturating_add(row as u16);
        if y >= height {
            break;
        }

        stdout.queue(MoveTo(origin_x, y))?;
        stdout.queue(Print(line.as_str()))?;
    }

    stdout.queue(MoveTo(buttons.minus_x, buttons.y))?;
    stdout.queue(Print("[-]"))?;
    stdout.queue(MoveTo(buttons.plus_x, buttons.y))?;
    stdout.queue(Print("[+]"))?;
    stdout.queue(MoveTo(8, buttons.y))?;
    stdout.queue(Print(format!("{} {}", state.font.family, state.font.size)))?;

    stdout.flush()?;
    Ok(buttons)
}

fn build_clock(max_width: u16, max_height: u16) -> Vec<String> {
    let (width, height) = clock_dimensions(max_width, max_height);
    let mut grid = vec![vec![' '; width]; height];
    let center_x = (width as f64 - 1.0) / 2.0;
    let center_y = (height as f64 - 1.0) / 2.0;
    let radius = center_y.min(center_x * CELL_ASPECT_RATIO);
    let radius_x = radius / CELL_ASPECT_RATIO;
    let radius_y = radius;

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
        &mut grid,
        center_x,
        center_y,
        radius_x,
        radius_y,
        hour_angle,
        HOUR_MARKER_SCALE,
    );
    draw_hand(
        &mut grid,
        center_x,
        center_y,
        radius_x,
        radius_y,
        minute_angle,
        MINUTE_HAND_SCALE,
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
    let is_minute_hand = (scale - MINUTE_HAND_SCALE).abs() < f64::EPSILON;
    let tip = hand_tip_glyph(angle);
    let body = if is_minute_hand {
        '-'
    } else {
        hand_body_glyph(angle)
    };
    let body_scale = if is_minute_hand {
        (scale - 0.03).max(0.0)
    } else {
        scale
    };
    let (body_end_x, body_end_y) =
        polar_to_grid(center_x, center_y, radius_x, radius_y, body_scale, angle);

    draw_line(
        grid,
        start_x,
        start_y,
        body_end_x as isize,
        body_end_y as isize,
        body,
    );
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

fn current_font_setting() -> io::Result<FontSetting> {
    if let Some(profile_schema) = current_terminal_profile_schema()? {
        let output = Command::new("gsettings")
            .args(["get", &profile_schema, "font"])
            .output()?;
        let raw = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if let Some(mut setting) = parse_font_setting(&raw) {
            setting.schema = profile_schema;
            return Ok(setting);
        }
    }

    let output = Command::new("gsettings")
        .args(["get", "org.gnome.desktop.interface", "monospace-font-name"])
        .output()?;
    let raw = String::from_utf8_lossy(&output.stdout).trim().to_string();
    let mut setting = parse_font_setting(&raw).ok_or_else(|| {
        io::Error::new(io::ErrorKind::InvalidData, "unable to parse font setting")
    })?;
    setting.schema = "org.gnome.desktop.interface".to_string();
    Ok(setting)
}

fn parse_font_setting(raw: &str) -> Option<FontSetting> {
    let trimmed = raw.trim().trim_matches('\'');
    let (family, size) = trimmed.rsplit_once(' ')?;
    Some(FontSetting {
        schema: String::new(),
        family: family.to_string(),
        size: size.parse().ok()?,
    })
}

fn adjust_font_size(state: &mut AppState, delta: i32) -> io::Result<()> {
    let next_size = (state.font.size + delta).max(MIN_FONT_SIZE);
    if next_size == state.font.size {
        return Ok(());
    }

    let value = format!("{} {}", state.font.family, next_size);
    let status = if state.font.schema == "org.gnome.desktop.interface" {
        Command::new("gsettings")
            .args([
                "set",
                "org.gnome.desktop.interface",
                "monospace-font-name",
                &value,
            ])
            .status()?
    } else {
        Command::new("gsettings")
            .args(["set", &state.font.schema, "font", &value])
            .status()?
    };

    if status.success() {
        state.font.size = next_size;
        Ok(())
    } else {
        Err(io::Error::other("failed to update gsettings font size"))
    }
}

fn current_terminal_profile_schema() -> io::Result<Option<String>> {
    let output = Command::new("gsettings")
        .args(["get", "org.gnome.Terminal.ProfilesList", "default"])
        .output()?;
    let profile_id = String::from_utf8_lossy(&output.stdout)
        .trim()
        .trim_matches('\'')
        .to_string();

    if profile_id.is_empty() {
        return Ok(None);
    }

    Ok(Some(format!(
        "org.gnome.Terminal.Legacy.Profile:/org/gnome/terminal/legacy/profiles:/:{profile_id}/"
    )))
}
