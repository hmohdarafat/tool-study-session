use std::f64::consts::PI;
use std::io::{self, Stdout, Write, stdout};
use std::process::Command;
use std::time::Duration;

use chrono::{Datelike, Local, NaiveDate, Timelike};
use crossterm::cursor::{Hide, MoveTo, Show};
use crossterm::event::{
    self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, MouseButton, MouseEventKind,
};
use crossterm::style::{Color, Print, ResetColor, SetForegroundColor};
use crossterm::terminal::{
    self, Clear, ClearType, EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode,
    enable_raw_mode,
};
use crossterm::{ExecutableCommand, QueueableCommand};

const CLOCK_HEIGHT: u16 = 37;
const CELL_ASPECT_RATIO: f64 = 0.5;
const HOUR_MARKER_SCALE: f64 = 0.8;
const MINUTE_LABEL_SCALE: f64 = 0.6;
const MINUTE_DOT_SCALE: f64 = 0.72;
const MINUTE_HAND_SCALE: f64 = MINUTE_DOT_SCALE;
const FONT_BUTTON_Y_PADDING: u16 = 1;
const MIN_FONT_SIZE: i32 = 6;
const HOUR_LABEL_COLOR: Color = Color::Cyan;
const MINUTE_LABEL_COLOR: Color = Color::Yellow;
const MINUTE_TICK_COLOR: Color = Color::DarkYellow;
const HOUR_HAND_COLOR: Color = Color::Magenta;
const MINUTE_HAND_COLOR: Color = Color::Green;
const CENTER_COLOR: Color = Color::White;
const CONTROL_COLOR: Color = Color::Blue;
const FONT_INFO_COLOR: Color = Color::DarkCyan;
const CALENDAR_HEADER_COLOR: Color = Color::Magenta;
const CALENDAR_YEAR_COLOR: Color = Color::Cyan;
const CALENDAR_WEEKDAY_COLOR: Color = Color::Blue;
const CALENDAR_DAY_COLOR: Color = Color::Green;
const CALENDAR_WEEKEND_COLOR: Color = Color::Red;
const CALENDAR_CURRENT_DAY_COLOR: Color = Color::White;
const CALENDAR_CURRENT_DAY_BG: Color = Color::DarkBlue;
const PANEL_GAP: u16 = 4;

struct AppState {
    font: FontSetting,
    original_font: FontSetting,
    calendar_year: i32,
    calendar_month: u32,
}

#[derive(Clone)]
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

struct CalendarButtons {
    month_prev_x: u16,
    month_prev_end: u16,
    month_next_x: u16,
    month_next_end: u16,
    year_prev_x: u16,
    year_prev_end: u16,
    year_next_x: u16,
    year_next_end: u16,
    y: u16,
}

struct UiControls {
    font: FontButtons,
    calendar: CalendarButtons,
}

#[derive(Clone, Copy)]
struct Cell {
    ch: char,
    fg: Option<Color>,
    bg: Option<Color>,
}

fn main() -> io::Result<()> {
    let mut stdout = stdout();
    let initial_font = current_font_setting().unwrap_or(FontSetting {
        schema: "org.gnome.desktop.interface".to_string(),
        family: "Ubuntu Sans Mono".to_string(),
        size: 13,
    });
    let today = Local::now().date_naive();
    let mut state = AppState {
        font: initial_font.clone(),
        original_font: initial_font,
        calendar_year: today.year(),
        calendar_month: today.month(),
    };

    enable_raw_mode()?;
    stdout.execute(EnterAlternateScreen)?;
    stdout.execute(Hide)?;
    stdout.execute(EnableMouseCapture)?;

    let result = run(&mut stdout, &mut state);
    let restore_result = restore_font_setting(&state.original_font);

    disable_raw_mode()?;
    stdout.execute(DisableMouseCapture)?;
    stdout.execute(Show)?;
    stdout.execute(LeaveAlternateScreen)?;
    result?;
    restore_result
}

