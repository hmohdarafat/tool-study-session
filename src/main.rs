use std::collections::HashMap;
use std::f64::consts::PI;
use std::fs;
use std::io::{self, Stdout, Write, stdout};
use std::path::Path;
use std::process::Command;
use std::thread;
use std::time::Duration;

use chrono::{DateTime, Datelike, Local, NaiveDate, Timelike};
use crossterm::cursor::{Hide, MoveTo, Show};
use crossterm::event::{
    self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, MouseButton, MouseEventKind,
};
use crossterm::style::{Attribute, Color, Print, ResetColor, SetAttribute, SetForegroundColor};
use crossterm::terminal::{
    self, Clear, ClearType, EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode,
    enable_raw_mode,
};
use crossterm::{ExecutableCommand, QueueableCommand};
use serde::{Deserialize, Serialize};

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
const CALENDAR_HEADER_COLOR: Color = Color::Magenta;
const CALENDAR_YEAR_COLOR: Color = Color::Cyan;
const CALENDAR_WEEKDAY_COLOR: Color = Color::Blue;
const CALENDAR_DAY_COLOR: Color = Color::Green;
const CALENDAR_WEEKEND_COLOR: Color = Color::Red;
const CALENDAR_CURRENT_DAY_COLOR: Color = Color::White;
const CALENDAR_CURRENT_DAY_BG: Color = Color::DarkBlue;
const CALENDAR_SELECTED_DAY_COLOR: Color = Color::Black;
const CALENDAR_SELECTED_DAY_BG: Color = Color::Cyan;
const CALENDAR_TODO_DAY_COLOR: Color = Color::Magenta;
const TODO_HEADER_COLOR: Color = Color::Yellow;
const TODO_TEXT_COLOR: Color = Color::White;
const TODO_DONE_COLOR: Color = Color::DarkGrey;
const TODO_BOX_COLOR: Color = Color::Green;
const TODO_ADD_BUTTON_COLOR: Color = Color::Blue;
const TODO_DELETE_COLOR: Color = Color::Red;
const TODO_EDIT_BG: Color = Color::DarkGrey;
const PANEL_GAP: u16 = 4;
const POMODORO_WORK_COLOR: Color = Color::DarkGreen;
const POMODORO_BREAK_COLOR: Color = Color::DarkRed;
const START_BUTTON_COLOR: Color = Color::Magenta;
const QUIT_PROMPT_COLOR: Color = Color::Yellow;
const TODO_SAVE_PATH: &str = "todos.json";
const GRADIENT_SPEED: f64 = 45.0;
const GRADIENT_X_SCALE: f64 = 0.18;
const GRADIENT_Y_SCALE: f64 = 0.32;

struct AppState {
    font: FontSetting,
    original_font: FontSetting,
    calendar_year: i32,
    calendar_month: u32,
    selected_date: NaiveDate,
    pomodoro_start: Option<DateTime<Local>>,
    todos: HashMap<NaiveDate, Vec<TodoItem>>,
    editing_todo: Option<EditingTodo>,
    window_fitted: bool,
    quit_prompt: bool,
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
    calendar: CalendarUiControls,
    start: ActionButton,
}

struct ActionButton {
    x: u16,
    end_x: u16,
    y: u16,
}

enum PomodoroMarkerStyle {
    Hidden,
    Normal,
    Work,
    Break,
}

#[derive(Clone, Copy)]
struct Cell {
    ch: char,
    fg: Option<Color>,
    bg: Option<Color>,
    crossed: bool,
}

#[derive(Serialize, Deserialize)]
struct StoredTodoItem {
    text: String,
    done: bool,
    is_placeholder: bool,
}

#[derive(Serialize, Deserialize)]
struct StoredTodoDay {
    date: String,
    items: Vec<StoredTodoItem>,
}

#[derive(Serialize, Deserialize)]
struct StoredTodos {
    days: Vec<StoredTodoDay>,
}

struct TodoItem {
    text: String,
    done: bool,
    is_placeholder: bool,
}

struct EditingTodo {
    date: NaiveDate,
    index: usize,
    buffer: String,
    original_text: String,
    was_placeholder: bool,
}

struct CalendarUiControls {
    buttons: CalendarButtons,
    dates: Vec<DateHitbox>,
    add_button: ActionButton,
    todo_checks: Vec<TodoHitbox>,
    todo_deletes: Vec<TodoHitbox>,
    todo_texts: Vec<TodoTextHitbox>,
}

struct DateHitbox {
    date: NaiveDate,
    x: u16,
    end_x: u16,
    y: u16,
}

struct TodoHitbox {
    index: usize,
    x: u16,
    end_x: u16,
    y: u16,
}

struct TodoTextHitbox {
    index: usize,
    x: u16,
    end_x: u16,
    y: u16,
    end_y: u16,
}

struct CalendarPanel {
    grid: Vec<Vec<Cell>>,
    controls: CalendarUiControls,
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
        selected_date: today,
        pomodoro_start: None,
        todos: load_todos().unwrap_or_default(),
        editing_todo: None,
        window_fitted: false,
        quit_prompt: false,
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
    let mut controls = render(stdout, state)?;
    let mut last_rendered_minute = Local::now().minute();

    loop {
        if event::poll(duration_until_next_minute())? {
            match event::read()? {
                Event::Key(key) => {
                    if state.quit_prompt {
                        match key.code {
                            KeyCode::Char('y') | KeyCode::Char('Y') => {
                                save_editing_todo(state);
                                save_todos(&state.todos)?;
                                break;
                            }
                            KeyCode::Char('n') | KeyCode::Char('N') => break,
                            _ => continue,
                        }
                    }
                    if handle_editing_key(state, &key.code) {
                        controls = render(stdout, state)?;
                        continue;
                    }
                    match key.code {
                        KeyCode::Char('q') | KeyCode::Esc => {
                            save_editing_todo(state);
                            state.quit_prompt = true;
                            controls = render(stdout, state)?;
                        }
                        KeyCode::Char('-') => {
                            save_editing_todo(state);
                            adjust_font_size(state, -1)?;
                            controls = render(stdout, state)?;
                        }
                        KeyCode::Char('+') | KeyCode::Char('=') => {
                            save_editing_todo(state);
                            adjust_font_size(state, 1)?;
                            controls = render(stdout, state)?;
                        }
                        _ => {}
                    }
                }
                Event::Mouse(mouse) => {
                    if state.quit_prompt {
                        continue;
                    }
                    if matches!(mouse.kind, MouseEventKind::Down(MouseButton::Left)) {
                        let mut keep_editing = false;
                        if mouse.row == controls.font.y {
                            save_editing_todo(state);
                            if (controls.font.minus_x..controls.font.minus_x + 3)
                                .contains(&mouse.column)
                            {
                                adjust_font_size(state, -1)?;
                            } else if (controls.font.plus_x..controls.font.plus_x + 3)
                                .contains(&mouse.column)
                            {
                                adjust_font_size(state, 1)?;
                            }
                        }
                        if mouse.row == controls.calendar.buttons.y {
                            save_editing_todo(state);
                            if (controls.calendar.buttons.month_prev_x
                                ..controls.calendar.buttons.month_prev_end)
                                .contains(&mouse.column)
                            {
                                shift_month(state, -1);
                            } else if (controls.calendar.buttons.month_next_x
                                ..controls.calendar.buttons.month_next_end)
                                .contains(&mouse.column)
                            {
                                shift_month(state, 1);
                            } else if (controls.calendar.buttons.year_prev_x
                                ..controls.calendar.buttons.year_prev_end)
                                .contains(&mouse.column)
                            {
                                state.calendar_year -= 1;
                            } else if (controls.calendar.buttons.year_next_x
                                ..controls.calendar.buttons.year_next_end)
                                .contains(&mouse.column)
                            {
                                state.calendar_year += 1;
                            }
                        }
                        if let Some(hit) = controls.calendar.dates.iter().find(|hit| {
                            mouse.row == hit.y && (hit.x..hit.end_x).contains(&mouse.column)
                        }) {
                            save_editing_todo(state);
                            state.selected_date = hit.date;
                        }
                        if mouse.row == controls.calendar.add_button.y
                            && (controls.calendar.add_button.x..controls.calendar.add_button.end_x)
                                .contains(&mouse.column)
                        {
                            save_editing_todo(state);
                            let next_number = state
                                .todos
                                .get(&state.selected_date)
                                .map(|items| items.len() + 1)
                                .unwrap_or(1);
                            state
                                .todos
                                .entry(state.selected_date)
                                .or_default()
                                .push(TodoItem {
                                    text: format!("Todo {next_number}"),
                                    done: false,
                                    is_placeholder: true,
                                });
                        }
                        if let Some(hit) = controls.calendar.todo_deletes.iter().find(|hit| {
                            mouse.row == hit.y && (hit.x..hit.end_x).contains(&mouse.column)
                        }) {
                            save_editing_todo(state);
                            if let Some(items) = state.todos.get_mut(&state.selected_date) {
                                if hit.index < items.len() {
                                    items.remove(hit.index);
                                }
                            }
                        }
                        if let Some(hit) = controls.calendar.todo_checks.iter().find(|hit| {
                            mouse.row == hit.y && (hit.x..hit.end_x).contains(&mouse.column)
                        }) {
                            save_editing_todo(state);
                            if let Some(items) = state.todos.get_mut(&state.selected_date) {
                                if let Some(item) = items.get_mut(hit.index) {
                                    item.done = !item.done;
                                }
                            }
                        }
                        if let Some(hit) = controls.calendar.todo_texts.iter().find(|hit| {
                            (hit.y..hit.end_y).contains(&mouse.row)
                                && (hit.x..hit.end_x).contains(&mouse.column)
                        }) {
                            save_editing_todo(state);
                            if let Some(items) = state.todos.get(&state.selected_date) {
                                if let Some(item) = items.get(hit.index) {
                                    state.editing_todo = Some(EditingTodo {
                                        date: state.selected_date,
                                        index: hit.index,
                                        buffer: if item.is_placeholder {
                                            String::new()
                                        } else {
                                            item.text.clone()
                                        },
                                        original_text: item.text.clone(),
                                        was_placeholder: item.is_placeholder,
                                    });
                                    keep_editing = true;
                                }
                            }
                        }
                        if mouse.row == controls.start.y
                            && (controls.start.x..controls.start.end_x).contains(&mouse.column)
                        {
                            save_editing_todo(state);
                            state.pomodoro_start = if state.pomodoro_start.is_some() {
                                None
                            } else {
                                Some(Local::now())
                            };
                        }
                        if !keep_editing {
                            save_editing_todo(state);
                        }
                        controls = render(stdout, state)?;
                    }
                }
                Event::Resize(_, _) => {
                    state.window_fitted = true;
                    controls = render(stdout, state)?;
                }
                _ => {}
            }
        } else {
            let minute = Local::now().minute();
            if minute != last_rendered_minute {
                last_rendered_minute = minute;
                controls = render(stdout, state)?;
            }
        }
    }