fn run(stdout: &mut Stdout, state: &mut AppState) -> io::Result<()> {
    loop {
        let controls = render(stdout, state)?;

        if event::poll(Duration::from_millis(250))? {
            match event::read()? {
                Event::Key(key) => match key.code {
                    KeyCode::Char('q') | KeyCode::Esc => break,
                    KeyCode::Char('-') => {
                        adjust_font_size(state, -1)?;
                        render(stdout, state)?;
                    }
                    KeyCode::Char('+') | KeyCode::Char('=') => {
                        adjust_font_size(state, 1)?;
                        render(stdout, state)?;
                    }
                    _ => {}
                },
                Event::Mouse(mouse) => {
                    if matches!(mouse.kind, MouseEventKind::Down(MouseButton::Left)) {
                        if mouse.row == controls.font.y {
                            if (controls.font.minus_x..controls.font.minus_x + 3)
                                .contains(&mouse.column)
                            {
                                adjust_font_size(state, -1)?;
                                render(stdout, state)?;
                            } else if (controls.font.plus_x..controls.font.plus_x + 3)
                                .contains(&mouse.column)
                            {
                                adjust_font_size(state, 1)?;
                                render(stdout, state)?;
                            }
                        }
                        if mouse.row == controls.calendar.y {
                            if (controls.calendar.month_prev_x..controls.calendar.month_prev_end)
                                .contains(&mouse.column)
                            {
                                shift_month(state, -1);
                            } else if (controls.calendar.month_next_x
                                ..controls.calendar.month_next_end)
                                .contains(&mouse.column)
                            {
                                shift_month(state, 1);
                            } else if (controls.calendar.year_prev_x
                                ..controls.calendar.year_prev_end)
                                .contains(&mouse.column)
                            {
                                state.calendar_year -= 1;
                            } else if (controls.calendar.year_next_x
                                ..controls.calendar.year_next_end)
                                .contains(&mouse.column)
                            {
                                state.calendar_year += 1;
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

fn render(stdout: &mut Stdout, state: &AppState) -> io::Result<UiControls> {
    let (width, height) = terminal::size()?;
    let calendar = build_calendar(state.calendar_year, state.calendar_month);
    let calendar_width = calendar.first().map(|line| line.len() as u16).unwrap_or(0);
    let available_clock_width = width.saturating_sub(calendar_width + PANEL_GAP);
    let clock = build_clock(available_clock_width, height);
    let clock_width = clock.first().map(|line| line.len() as u16).unwrap_or(0);
    let total_width = calendar_width + PANEL_GAP + clock_width;
    let calendar_x = width.saturating_sub(total_width);
    let clock_x = calendar_x + calendar_width + PANEL_GAP;
    let origin_y: u16 = 0;
    let font_buttons = FontButtons {
        minus_x: 0,
        plus_x: 4,
        y: height.saturating_sub(FONT_BUTTON_Y_PADDING),
    };
    let calendar_buttons =
        build_calendar_buttons(state.calendar_year, state.calendar_month, calendar_x);

    stdout.queue(Clear(ClearType::All))?;

    for (row, line) in calendar.iter().enumerate() {
        let y = origin_y.saturating_add(row as u16);
        if y >= height {
            break;
        }

        stdout.queue(MoveTo(calendar_x, y))?;
        write_colored_line(stdout, line)?;
    }

    for (row, line) in clock.iter().enumerate() {
        let y = origin_y.saturating_add(row as u16);
        if y >= height {
            break;
        }

        stdout.queue(MoveTo(clock_x, y))?;
        write_colored_line(stdout, line)?;
    }

    stdout.queue(MoveTo(font_buttons.minus_x, font_buttons.y))?;
    stdout.queue(SetForegroundColor(CONTROL_COLOR))?;
    stdout.queue(Print("[-]"))?;
    stdout.queue(MoveTo(font_buttons.plus_x, font_buttons.y))?;
    stdout.queue(Print("[+]"))?;
    stdout.queue(MoveTo(8, font_buttons.y))?;
    stdout.queue(SetForegroundColor(FONT_INFO_COLOR))?;
    stdout.queue(Print(format!("{} {}", state.font.family, state.font.size)))?;
    stdout.queue(ResetColor)?;

    stdout.flush()?;
    Ok(UiControls {
        font: font_buttons,
        calendar: calendar_buttons,
    })
}

fn write_colored_line(stdout: &mut Stdout, line: &[Cell]) -> io::Result<()> {
    let mut active_fg = None;
    let mut active_bg = None;
    for cell in line {
        if cell.fg != active_fg {
            match cell.fg {
                Some(color) => stdout.queue(SetForegroundColor(color))?,
                None => stdout.queue(ResetColor)?,
            };
            active_fg = cell.fg;
            active_bg = None;
        }
        if cell.bg != active_bg {
            if let Some(color) = cell.bg {
                stdout.queue(crossterm::style::SetBackgroundColor(color))?;
            } else if active_fg.is_some() {
                stdout.queue(crossterm::style::SetBackgroundColor(Color::Reset))?;
            }
            active_bg = cell.bg;
        }
        stdout.queue(Print(cell.ch))?;
    }
    stdout.queue(ResetColor)?;
    stdout.queue(crossterm::style::SetBackgroundColor(Color::Reset))?;
    Ok(())
}

fn build_calendar(year: i32, month: u32) -> Vec<Vec<Cell>> {
    let width = 28usize;
    let height = 10usize;
    let mut grid = vec![vec![blank_cell(); width]; height];
    let buttons = build_calendar_buttons(year, month, 0);
    let month_name = month_name(month);
    let title = format!("< {} >", month_name);
    let year_text = format!("< {} >", year);

    write_text(&mut grid, 0, 0, &title, CALENDAR_HEADER_COLOR, None);
    write_text(
        &mut grid,
        buttons.year_prev_x as usize,
        0,
        &year_text,
        CALENDAR_YEAR_COLOR,
        None,
    );

    let weekdays = ["Su", "Mo", "Tu", "We", "Th", "Fr", "Sa"];
    for (idx, day) in weekdays.iter().enumerate() {
        let color = if is_weekend_column(idx) {
            CALENDAR_WEEKEND_COLOR
        } else {
            CALENDAR_WEEKDAY_COLOR
        };
        write_text(&mut grid, idx * 4, 2, day, color, None);
    }

    let first_day = NaiveDate::from_ymd_opt(year, month, 1).unwrap();
    let start_col = first_day.weekday().num_days_from_sunday() as usize;
    let days_in_month = days_in_month(year, month);
    let today = Local::now().date_naive();

    for day in 1..=days_in_month {
        let index = start_col + (day - 1) as usize;
        let row = 4 + index / 7;
        let col = (index % 7) * 4;
        let color = if is_weekend_column(index % 7) {
            CALENDAR_WEEKEND_COLOR
        } else {
            CALENDAR_DAY_COLOR
        };
        let bg = if today.year() == year && today.month() == month && today.day() == day {
            Some(CALENDAR_CURRENT_DAY_BG)
        } else {
            None
        };
        let fg = if bg.is_some() {
            CALENDAR_CURRENT_DAY_COLOR
        } else {
            color
        };
        write_text(&mut grid, col, row, &format!("{day:>2}"), fg, bg);
    }

    grid
}

fn build_clock(max_width: u16, max_height: u16) -> Vec<Vec<Cell>> {
    let (width, height) = clock_dimensions(max_width, max_height);
    let mut grid = vec![vec![blank_cell(); width]; height];
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

    grid
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
    grid: &mut [Vec<Cell>],
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
            grid,
            center_x,
            center_y,
            radius_x,
            radius_y,
            scale,
            angle,
            &label,
            HOUR_LABEL_COLOR,
        );
    }
}

fn place_minute_labels(
    grid: &mut [Vec<Cell>],
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
            grid,
            center_x,
            center_y,
            radius_x,
            radius_y,
            scale,
            angle,
            &label,
            MINUTE_LABEL_COLOR,
        );
    }
}

fn place_minute_ticks(
    grid: &mut [Vec<Cell>],
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
        plot_hand_point(grid, x, y, minute_tick_glyph(minute), MINUTE_TICK_COLOR);
    }
}

fn minute_tick_glyph(_minute: u32) -> char {
    '.'
}

fn place_text(
    grid: &mut [Vec<Cell>],
    center_x: f64,
    center_y: f64,
    radius_x: f64,
    radius_y: f64,
    scale: f64,
    angle: f64,
    text: &str,
    color: Color,
) {
    let (x, y) = polar_to_grid(center_x, center_y, radius_x, radius_y, scale, angle);
    let text_len = text.chars().count() as isize;
    let start_x = x as isize - (text_len.saturating_sub(1) / 2);
    let y = y as isize;

    for (idx, ch) in text.chars().enumerate() {
        let px = start_x + idx as isize;
        if y >= 0 && (y as usize) < grid.len() && px >= 0 && (px as usize) < grid[y as usize].len()
        {
            grid[y as usize][px as usize] = Cell {
                ch,
                fg: Some(color),
                bg: None,
            };
        }
    }
}

fn draw_hand(
    grid: &mut [Vec<Cell>],
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
    let color = if is_minute_hand {
        MINUTE_HAND_COLOR
    } else {
        HOUR_HAND_COLOR
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
        color,
    );
    overwrite_hand_point(grid, tip_x, tip_y, tip, color);
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

fn plot_hand_point(grid: &mut [Vec<Cell>], x: usize, y: usize, glyph: char, color: Color) {
    if y < grid.len() && x < grid[y].len() && grid[y][x].ch == ' ' {
        grid[y][x] = Cell {
            ch: glyph,
            fg: Some(color),
            bg: None,
        };
    }
}

fn draw_line(
    grid: &mut [Vec<Cell>],
    mut x0: isize,
    mut y0: isize,
    x1: isize,
    y1: isize,
    glyph: char,
    color: Color,
) {
    let dx = (x1 - x0).abs();
    let sx = if x0 < x1 { 1 } else { -1 };
    let dy = -(y1 - y0).abs();
    let sy = if y0 < y1 { 1 } else { -1 };
    let mut err = dx + dy;

    loop {
        if x0 >= 0 && y0 >= 0 {
            plot_hand_point(grid, x0 as usize, y0 as usize, glyph, color);
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

fn overwrite_hand_point(grid: &mut [Vec<Cell>], x: usize, y: usize, glyph: char, color: Color) {
    if y < grid.len() && x < grid[y].len() {
        grid[y][x] = Cell {
            ch: glyph,
            fg: Some(color),
            bg: None,
        };
    }
}

fn draw_center(grid: &mut [Vec<Cell>], center_x: f64, center_y: f64) {
    let x = center_x.round() as usize;
    let y = center_y.round() as usize;
    if y < grid.len() && x < grid[y].len() {
        grid[y][x] = Cell {
            ch: '+',
            fg: Some(CENTER_COLOR),
            bg: None,
        };
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

fn blank_cell() -> Cell {
    Cell {
        ch: ' ',
        fg: None,
        bg: None,
    }
}

fn write_text(
    grid: &mut [Vec<Cell>],
    x: usize,
    y: usize,
    text: &str,
    fg: Color,
    bg: Option<Color>,
) {
    if y >= grid.len() {
        return;
    }
    for (idx, ch) in text.chars().enumerate() {
        let px = x + idx;
        if px < grid[y].len() {
            grid[y][px] = Cell {
                ch,
                fg: Some(fg),
                bg,
            };
        }
    }
}

fn build_calendar_buttons(year: i32, month: u32, calendar_x: u16) -> CalendarButtons {
    let month_title = format!("< {} >", month_name(month));
    let year_title = format!("< {} >", year);
    let year_x = 14u16;
    CalendarButtons {
        month_prev_x: calendar_x,
        month_prev_end: calendar_x + 1,
        month_next_x: calendar_x + month_title.len() as u16 - 1,
        month_next_end: calendar_x + month_title.len() as u16,
        year_prev_x: calendar_x + year_x,
        year_prev_end: calendar_x + year_x + 1,
        year_next_x: calendar_x + year_x + year_title.len() as u16 - 1,
        year_next_end: calendar_x + year_x + year_title.len() as u16,
        y: 0,
    }
}

fn month_name(month: u32) -> &'static str {
    match month {
        1 => "January",
        2 => "February",
        3 => "March",
        4 => "April",
        5 => "May",
        6 => "June",
        7 => "July",
        8 => "August",
        9 => "September",
        10 => "October",
        11 => "November",
        _ => "December",
    }
}

fn days_in_month(year: i32, month: u32) -> u32 {
    let next = if month == 12 {
        NaiveDate::from_ymd_opt(year + 1, 1, 1).unwrap()
    } else {
        NaiveDate::from_ymd_opt(year, month + 1, 1).unwrap()
    };
    (next - chrono::Days::new(1)).day()
}

fn shift_month(state: &mut AppState, delta: i32) {
    let mut month = state.calendar_month as i32 + delta;
    let mut year = state.calendar_year;
    if month < 1 {
        month = 12;
        year -= 1;
    } else if month > 12 {
        month = 1;
        year += 1;
    }
    state.calendar_month = month as u32;
    state.calendar_year = year;
}

fn is_weekend_column(column: usize) -> bool {
    matches!(column, 5 | 6)
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

    let mut next_font = state.font.clone();
    next_font.size = next_size;

    if apply_font_setting(&next_font)?.success() {
        state.font.size = next_size;
        Ok(())
    } else {
        Err(io::Error::other("failed to update gsettings font size"))
    }
}

fn restore_font_setting(font: &FontSetting) -> io::Result<()> {
    if apply_font_setting(font)?.success() {
        Ok(())
    } else {
        Err(io::Error::other("failed to restore gsettings font size"))
    }
}

fn apply_font_setting(font: &FontSetting) -> io::Result<std::process::ExitStatus> {
    let value = format!("{} {}", font.family, font.size);
    if font.schema == "org.gnome.desktop.interface" {
        Command::new("gsettings")
            .args([
                "set",
                "org.gnome.desktop.interface",
                "monospace-font-name",
                &value,
            ])
            .status()
    } else {
        Command::new("gsettings")
            .args(["set", &font.schema, "font", &value])
            .status()
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