    Ok(())
}

fn duration_until_next_minute() -> Duration {
    let now = Local::now();
    let millis_until_next_second = 1_000u32.saturating_sub(now.nanosecond() / 1_000_000);
    let seconds_remaining = 59u64.saturating_sub(now.second() as u64);
    let total_millis = seconds_remaining * 1_000 + millis_until_next_second as u64;
    Duration::from_millis(total_millis.max(1))
}

fn render(stdout: &mut Stdout, state: &mut AppState) -> io::Result<UiControls> {
    let (mut width, mut height) = terminal::size()?;
    let preview_calendar = build_calendar_panel(
        state.calendar_year,
        state.calendar_month,
        state.selected_date,
        &state.todos,
        state.todos.get(&state.selected_date),
        state.editing_todo.as_ref(),
        0,
    );
    let required_height = (preview_calendar.grid.len() as u16).saturating_add(2);
    if height < required_height {
        request_terminal_size(stdout, width, required_height)?;
        let (new_width, new_height) = terminal::size()?;
        width = new_width;
        height = new_height;
    }

    let content_height = height.saturating_sub(2);
    let calendar_width = preview_calendar
        .grid
        .first()
        .map(|line| line.len() as u16)
        .unwrap_or(0);
    let available_clock_width = width.saturating_sub(calendar_width + PANEL_GAP);
    let clock = build_clock(available_clock_width, content_height, state.pomodoro_start);
    let clock_width = clock.first().map(|line| line.len() as u16).unwrap_or(0);
    let clock_height = clock.len() as u16;
    let total_width = calendar_width + PANEL_GAP + clock_width;
    if !state.window_fitted {
        request_terminal_size(stdout, total_width, height)?;
        let (new_width, new_height) = terminal::size()?;
        width = new_width;
        height = new_height;
        state.window_fitted = true;
    }
    let calendar_x = 0;
    let clock_x = calendar_x + calendar_width + PANEL_GAP;
    let origin_y: u16 = 0;
    let start_label = if state.pomodoro_start.is_some() {
        "[Stop]"
    } else {
        "[Start]"
    };
    let start_button = ActionButton {
        x: clock_x + clock_width.saturating_sub(start_label.len() as u16) / 2,
        end_x: clock_x
            + clock_width.saturating_sub(start_label.len() as u16) / 2
            + start_label.len() as u16,
        y: clock_height.min(content_height),
    };
    let font_buttons = FontButtons {
        minus_x: 0,
        plus_x: 4,
        y: height.saturating_sub(FONT_BUTTON_Y_PADDING),
    };
    let quit_hint = "q = quit";
    let quit_hint_x = total_width
        .saturating_sub(quit_hint.len() as u16)
        .min(width.saturating_sub(quit_hint.len() as u16));
    let quit_prompt = "Save todos before quitting? (y/n)";
    let calendar = build_calendar_panel(
        state.calendar_year,
        state.calendar_month,
        state.selected_date,
        &state.todos,
        state.todos.get(&state.selected_date),
        state.editing_todo.as_ref(),
        calendar_x,
    );

    let mut frame = build_gradient_frame(width as usize, height as usize);
    overlay_grid(&mut frame, &calendar.grid, calendar_x as usize, origin_y as usize);
    overlay_grid(&mut frame, &clock, clock_x as usize, origin_y as usize);
    write_text(
        &mut frame,
        font_buttons.minus_x as usize,
        font_buttons.y as usize,
        "[-]",
        CONTROL_COLOR,
        None,
        false,
    );
    write_text(
        &mut frame,
        font_buttons.plus_x as usize,
        font_buttons.y as usize,
        "[+]",
        CONTROL_COLOR,
        None,
        false,
    );
    write_text(
        &mut frame,
        quit_hint_x as usize,
        font_buttons.y as usize,
        quit_hint,
        CONTROL_COLOR,
        None,
        false,
    );
    if state.quit_prompt {
        write_text(
            &mut frame,
            8,
            font_buttons.y as usize,
            quit_prompt,
            QUIT_PROMPT_COLOR,
            None,
            false,
        );
    }
    write_text(
        &mut frame,
        start_button.x as usize,
        start_button.y as usize,
        start_label,
        START_BUTTON_COLOR,
        None,
        false,
    );

    stdout.queue(Clear(ClearType::All))?;
    for (row, line) in frame.iter().enumerate() {
        stdout.queue(MoveTo(0, row as u16))?;
        write_colored_line(stdout, line)?;
    }

    stdout.flush()?;
    Ok(UiControls {
        font: font_buttons,
        calendar: calendar.controls,
        start: start_button,
    })
}

fn request_terminal_size(stdout: &mut Stdout, width: u16, height: u16) -> io::Result<()> {
    write!(stdout, "\x1b[8;{};{}t", height.max(1), width.max(1))?;
    stdout.flush()?;
    thread::sleep(Duration::from_millis(120));
    Ok(())
}

fn build_gradient_frame(width: usize, height: usize) -> Vec<Vec<Cell>> {
    (0..height)
        .map(|y| {
            (0..width)
                .map(|x| Cell {
                    ch: ' ',
                    fg: None,
                    bg: Some(gradient_color(x, y, width, height)),
                    crossed: false,
                })
                .collect()
        })
        .collect()
}

fn overlay_grid(frame: &mut [Vec<Cell>], overlay: &[Vec<Cell>], offset_x: usize, offset_y: usize) {
    for (row_idx, row) in overlay.iter().enumerate() {
        let target_y = offset_y + row_idx;
        if target_y >= frame.len() {
            break;
        }

        for (col_idx, cell) in row.iter().enumerate() {
            let target_x = offset_x + col_idx;
            if target_x >= frame[target_y].len() {
                break;
            }

            if cell.ch != ' ' || cell.fg.is_some() || cell.bg.is_some() || cell.crossed {
                let target = &mut frame[target_y][target_x];
                target.ch = cell.ch;
                target.fg = cell.fg;
                target.crossed = cell.crossed;
                if cell.bg.is_some() {
                    target.bg = cell.bg;
                }
            }
        }
    }
}

fn gradient_color(x: usize, y: usize, width: usize, height: usize) -> Color {
    let now = Local::now();
    let phase = now.timestamp_millis() as f64 / 1_000.0 / GRADIENT_SPEED;
    let x_ratio = if width > 1 {
        x as f64 / (width - 1) as f64
    } else {
        0.0
    };
    let y_ratio = if height > 1 {
        y as f64 / (height - 1) as f64
    } else {
        0.0
    };

    let red = wave(phase + x_ratio * GRADIENT_X_SCALE + y_ratio * 0.08, 10.0, 42.0);
    let green = wave(phase + y_ratio * GRADIENT_Y_SCALE + 0.33, 12.0, 36.0);
    let blue = wave(phase + (x_ratio + y_ratio) * 0.22 + 0.66, 22.0, 64.0);

    Color::Rgb { r: red, g: green, b: blue }
}

fn wave(position: f64, min: f64, amplitude: f64) -> u8 {
    (min + amplitude * (0.5 + 0.5 * (position * 2.0 * PI).sin())).round() as u8
}

fn write_colored_line(stdout: &mut Stdout, line: &[Cell]) -> io::Result<()> {
    let mut active_fg = None;
    let mut active_bg = None;
    let mut active_crossed = false;
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
        if cell.crossed != active_crossed {
            stdout.queue(SetAttribute(if cell.crossed {
                Attribute::CrossedOut
            } else {
                Attribute::NotCrossedOut
            }))?;
            active_crossed = cell.crossed;
        }
        stdout.queue(Print(cell.ch))?;
    }
    stdout.queue(ResetColor)?;
    stdout.queue(crossterm::style::SetBackgroundColor(Color::Reset))?;
    stdout.queue(SetAttribute(Attribute::NotCrossedOut))?;
    Ok(())
}

fn build_calendar_panel(
    year: i32,
    month: u32,
    selected_date: NaiveDate,
    all_todos: &HashMap<NaiveDate, Vec<TodoItem>>,
    todos: Option<&Vec<TodoItem>>,
    editing_todo: Option<&EditingTodo>,
    calendar_x: u16,
) -> CalendarPanel {
    let width = 28usize;
    let text_x = 6usize;
    let text_width = width.saturating_sub(text_x + 1);
    let year_x = 14usize;
    let todo_rows = todos
        .map(|items| {
            items
                .iter()
                .take(6)
                .enumerate()
                .map(|(index, item)| {
                    let display_text = if let Some(editing) = editing_todo {
                        if editing.date == selected_date && editing.index == index {
                            editing.buffer.as_str()
                        } else {
                            item.text.as_str()
                        }
                    } else {
                        item.text.as_str()
                    };
                    wrap_text(display_text, text_width).len().max(1)
                })
                .sum::<usize>()
        })
        .unwrap_or(0);
    let height = 13 + todo_rows;
    let mut grid = vec![vec![blank_cell(); width]; height];
    let buttons = build_calendar_buttons(year, month, calendar_x);
    let mut date_hits = Vec::new();
    let mut todo_check_hits = Vec::new();
    let mut todo_delete_hits = Vec::new();
    let mut todo_text_hits = Vec::new();
    let month_name = month_name(month);
    let title = format!("< {} >", month_name);
    let year_text = format!("< {} >", year);

    write_text(&mut grid, 0, 0, &title, CALENDAR_HEADER_COLOR, None, false);
    write_text(
        &mut grid,
        year_x,
        0,
        &year_text,
        CALENDAR_YEAR_COLOR,
        None,
        false,
    );

    let weekdays = ["Su", "Mo", "Tu", "We", "Th", "Fr", "Sa"];
    for (idx, day) in weekdays.iter().enumerate() {
        let color = if is_weekend_column(idx) {
            CALENDAR_WEEKEND_COLOR
        } else {
            CALENDAR_WEEKDAY_COLOR
        };
        write_text(&mut grid, idx * 4, 2, day, color, None, false);
    }

    let first_day = NaiveDate::from_ymd_opt(year, month, 1).unwrap();
    let start_col = first_day.weekday().num_days_from_sunday() as usize;
    let days_in_month = days_in_month(year, month);
    let today = Local::now().date_naive();

    for day in 1..=days_in_month {
        let index = start_col + (day - 1) as usize;
        let row = 4 + index / 7;
        let col = (index % 7) * 4;
        let date = NaiveDate::from_ymd_opt(year, month, day).unwrap();
        let color = if is_weekend_column(index % 7) {
            CALENDAR_WEEKEND_COLOR
        } else {
            CALENDAR_DAY_COLOR
        };
        let has_todo = all_todos
            .get(&date)
            .map(|items| !items.is_empty())
            .unwrap_or(false);
        let bg = if today.year() == year && today.month() == month && today.day() == day {
            Some(CALENDAR_CURRENT_DAY_BG)
        } else if selected_date.year() == year
            && selected_date.month() == month
            && selected_date.day() == day
        {
            Some(CALENDAR_SELECTED_DAY_BG)
        } else {
            None
        };
        let fg = if bg.is_some() {
            if selected_date.year() == year
                && selected_date.month() == month
                && selected_date.day() == day
                && !(today.year() == year && today.month() == month && today.day() == day)
            {
                CALENDAR_SELECTED_DAY_COLOR
            } else {
                CALENDAR_CURRENT_DAY_COLOR
            }
        } else {
            if has_todo {
                CALENDAR_TODO_DAY_COLOR
            } else {
                color
            }
        };
        write_text(&mut grid, col, row, &format!("{day:>2}"), fg, bg, false);
        date_hits.push(DateHitbox {
            date,
            x: calendar_x + col as u16,
            end_x: calendar_x + col as u16 + 2,
            y: row as u16,
        });
    }

    let todo_header_y = 10usize;
    let add_label = "[+]";
    write_text(
        &mut grid,
        0,
        todo_header_y,
        add_label,
        TODO_ADD_BUTTON_COLOR,
        None,
        false,
    );
    write_text(
        &mut grid,
        4,
        todo_header_y,
        &format!("Todos {}", selected_date.format("%Y-%m-%d")),
        TODO_HEADER_COLOR,
        None,
        false,
    );
    let add_button = ActionButton {
        x: calendar_x,
        end_x: calendar_x + add_label.len() as u16,
        y: todo_header_y as u16,
    };

    let mut current_row = todo_header_y + 2;
    if let Some(items) = todos {
        for (index, item) in items.iter().take(6).enumerate() {
            let delete_x = 0;
            let check_x = 2;
            let display_text = if let Some(editing) = editing_todo {
                if editing.date == selected_date && editing.index == index {
                    editing.buffer.as_str()
                } else {
                    item.text.as_str()
                }
            } else {
                item.text.as_str()
            };
            let is_editing = editing_todo
                .map(|editing| editing.date == selected_date && editing.index == index)
                .unwrap_or(false);
            let wrapped = wrap_text(display_text, text_width);
            write_text(
                &mut grid,
                delete_x,
                current_row,
                "x",
                TODO_DELETE_COLOR,
                None,
                false,
            );
            let checkbox = if item.done { "[x]" } else { "[ ]" };
            write_text(
                &mut grid,
                check_x,
                current_row,
                checkbox,
                TODO_BOX_COLOR,
                None,
                false,
            );
            for (line_offset, line) in wrapped.iter().enumerate() {
                write_text(
                    &mut grid,
                    text_x,
                    current_row + line_offset,
                    line,
                    if item.done {
                        TODO_DONE_COLOR
                    } else {
                        TODO_TEXT_COLOR
                    },
                    if is_editing { Some(TODO_EDIT_BG) } else { None },
                    item.done,
                );
            }
            todo_delete_hits.push(TodoHitbox {
                index,
                x: calendar_x,
                end_x: calendar_x + 1,
                y: current_row as u16,
            });
            todo_check_hits.push(TodoHitbox {
                index,
                x: calendar_x + check_x as u16,
                end_x: calendar_x + check_x as u16 + checkbox.len() as u16,
                y: current_row as u16,
            });
            todo_text_hits.push(TodoTextHitbox {
                index,
                x: calendar_x + text_x as u16,
                end_x: calendar_x + width as u16,
                y: current_row as u16,
                end_y: (current_row + wrapped.len()) as u16,
            });
            current_row += wrapped.len();
        }
    }

    CalendarPanel {
        grid,
        controls: CalendarUiControls {
            buttons,
            dates: date_hits,
            add_button,
            todo_checks: todo_check_hits,
            todo_deletes: todo_delete_hits,
            todo_texts: todo_text_hits,
        },
    }
}

fn build_clock(
    max_width: u16,
    max_height: u16,
    pomodoro_start: Option<DateTime<Local>>,
) -> Vec<Vec<Cell>> {
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
        pomodoro_start,
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
        pomodoro_start,
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
    pomodoro_start: Option<DateTime<Local>>,
) {
    for minute in (0..60).step_by(5) {
        let angle = minute as f64 / 60.0 * 2.0 * PI;
        let label = minute.to_string();
        let style = pomodoro_marker_style(pomodoro_start, minute);
        if matches!(style, PomodoroMarkerStyle::Hidden) {
            continue;
        }
        let color = marker_style_color(style, MINUTE_LABEL_COLOR);
        place_text(
            grid, center_x, center_y, radius_x, radius_y, scale, angle, &label, color,
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
    pomodoro_start: Option<DateTime<Local>>,
) {
    for minute in 0..60 {
        if minute % 5 == 0 {
            continue;
        }

        let style = pomodoro_marker_style(pomodoro_start, minute);
        if matches!(style, PomodoroMarkerStyle::Hidden) {
            continue;
        }
        let angle = minute as f64 / 60.0 * 2.0 * PI;
        let color = marker_style_color(style, MINUTE_TICK_COLOR);
        let (x, y) = polar_to_grid(center_x, center_y, radius_x, radius_y, scale, angle);
        plot_hand_point(grid, x, y, minute_tick_glyph(minute), color);
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
                crossed: false,
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
            crossed: false,
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
            crossed: false,
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
            crossed: false,
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
        crossed: false,
    }
}

fn pomodoro_marker_style(
    pomodoro_start: Option<DateTime<Local>>,
    minute_marker: u32,
) -> PomodoroMarkerStyle {
    let Some(start) = pomodoro_start else {
        return PomodoroMarkerStyle::Normal;
    };
    let now = Local::now();
    let elapsed_minutes = (now.minute() + 60 - start.minute()) % 60;
    if elapsed_minutes == 0 && now.signed_duration_since(start).num_minutes() >= 60 {
        return PomodoroMarkerStyle::Hidden;
    }

    let start_minute = start.minute();
    let offset = (minute_marker + 60 - start_minute) % 60;

    if offset < elapsed_minutes {
        PomodoroMarkerStyle::Hidden
    } else if offset < 50 {
        PomodoroMarkerStyle::Work
    } else {
        PomodoroMarkerStyle::Break
    }
}

fn marker_style_color(style: PomodoroMarkerStyle, normal: Color) -> Color {
    match style {
        PomodoroMarkerStyle::Normal => normal,
        PomodoroMarkerStyle::Work => POMODORO_WORK_COLOR,
        PomodoroMarkerStyle::Break => POMODORO_BREAK_COLOR,
        PomodoroMarkerStyle::Hidden => normal,
    }
}

fn write_text(
    grid: &mut [Vec<Cell>],
    x: usize,
    y: usize,
    text: &str,
    fg: Color,
    bg: Option<Color>,
    crossed: bool,
) {
    if y >= grid.len() {
        return;
    }
    for (idx, ch) in text.chars().enumerate() {
        let px = x + idx;
        if px < grid[y].len() {
            let existing_bg = grid[y][px].bg;
            grid[y][px] = Cell {
                ch,
                fg: Some(fg),
                bg: bg.or(existing_bg),
                crossed,
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

fn wrap_text(text: &str, width: usize) -> Vec<String> {
    if width == 0 {
        return vec![String::new()];
    }
    if text.is_empty() {
        return vec![String::new()];
    }

    let mut lines = Vec::new();
    let mut current = String::new();

    for word in text.split_whitespace() {
        let word_len = word.chars().count();
        let current_len = current.chars().count();

        if current.is_empty() {
            if word_len <= width {
                current.push_str(word);
            } else {
                for chunk in word.chars().collect::<Vec<_>>().chunks(width) {
                    lines.push(chunk.iter().collect());
                }
            }
        } else if current_len + 1 + word_len <= width {
            current.push(' ');
            current.push_str(word);
        } else {
            lines.push(current);
            current = String::new();
            if word_len <= width {
                current.push_str(word);
            } else {
                for chunk in word.chars().collect::<Vec<_>>().chunks(width) {
                    lines.push(chunk.iter().collect());
                }
            }
        }
    }

    if !current.is_empty() {
        lines.push(current);
    }

    if lines.is_empty() {
        vec![String::new()]
    } else {
        lines
    }
}

fn handle_editing_key(state: &mut AppState, code: &KeyCode) -> bool {
    let Some(editing) = state.editing_todo.as_mut() else {
        return false;
    };

    match code {
        KeyCode::Char(c) => {
            editing.buffer.push(*c);
            true
        }
        KeyCode::Backspace => {
            editing.buffer.pop();
            true
        }
        KeyCode::Enter | KeyCode::Esc => {
            save_editing_todo(state);
            true
        }
        _ => true,
    }
}

fn save_editing_todo(state: &mut AppState) {
    let Some(editing) = state.editing_todo.take() else {
        return;
    };

    if let Some(items) = state.todos.get_mut(&editing.date) {
        if let Some(item) = items.get_mut(editing.index) {
            if editing.buffer.trim().is_empty() && editing.was_placeholder {
                item.text = editing.original_text;
                item.is_placeholder = true;
            } else {
                item.text = editing.buffer;
                item.is_placeholder = false;
            }
        }
    }
}

fn load_todos() -> io::Result<HashMap<NaiveDate, Vec<TodoItem>>> {
    let path = Path::new(TODO_SAVE_PATH);
    if !path.exists() {
        return Ok(HashMap::new());
    }

    let raw = fs::read_to_string(path)?;
    let stored: StoredTodos = serde_json::from_str(&raw)
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
    let mut todos = HashMap::new();

    for day in stored.days {
        let date = NaiveDate::parse_from_str(&day.date, "%Y-%m-%d")
            .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
        let items = day
            .items
            .into_iter()
            .map(|item| TodoItem {
                text: item.text,
                done: item.done,
                is_placeholder: item.is_placeholder,
            })
            .collect::<Vec<_>>();
        if !items.is_empty() {
            todos.insert(date, items);
        }
    }

    Ok(todos)
}

fn save_todos(todos: &HashMap<NaiveDate, Vec<TodoItem>>) -> io::Result<()> {
    let mut days = todos
        .iter()
        .filter_map(|(date, items)| {
            if items.is_empty() {
                None
            } else {
                Some(StoredTodoDay {
                    date: date.format("%Y-%m-%d").to_string(),
                    items: items
                        .iter()
                        .map(|item| StoredTodoItem {
                            text: item.text.clone(),
                            done: item.done,
                            is_placeholder: item.is_placeholder,
                        })
                        .collect(),
                })
            }
        })
        .collect::<Vec<_>>();
    days.sort_by(|left, right| left.date.cmp(&right.date));

    let payload = StoredTodos { days };
    let json = serde_json::to_string_pretty(&payload)
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
    fs::write(TODO_SAVE_PATH, json)?;
    Ok(())
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
