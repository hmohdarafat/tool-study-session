use std::collections::HashMap;
use std::env;
use std::f64::consts::PI;
use std::fs;
use std::io::{self, Stdout, Write, stdout};
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use std::thread;
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use chrono::{DateTime, Datelike, Local, NaiveDate, Timelike};
use crossterm::cursor::{Hide, MoveTo, Show};
use crossterm::event::{
    self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, MouseButton, MouseEventKind,
};
use crossterm::style::{Attribute, Color, Print, ResetColor, SetAttribute, SetForegroundColor};
use crossterm::terminal::{
    self, EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use crossterm::{ExecutableCommand, QueueableCommand};
use serde::{Deserialize, Serialize};

const CLOCK_HEIGHT: u16 = 37;
const CELL_ASPECT_RATIO: f64 = 0.5;
const HOUR_MARKER_SCALE: f64 = 0.96;
const MINUTE_LABEL_SCALE: f64 = 0.78;
const MINUTE_DOT_SCALE: f64 = 0.9;
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
const PANEL_GAP: u16 = 2;
const POMODORO_WORK_COLOR: Color = Color::DarkGreen;
const POMODORO_BREAK_COLOR: Color = Color::DarkRed;
const START_BUTTON_COLOR: Color = Color::Magenta;
const QUIT_PROMPT_COLOR: Color = Color::Yellow;
const POMODORO_STATUS_COLOR: Color = Color::White;
const POMODORO_INACTIVE_COLOR: Color = Color::Grey;
const NOISE_LABEL_COLOR: Color = Color::White;
const NOISE_BUTTON_COLOR: Color = Color::Cyan;
const NOISE_BUTTON_ACTIVE_BG: Color = Color::DarkBlue;
const NOISE_VOLUME_COLOR: Color = Color::White;
const TODO_FILE_NAME: &str = "todos.json";
const FOCUS_TRACKER_SECONDS: usize = 50 * 60;
const FOCUS_GRAPH_WIDTH: usize = 50;
const FOCUS_LIVE_SECONDS: usize = FOCUS_GRAPH_WIDTH;
const FOCUS_SUMMARY_ROWS: usize = 6;
const FOCUS_HEADER_COLOR: Color = Color::Yellow;
const FOCUS_FOCUSED_COLOR: Color = Color::Green;
const FOCUS_BREAKING_COLOR: Color = Color::Yellow;
const FOCUS_BROKEN_COLOR: Color = Color::Red;
const FOCUS_AXIS_COLOR: Color = Color::DarkGrey;
const FOCUS_ACTIVE_BG: Color = Color::DarkBlue;
const GRADIENT_SPEED: f64 = 24.0;
const GRADIENT_X_SCALE: f64 = 1.8;
const GRADIENT_Y_SCALE: f64 = 1.4;
const GRADIENT_REFRESH_INTERVAL: Duration = Duration::from_millis(100);
const FOCUS_TINT_TRANSITION_MS: u64 = 1_300;
const POMODORO_TINT_TRANSITION_MS: u64 = 1_000;

struct AppState {
    font: FontSetting,
    original_font: FontSetting,
    calendar_year: i32,
    calendar_month: u32,
    selected_date: NaiveDate,
    pomodoro_start: Option<DateTime<Local>>,
    focus_level: FocusLevel,
    focus_tint_from: FocusLevel,
    focus_tint_started_at: Option<Instant>,
    pomodoro_tint_from: f32,
    pomodoro_tint_to: f32,
    pomodoro_tint_started_at: Option<Instant>,
    focus_samples: [Option<FocusLevel>; FOCUS_TRACKER_SECONDS],
    last_focus_sample_second: Option<usize>,
    selected_noise: Option<NoiseKind>,
    noise_volume: u8,
    noise_audio: NoiseAudio,
    todos: HashMap<NaiveDate, Vec<TodoItem>>,
    editing_todo: Option<EditingTodo>,
    window_fitted: bool,
    pending_height_fit: bool,
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

#[derive(Clone, Copy, PartialEq, Eq)]
enum NoiseKind {
    Brown,
    White,
    Pink,
}

struct NoiseButtonHitbox {
    kind: Option<NoiseKind>,
    x: u16,
    end_x: u16,
}

struct NoiseButtons {
    y: u16,
    buttons: Vec<NoiseButtonHitbox>,
    volume_down_x: u16,
    volume_down_end_x: u16,
    volume_up_x: u16,
    volume_up_end_x: u16,
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
    noise: NoiseButtons,
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

struct PomodoroStatus {
    work_countdown: String,
    break_countdown: String,
    work_color: Color,
    break_color: Color,
}

struct NoiseAudio {
    child: Option<Child>,
    stop_signal: Option<Arc<AtomicBool>>,
    worker: Option<JoinHandle<()>>,
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
    focus_buttons: Vec<FocusButtonHitbox>,
    todo_checks: Vec<TodoHitbox>,
    todo_deletes: Vec<TodoHitbox>,
    todo_texts: Vec<TodoTextHitbox>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum FocusLevel {
    Focused,
    Breaking,
    Broken,
}

struct FocusButtonHitbox {
    level: FocusLevel,
    x: u16,
    end_x: u16,
    y: u16,
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
        focus_level: FocusLevel::Focused,
        focus_tint_from: FocusLevel::Focused,
        focus_tint_started_at: None,
        pomodoro_tint_from: 0.0,
        pomodoro_tint_to: 0.0,
        pomodoro_tint_started_at: None,
        focus_samples: [None; FOCUS_TRACKER_SECONDS],
        last_focus_sample_second: None,
        selected_noise: None,
        noise_volume: 50,
        noise_audio: NoiseAudio::new(),
        todos: load_todos().unwrap_or_default(),
        editing_todo: None,
        window_fitted: false,
        pending_height_fit: true,
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

    loop {
        if pomodoro_finished(state.pomodoro_start) {
            toggle_pomodoro(state);
            state.pending_height_fit = true;
            controls = render(stdout, state)?;
            continue;
        }

        if event::poll(gradient_poll_interval())? {
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
                        KeyCode::Esc => {
                            save_editing_todo(state);
                            state.quit_prompt = true;
                            controls = render(stdout, state)?;
                        }
                        KeyCode::Char('q') | KeyCode::Char('Q') => {
                            save_editing_todo(state);
                            set_focus_level(state, FocusLevel::Focused);
                            controls = render(stdout, state)?;
                        }
                        KeyCode::Char('w') | KeyCode::Char('W') => {
                            save_editing_todo(state);
                            set_focus_level(state, FocusLevel::Breaking);
                            controls = render(stdout, state)?;
                        }
                        KeyCode::Char('e') | KeyCode::Char('E') => {
                            save_editing_todo(state);
                            set_focus_level(state, FocusLevel::Broken);
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
                        KeyCode::Char(' ') => {
                            save_editing_todo(state);
                            toggle_pomodoro(state);
                            state.pending_height_fit = true;
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
                        if mouse.row == controls.noise.y {
                            if let Some(button) = controls
                                .noise
                                .buttons
                                .iter()
                                .find(|button| (button.x..button.end_x).contains(&mouse.column))
                            {
                                select_noise(state, button.kind);
                            } else if (controls.noise.volume_down_x
                                ..controls.noise.volume_down_end_x)
                                .contains(&mouse.column)
                            {
                                adjust_noise_volume(state, -1);
                            } else if (controls.noise.volume_up_x
                                ..controls.noise.volume_up_end_x)
                                .contains(&mouse.column)
                            {
                                adjust_noise_volume(state, 1);
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
                            state.pending_height_fit = true;
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
                            state.pending_height_fit = true;
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
                            state.pending_height_fit = true;
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
                        if let Some(hit) = controls.calendar.focus_buttons.iter().find(|hit| {
                            mouse.row == hit.y && (hit.x..hit.end_x).contains(&mouse.column)
                        }) {
                            save_editing_todo(state);
                            set_focus_level(state, hit.level);
                        }
                        if mouse.row == controls.start.y
                            && (controls.start.x..controls.start.end_x).contains(&mouse.column)
                        {
                            save_editing_todo(state);
                            toggle_pomodoro(state);
                            state.pending_height_fit = true;
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
            controls = render(stdout, state)?;
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

fn gradient_poll_interval() -> Duration {
    duration_until_next_minute().min(GRADIENT_REFRESH_INTERVAL)
}

fn select_noise(state: &mut AppState, kind: Option<NoiseKind>) {
    match kind {
        Some(kind) => {
            state.noise_volume = 0;
            if state
                .noise_audio
                .play(kind, state.noise_volume as f32 / 100.0)
                .is_ok()
            {
                state.selected_noise = Some(kind);
            }
        }
        None => {
            state.noise_audio.stop();
            state.selected_noise = None;
        }
    }
}

fn adjust_noise_volume(state: &mut AppState, delta: i16) {
    let next = (state.noise_volume as i16 + delta).clamp(0, 100) as u8;
    if next == state.noise_volume {
        return;
    }

    state.noise_volume = next;
    if let Some(kind) = state.selected_noise {
        let _ = state
            .noise_audio
            .play(kind, state.noise_volume as f32 / 100.0);
    }
}

fn render(stdout: &mut Stdout, state: &mut AppState) -> io::Result<UiControls> {
    record_focus_sample(state);

    let (mut width, mut height) = terminal::size()?;
    let preview_calendar = build_calendar_panel(
        state.calendar_year,
        state.calendar_month,
        state.selected_date,
        &state.todos,
        state.todos.get(&state.selected_date),
        state.editing_todo.as_ref(),
        state.pomodoro_start,
        state.focus_level,
        &state.focus_samples,
        0,
    );
    let minimum_height = (preview_calendar.grid.len() as u16).saturating_add(2);
    if height < minimum_height {
        request_terminal_size(stdout, width, minimum_height)?;
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
    let quit_hint = "Esc = quit  q/w/e = focus";
    let footer_width = quit_hint.len() as u16;
    let clock_header_rows: u16 = 2;
    if !state.window_fitted {
        let preferred_clock = build_clock(
            preferred_width(make_odd(CLOCK_HEIGHT.min(content_height.max(11)))),
            content_height.saturating_sub(clock_header_rows),
            state.pomodoro_start,
        );
        let preferred_clock_block_width =
            visible_grid_width(&preferred_clock).max(noise_controls_width(state.noise_volume));
        let preferred_content_width = calendar_width + PANEL_GAP + preferred_clock_block_width;
        let preferred_total_width = preferred_content_width.max(footer_width);
        request_terminal_size(stdout, preferred_total_width, height)?;
        let (new_width, new_height) = terminal::size()?;
        width = new_width;
        height = new_height;
        state.window_fitted = true;
    }
    let content_height = height.saturating_sub(2);
    let available_clock_width = width.saturating_sub(calendar_width + PANEL_GAP);
    let calendar_x = 0;
    let clock_x = calendar_x + calendar_width + PANEL_GAP;
    let noise_y: u16 = 0;
    let clock_y: u16 = 2;
    let start_label = if state.pomodoro_start.is_some() {
        "[Stop]"
    } else {
        "[Start]"
    };
    let pomodoro_status = pomodoro_status(state.pomodoro_start);
    let clock_header_rows = clock_y;
    let preferred_clock = build_clock(
        preferred_width(make_odd(CLOCK_HEIGHT)),
        CLOCK_HEIGHT,
        state.pomodoro_start,
    );
    let preferred_clock_height = preferred_clock.len() as u16;
    let available_clock_height = content_height.saturating_sub(clock_header_rows);
    let mut clock = build_clock(
        available_clock_width,
        available_clock_height,
        state.pomodoro_start,
    );
    let mut visible_clock_width = visible_grid_width(&clock);
    let mut clock_height = clock.len() as u16;
    let initial_start_button_y = clock_y + preferred_clock_height.saturating_add(1);
    let initial_status_countdown_y = if pomodoro_status.is_some() {
        initial_start_button_y.saturating_add(3)
    } else {
        initial_start_button_y
    };
    let desired_height = preview_calendar
        .grid
        .len()
        .max((initial_status_countdown_y as usize).saturating_add(1))
        .saturating_add(1) as u16;
    if state.pending_height_fit && height != desired_height {
        request_terminal_size(stdout, width, desired_height)?;
        let (new_width, new_height) = terminal::size()?;
        width = new_width;
        height = new_height;

        let content_height = height.saturating_sub(2);
        let available_clock_width = width.saturating_sub(calendar_width + PANEL_GAP);
        let available_clock_height = content_height.saturating_sub(clock_header_rows);
        clock = build_clock(
            available_clock_width,
            available_clock_height,
            state.pomodoro_start,
        );
        visible_clock_width = visible_grid_width(&clock);
        clock_height = clock.len() as u16;
    }
    state.pending_height_fit = false;
    let start_button_y = clock_y + clock_height.saturating_add(1);
    let status_phase_y = start_button_y.saturating_add(2);
    let status_countdown_y = start_button_y.saturating_add(3);
    let start_button = ActionButton {
        x: clock_x + visible_clock_width.saturating_sub(start_label.len() as u16) / 2,
        end_x: clock_x
            + visible_clock_width.saturating_sub(start_label.len() as u16) / 2
            + start_label.len() as u16,
        y: start_button_y,
    };
    let font_buttons = FontButtons {
        minus_x: 0,
        plus_x: 4,
        y: height.saturating_sub(FONT_BUTTON_Y_PADDING),
    };
    let quit_hint_x = width.saturating_sub(quit_hint.len() as u16);
    let quit_prompt = "Save todos before quitting? (y/n)";
    let calendar = build_calendar_panel(
        state.calendar_year,
        state.calendar_month,
        state.selected_date,
        &state.todos,
        state.todos.get(&state.selected_date),
        state.editing_todo.as_ref(),
        state.pomodoro_start,
        state.focus_level,
        &state.focus_samples,
        calendar_x,
    );

    let mut frame = build_gradient_frame(
        width as usize,
        height as usize,
        state.focus_tint_from,
        state.focus_level,
        focus_tint_progress(state),
        pomodoro_tint_alpha(state),
    );
    overlay_grid(
        &mut frame,
        &calendar.grid,
        calendar_x as usize,
        0,
    );
    overlay_grid(&mut frame, &clock, clock_x as usize, clock_y as usize);
    let noise_controls = render_noise_buttons(
        &mut frame,
        clock_x,
        visible_clock_width,
        noise_y,
        state.selected_noise,
        state.noise_volume,
    );
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
    if let Some(status) = pomodoro_status {
        let work_label = "Work / Study";
        let break_label = "Break";
        let left_width = work_label.len().max(status.work_countdown.len()) as u16;
        let right_width = break_label.len().max(status.break_countdown.len()) as u16;
        let block_width = left_width + 3 + right_width;
        let block_x = clock_x + visible_clock_width.saturating_sub(block_width) / 2;
        let right_x = block_x + left_width + 3;
        let work_countdown_x =
            block_x + left_width.saturating_sub(status.work_countdown.len() as u16) / 2;
        let break_countdown_x =
            right_x + right_width.saturating_sub(status.break_countdown.len() as u16) / 2;

        write_text(
            &mut frame,
            block_x as usize,
            status_phase_y as usize,
            work_label,
            status.work_color,
            None,
            false,
        );
        write_text(
            &mut frame,
            (block_x + left_width) as usize,
            status_phase_y as usize,
            " | ",
            POMODORO_STATUS_COLOR,
            None,
            false,
        );
        write_text(
            &mut frame,
            right_x as usize,
            status_phase_y as usize,
            break_label,
            status.break_color,
            None,
            false,
        );
        write_text(
            &mut frame,
            work_countdown_x as usize,
            status_countdown_y as usize,
            &status.work_countdown,
            status.work_color,
            None,
            false,
        );
        write_text(
            &mut frame,
            (block_x + left_width) as usize,
            status_countdown_y as usize,
            " | ",
            POMODORO_STATUS_COLOR,
            None,
            false,
        );
        write_text(
            &mut frame,
            break_countdown_x as usize,
            status_countdown_y as usize,
            &status.break_countdown,
            status.break_color,
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

    for (row, line) in frame.iter().enumerate() {
        stdout.queue(MoveTo(0, row as u16))?;
        write_colored_line(stdout, line)?;
    }

    stdout.flush()?;
    Ok(UiControls {
        font: font_buttons,
        noise: noise_controls,
        calendar: calendar.controls,
        start: start_button,
    })
}

fn request_terminal_size(stdout: &mut Stdout, width: u16, height: u16) -> io::Result<()> {
    let target_width = width.max(1);
    let target_height = height.max(1);
    let current_size = terminal::size()?;
    if current_size == (target_width, target_height) {
        return Ok(());
    }

    write!(stdout, "\x1b[8;{};{}t", target_height, target_width)?;
    stdout.flush()?;
    let deadline = Instant::now() + Duration::from_millis(40);
    while Instant::now() < deadline {
        if terminal::size()? == (target_width, target_height) {
            break;
        }
        thread::sleep(Duration::from_millis(5));
    }
    Ok(())
}

fn noise_controls_width(noise_volume: u8) -> u16 {
    let label = "Noise (Sandpaper):";
    let volume_label = format!("Vol [-] {:>3}% [+]", noise_volume);
    let button_labels = ["[None]", "[Brown]", "[White]", "[Pink]"];
    let buttons_width = button_labels
        .iter()
        .map(|text| text.len() as u16)
        .sum::<u16>()
        .saturating_add((button_labels.len().saturating_sub(1) as u16) * 1);
    volume_label.len() as u16 + 1 + label.len() as u16 + 1 + buttons_width
}

fn render_noise_buttons(
    frame: &mut [Vec<Cell>],
    clock_x: u16,
    clock_width: u16,
    y: u16,
    selected_noise: Option<NoiseKind>,
    noise_volume: u8,
) -> NoiseButtons {
    let label = "Noise (Sandpaper):";
    let defs = [
        (None, "[None]"),
        (Some(NoiseKind::Brown), "[Brown]"),
        (Some(NoiseKind::White), "[White]"),
        (Some(NoiseKind::Pink), "[Pink]"),
    ];
    let volume_label = format!("Vol [-] {:>3}% [+]", noise_volume);
    let buttons_width = defs
        .iter()
        .map(|(_, text)| text.len() as u16)
        .sum::<u16>()
        .saturating_add((defs.len().saturating_sub(1) as u16) * 1);
    let total_width = volume_label.len() as u16
        + 1
        + label.len() as u16
        + 1
        + buttons_width
        ;
    let start_x = clock_x + clock_width.saturating_sub(total_width) / 2;

    write_text(
        frame,
        start_x as usize,
        y as usize,
        &volume_label,
        NOISE_VOLUME_COLOR,
        None,
        false,
    );
    let volume_down_x = start_x + 4;
    let volume_down_end_x = volume_down_x + 3;
    let volume_up_x = start_x + volume_label.len() as u16 - 3;
    let volume_up_end_x = volume_up_x + 3;

    let label_x = start_x + volume_label.len() as u16 + 1;
    write_text(
        frame,
        label_x as usize,
        y as usize,
        label,
        NOISE_LABEL_COLOR,
        None,
        false,
    );

    let mut cursor_x = label_x + label.len() as u16 + 1;
    let mut buttons = Vec::new();
    for (idx, (kind, text)) in defs.iter().enumerate() {
        let is_selected = selected_noise == *kind;
        write_text(
            frame,
            cursor_x as usize,
            y as usize,
            text,
            NOISE_BUTTON_COLOR,
            if is_selected {
                Some(NOISE_BUTTON_ACTIVE_BG)
            } else {
                None
            },
            false,
        );
        buttons.push(NoiseButtonHitbox {
            kind: *kind,
            x: cursor_x,
            end_x: cursor_x + text.len() as u16,
        });
        cursor_x += text.len() as u16;
        if idx + 1 < defs.len() {
            cursor_x += 1;
        }
    }

    NoiseButtons {
        y,
        buttons,
        volume_down_x,
        volume_down_end_x,
        volume_up_x,
        volume_up_end_x,
    }
}

fn visible_grid_width(grid: &[Vec<Cell>]) -> u16 {
    let mut max_width = 0usize;
    for row in grid {
        if let Some(last_used) = row
            .iter()
            .rposition(|cell| cell.ch != ' ' || cell.fg.is_some() || cell.bg.is_some())
        {
            max_width = max_width.max(last_used + 1);
        }
    }
    max_width as u16
}

fn trim_grid(grid: Vec<Vec<Cell>>) -> Vec<Vec<Cell>> {
    if grid.is_empty() || grid[0].is_empty() {
        return grid;
    }

    let mut min_x = usize::MAX;
    let mut max_x = 0usize;
    let mut min_y = usize::MAX;
    let mut max_y = 0usize;
    let mut found = false;

    for (y, row) in grid.iter().enumerate() {
        for (x, cell) in row.iter().enumerate() {
            if cell.ch != ' ' || cell.fg.is_some() || cell.bg.is_some() {
                min_x = min_x.min(x);
                max_x = max_x.max(x);
                min_y = min_y.min(y);
                max_y = max_y.max(y);
                found = true;
            }
        }
    }

    if !found {
        return grid;
    }

    grid.into_iter()
        .skip(min_y)
        .take(max_y - min_y + 1)
        .map(|row| {
            row.into_iter()
                .skip(min_x)
                .take(max_x - min_x + 1)
                .collect()
        })
        .collect()
}

fn build_gradient_frame(
    width: usize,
    height: usize,
    focus_tint_from: FocusLevel,
    focus_level: FocusLevel,
    tint_progress: f32,
    tint_alpha: f32,
) -> Vec<Vec<Cell>> {
    (0..height)
        .map(|y| {
            (0..width)
                .map(|x| Cell {
                    ch: ' ',
                    fg: None,
                    bg: Some(gradient_color(
                        x,
                        y,
                        width,
                        height,
                        focus_tint_from,
                        focus_level,
                        tint_progress,
                        tint_alpha,
                    )),
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

fn gradient_color(
    x: usize,
    y: usize,
    width: usize,
    height: usize,
    focus_tint_from: FocusLevel,
    focus_level: FocusLevel,
    tint_progress: f32,
    tint_alpha: f32,
) -> Color {
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

    let field = soft_field(phase, x_ratio, y_ratio);
    let drift = soft_field(phase + 0.19, y_ratio * 0.9 + 0.07, x_ratio * 0.8 + 0.11);
    let shimmer = soft_field(
        phase + 0.41,
        x_ratio * 0.65 + 0.17,
        y_ratio * 0.7 + pseudo_offset(x, y),
    );
    let bloom = soft_field(phase + 0.73, x_ratio * 0.55 + 0.31, y_ratio * 0.6 + 0.13);
    let glow = soft_field(phase + 0.92, y_ratio * 0.5 + 0.29, x_ratio * 0.75 + 0.23);
    let ember = soft_field(phase + 1.17, x_ratio * 0.48 + 0.43, y_ratio * 0.52 + 0.19);
    let tide = soft_field(phase + 1.36, y_ratio * 0.62 + 0.37, x_ratio * 0.58 + 0.27);

    let red = channel(field, bloom, ember, glow, 6.0, 34.0, 0.11);
    let green = channel(drift, glow, tide, shimmer, 8.0, 30.0, 0.37);
    let blue = channel(shimmer, tide, field, glow, 14.0, 44.0, 0.63);
    let (red_bias, green_bias, blue_bias) = interpolate_bias(
        (0.0, 0.0, 0.0),
        interpolate_bias(
            focus_background_bias(focus_tint_from),
            focus_background_bias(focus_level),
            tint_progress,
        ),
        tint_alpha,
    );

    Color::Rgb {
        r: ((red as f32 + red_bias).round().clamp(0.0, 255.0)) as u8,
        g: ((green as f32 + green_bias).round().clamp(0.0, 255.0)) as u8,
        b: ((blue as f32 + blue_bias).round().clamp(0.0, 255.0)) as u8,
    }
}

fn interpolate_bias(from: (f32, f32, f32), to: (f32, f32, f32), progress: f32) -> (f32, f32, f32) {
    (
        from.0 + (to.0 - from.0) * progress,
        from.1 + (to.1 - from.1) * progress,
        from.2 + (to.2 - from.2) * progress,
    )
}

fn focus_background_bias(level: FocusLevel) -> (f32, f32, f32) {
    match level {
        FocusLevel::Focused => (0.0, 20.0, 3.0),
        FocusLevel::Breaking => (30.0, 22.0, 0.0),
        FocusLevel::Broken => (38.0, 0.0, 0.0),
    }
}

fn soft_field(phase: f64, x_ratio: f64, y_ratio: f64) -> f64 {
    let broad = ((x_ratio * GRADIENT_X_SCALE + phase) * 2.0 * PI).sin();
    let tall = ((y_ratio * GRADIENT_Y_SCALE - phase * 0.7 + 0.23) * 2.0 * PI).sin();
    let diagonal = (((x_ratio * 0.9 + y_ratio * 0.7) * 1.3 + phase * 0.5 + 0.41) * 2.0 * PI).sin();
    0.45 * broad + 0.35 * tall + 0.20 * diagonal
}

fn pseudo_offset(x: usize, y: usize) -> f64 {
    let seed = ((x as u64).wrapping_mul(73_856_093)) ^ ((y as u64).wrapping_mul(19_349_663));
    (seed % 10_000) as f64 / 10_000.0 * 0.08
}

fn channel(a: f64, b: f64, c: f64, d: f64, base: f64, amplitude: f64, phase: f64) -> u8 {
    let blend = 0.5 + 0.5 * (0.34 * a + 0.24 * b + 0.22 * c + 0.20 * d + phase).sin();
    (base + amplitude * blend).round() as u8
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
    pomodoro_start: Option<DateTime<Local>>,
    focus_level: FocusLevel,
    focus_samples: &[Option<FocusLevel>; FOCUS_TRACKER_SECONDS],
    calendar_x: u16,
) -> CalendarPanel {
    let width = 58usize;
    let text_x = 6usize;
    let text_width = width.saturating_sub(text_x + 1);
    let year_x = 14usize;
    let focus_rows = 23usize;
    let todo_rows = todos
        .map(|items| {
            items
                .iter()
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
    let height = 13 + todo_rows + focus_rows;
    let mut grid = vec![vec![blank_cell(); width]; height];
    let buttons = build_calendar_buttons(year, month, calendar_x);
    let mut date_hits = Vec::new();
    let mut todo_check_hits = Vec::new();
    let mut todo_delete_hits = Vec::new();
    let mut todo_text_hits = Vec::new();
    let mut focus_button_hits = Vec::new();
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
        for (index, item) in items.iter().enumerate() {
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
    current_row += 2;
    draw_focus_tracker(
        &mut grid,
        calendar_x,
        current_row,
        pomodoro_start,
        focus_level,
        focus_samples,
        &mut focus_button_hits,
    );

    CalendarPanel {
        grid,
        controls: CalendarUiControls {
            buttons,
            dates: date_hits,
            add_button,
            focus_buttons: focus_button_hits,
            todo_checks: todo_check_hits,
            todo_deletes: todo_delete_hits,
            todo_texts: todo_text_hits,
        },
    }
}

fn draw_focus_tracker(
    grid: &mut [Vec<Cell>],
    calendar_x: u16,
    start_y: usize,
    pomodoro_start: Option<DateTime<Local>>,
    focus_level: FocusLevel,
    focus_samples: &[Option<FocusLevel>; FOCUS_TRACKER_SECONDS],
    focus_button_hits: &mut Vec<FocusButtonHitbox>,
) {
    let buttons_y = start_y + 2;
    let live_label_y = start_y + 4;
    let live_graph_y = start_y + 5;
    let summary_label_y = start_y + 11;
    let summary_graph_y = start_y + 12;
    let stats_y = start_y + 21;
    let graph_x = 6usize;
    let current_second = focus_elapsed_second(pomodoro_start);

    write_text(
        grid,
        0,
        start_y,
        "Focus tracker",
        FOCUS_HEADER_COLOR,
        None,
        false,
    );

    let mut button_x = 0usize;
    for (label, level) in [
        ("[Focused]", FocusLevel::Focused),
        ("[Focus breaking]", FocusLevel::Breaking),
        ("[Focus broken]", FocusLevel::Broken),
    ] {
        let color = focus_level_color(level);
        let bg = if level == focus_level {
            Some(FOCUS_ACTIVE_BG)
        } else {
            None
        };
        write_text(grid, button_x, buttons_y, label, color, bg, false);
        focus_button_hits.push(FocusButtonHitbox {
            level,
            x: calendar_x + button_x as u16,
            end_x: calendar_x + button_x as u16 + label.len() as u16,
            y: buttons_y as u16,
        });
        button_x += label.len() + 1;
    }

    write_text(
        grid,
        0,
        live_label_y,
        "Live 50s",
        FOCUS_HEADER_COLOR,
        None,
        false,
    );
    draw_focus_axis(grid, live_graph_y);
    draw_live_focus_graph(grid, graph_x, live_graph_y, current_second, focus_samples);
    write_text(
        grid,
        graph_x,
        live_graph_y + 4,
        "-49s      -40s      -30s      -20s      -10s   now",
        FOCUS_AXIS_COLOR,
        None,
        false,
    );

    write_text(
        grid,
        0,
        summary_label_y,
        "Full 50m average",
        FOCUS_HEADER_COLOR,
        None,
        false,
    );
    draw_summary_focus_axis(grid, summary_graph_y);
    draw_summary_focus_graph(grid, graph_x, summary_graph_y, focus_samples);
    write_text(
        grid,
        graph_x,
        summary_graph_y + FOCUS_SUMMARY_ROWS + 1,
        "0         10        20        30        40       50m",
        FOCUS_AXIS_COLOR,
        None,
        false,
    );

    let (focused, breaking, broken) = focus_percentages(focus_samples);
    write_text(
        grid,
        0,
        stats_y,
        &format!("Focused {focused:>3}%  Breaking {breaking:>3}%  Broken {broken:>3}%"),
        FOCUS_AXIS_COLOR,
        None,
        false,
    );
    if current_second.is_some_and(|second| second >= FOCUS_TRACKER_SECONDS) {
        if let Some((level, start_second, end_second)) = least_productive_interval(focus_samples) {
            write_text(
                grid,
                0,
                stats_y + 1,
                &format!(
                    "Least productive: {} to {} ({})",
                    format_focus_time(start_second),
                    format_focus_time(end_second),
                    focus_level_label(level)
                ),
                focus_level_color(level),
                None,
                false,
            );
        }
    }
}

fn draw_focus_axis(grid: &mut [Vec<Cell>], graph_y: usize) {
    for (row_offset, label) in [(0usize, "1.0 |"), (1, "0.5 |"), (2, "0.0 |")] {
        write_text(
            grid,
            0,
            graph_y + row_offset,
            label,
            FOCUS_AXIS_COLOR,
            None,
            false,
        );
    }
}

fn draw_summary_focus_axis(grid: &mut [Vec<Cell>], graph_y: usize) {
    for (row_offset, label) in [
        (0usize, "1.0 |"),
        (1, "0.8 |"),
        (2, "0.6 |"),
        (3, "0.4 |"),
        (4, "0.2 |"),
        (5, "0.0 |"),
    ] {
        write_text(
            grid,
            0,
            graph_y + row_offset,
            label,
            FOCUS_AXIS_COLOR,
            None,
            false,
        );
    }
}

fn draw_live_focus_graph(
    grid: &mut [Vec<Cell>],
    graph_x: usize,
    graph_y: usize,
    current_second: Option<usize>,
    focus_samples: &[Option<FocusLevel>; FOCUS_TRACKER_SECONDS],
) {
    let live_start = current_second
        .map(|second| second.saturating_sub(FOCUS_LIVE_SECONDS.saturating_sub(1)))
        .unwrap_or(0);

    for column in 0..FOCUS_GRAPH_WIDTH {
        let x = graph_x + column;
        for row_offset in 0..3 {
            write_text(
                grid,
                x,
                graph_y + row_offset,
                ".",
                FOCUS_AXIS_COLOR,
                None,
                false,
            );
        }

        let sample_second = live_start + column;
        if sample_second >= FOCUS_TRACKER_SECONDS {
            continue;
        }
        if current_second
            .map(|second| sample_second > second)
            .unwrap_or(false)
        {
            continue;
        }
        if let Some(level) = focus_samples[sample_second] {
            let y = graph_y + focus_level_row(level);
            write_text(grid, x, y, "*", focus_level_color(level), None, false);
        }
    }

    if let Some(current_second) = current_second {
        if current_second < FOCUS_TRACKER_SECONDS {
            let x =
                graph_x + (current_second.saturating_sub(live_start)).min(FOCUS_GRAPH_WIDTH - 1);
            if let Some(level) = focus_samples[current_second] {
                let y = graph_y + focus_level_row(level);
                write_text(grid, x, y, "o", focus_level_color(level), None, false);
            }
        }
    }
}

fn draw_summary_focus_graph(
    grid: &mut [Vec<Cell>],
    graph_x: usize,
    graph_y: usize,
    focus_samples: &[Option<FocusLevel>; FOCUS_TRACKER_SECONDS],
) {
    for column in 0..FOCUS_GRAPH_WIDTH {
        let x = graph_x + column;
        for row_offset in 0..FOCUS_SUMMARY_ROWS {
            write_text(
                grid,
                x,
                graph_y + row_offset,
                ".",
                FOCUS_AXIS_COLOR,
                None,
                false,
            );
        }

        if let Some(average) = average_focus_score_for_summary_column(focus_samples, column) {
            let y = graph_y + focus_average_row(average);
            write_text(grid, x, y, "*", focus_average_color(average), None, false);
        }
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

    trim_grid(grid)
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

fn record_focus_sample(state: &mut AppState) {
    let Some(current_second) = focus_elapsed_second(state.pomodoro_start) else {
        return;
    };
    if current_second >= FOCUS_TRACKER_SECONDS {
        return;
    }

    let start_second = state
        .last_focus_sample_second
        .map(|second| second.saturating_add(1))
        .unwrap_or(0)
        .min(current_second);

    for second in start_second..=current_second {
        state.focus_samples[second] = Some(state.focus_level);
    }
    state.last_focus_sample_second = Some(current_second);
}

fn set_focus_level(state: &mut AppState, level: FocusLevel) {
    record_focus_sample(state);
    if state.focus_level != level {
        state.focus_tint_from = state.focus_level;
        state.focus_tint_started_at = Some(Instant::now());
    }
    state.focus_level = level;
    record_focus_sample(state);
}

fn toggle_pomodoro(state: &mut AppState) {
    let current_alpha = pomodoro_tint_alpha(state);
    let was_active = state.pomodoro_start.is_some();
    state.pomodoro_tint_from = current_alpha;
    state.pomodoro_tint_to = if was_active { 0.0 } else { 1.0 };
    state.pomodoro_tint_started_at = Some(Instant::now());

    if was_active {
        state.pomodoro_start = None;
        state.last_focus_sample_second = None;
    } else {
        set_focus_level(state, FocusLevel::Focused);
        state.focus_samples = [None; FOCUS_TRACKER_SECONDS];
        state.last_focus_sample_second = None;
        state.pomodoro_start = Some(Local::now());
    }
}

fn focus_tint_progress(state: &mut AppState) -> f32 {
    let Some(started_at) = state.focus_tint_started_at else {
        return 1.0;
    };

    let elapsed = started_at.elapsed();
    let progress = (elapsed.as_secs_f32() / (FOCUS_TINT_TRANSITION_MS as f32 / 1_000.0)).min(1.0);
    if progress >= 1.0 {
        state.focus_tint_started_at = None;
        state.focus_tint_from = state.focus_level;
        1.0
    } else {
        ease_in_out_sine(progress)
    }
}

fn pomodoro_tint_alpha(state: &mut AppState) -> f32 {
    let Some(started_at) = state.pomodoro_tint_started_at else {
        return state.pomodoro_tint_to;
    };

    let elapsed = started_at.elapsed();
    let progress =
        (elapsed.as_secs_f32() / (POMODORO_TINT_TRANSITION_MS as f32 / 1_000.0)).min(1.0);
    if progress >= 1.0 {
        state.pomodoro_tint_started_at = None;
        state.pomodoro_tint_from = state.pomodoro_tint_to;
        state.pomodoro_tint_to = state.pomodoro_tint_from;
        state.pomodoro_tint_from
    } else {
        interpolate_scalar(
            state.pomodoro_tint_from,
            state.pomodoro_tint_to,
            ease_in_out_sine(progress),
        )
    }
}

fn ease_in_out_sine(progress: f32) -> f32 {
    let clamped = progress.clamp(0.0, 1.0);
    0.5 - 0.5 * (PI as f32 * clamped).cos()
}

fn interpolate_scalar(from: f32, to: f32, progress: f32) -> f32 {
    from + (to - from) * progress
}

fn focus_elapsed_second(pomodoro_start: Option<DateTime<Local>>) -> Option<usize> {
    let start = pomodoro_start?;
    let elapsed_seconds = Local::now().signed_duration_since(start).num_seconds();
    if elapsed_seconds < 0 {
        return None;
    }
    Some(elapsed_seconds as usize)
}

fn average_focus_score_for_summary_column(
    focus_samples: &[Option<FocusLevel>; FOCUS_TRACKER_SECONDS],
    column: usize,
) -> Option<f64> {
    let start = column * FOCUS_TRACKER_SECONDS / FOCUS_GRAPH_WIDTH;
    let end = ((column + 1) * FOCUS_TRACKER_SECONDS / FOCUS_GRAPH_WIDTH).min(FOCUS_TRACKER_SECONDS);

    let mut total_score = 0.0;
    let mut sample_count = 0usize;

    for level in focus_samples[start..end].iter().flatten() {
        total_score += focus_level_score(*level);
        sample_count += 1;
    }

    if sample_count == 0 {
        return None;
    }

    Some(total_score / sample_count as f64)
}

fn focus_level_score(level: FocusLevel) -> f64 {
    match level {
        FocusLevel::Focused => 1.0,
        FocusLevel::Breaking => 0.5,
        FocusLevel::Broken => 0.0,
    }
}

fn focus_average_row(average: f64) -> usize {
    let scaled = (average.clamp(0.0, 1.0) * (FOCUS_SUMMARY_ROWS - 1) as f64).round() as usize;
    (FOCUS_SUMMARY_ROWS - 1).saturating_sub(scaled)
}

fn focus_average_color(average: f64) -> Color {
    if average >= 2.0 / 3.0 {
        FOCUS_FOCUSED_COLOR
    } else if average >= 1.0 / 3.0 {
        FOCUS_BREAKING_COLOR
    } else {
        FOCUS_BROKEN_COLOR
    }
}

fn focus_percentages(focus_samples: &[Option<FocusLevel>; FOCUS_TRACKER_SECONDS]) -> (u8, u8, u8) {
    let mut focused = 0usize;
    let mut breaking = 0usize;
    let mut broken = 0usize;

    for level in focus_samples.iter().flatten() {
        match level {
            FocusLevel::Focused => focused += 1,
            FocusLevel::Breaking => breaking += 1,
            FocusLevel::Broken => broken += 1,
        }
    }

    let total = focused + breaking + broken;
    if total == 0 {
        return (0, 0, 0);
    }

    (
        focus_percentage(focused, total),
        focus_percentage(breaking, total),
        focus_percentage(broken, total),
    )
}

fn focus_percentage(count: usize, total: usize) -> u8 {
    ((count * 100 + total / 2) / total) as u8
}

fn focus_level_row(level: FocusLevel) -> usize {
    match level {
        FocusLevel::Focused => 0,
        FocusLevel::Breaking => 1,
        FocusLevel::Broken => 2,
    }
}

fn focus_level_color(level: FocusLevel) -> Color {
    match level {
        FocusLevel::Focused => FOCUS_FOCUSED_COLOR,
        FocusLevel::Breaking => FOCUS_BREAKING_COLOR,
        FocusLevel::Broken => FOCUS_BROKEN_COLOR,
    }
}

fn focus_level_label(level: FocusLevel) -> &'static str {
    match level {
        FocusLevel::Focused => "Focused",
        FocusLevel::Breaking => "Breaking",
        FocusLevel::Broken => "Broken",
    }
}

fn least_productive_interval(
    focus_samples: &[Option<FocusLevel>; FOCUS_TRACKER_SECONDS],
) -> Option<(FocusLevel, usize, usize)> {
    let target_level = if focus_samples
        .iter()
        .flatten()
        .any(|level| *level == FocusLevel::Broken)
    {
        FocusLevel::Broken
    } else if focus_samples
        .iter()
        .flatten()
        .any(|level| *level == FocusLevel::Breaking)
    {
        FocusLevel::Breaking
    } else if focus_samples
        .iter()
        .flatten()
        .any(|level| *level == FocusLevel::Focused)
    {
        FocusLevel::Focused
    } else {
        return None;
    };

    let mut best_start = 0usize;
    let mut best_end = 0usize;
    let mut current_start = None;

    for (second, sample) in focus_samples.iter().enumerate() {
        if *sample == Some(target_level) {
            current_start.get_or_insert(second);
        } else if let Some(start) = current_start.take() {
            if second - start > best_end.saturating_sub(best_start) {
                best_start = start;
                best_end = second;
            }
        }
    }

    if let Some(start) = current_start {
        if FOCUS_TRACKER_SECONDS - start > best_end.saturating_sub(best_start) {
            best_start = start;
            best_end = FOCUS_TRACKER_SECONDS;
        }
    }

    Some((target_level, best_start, best_end))
}

fn format_focus_time(second: usize) -> String {
    let minutes = second / 60;
    let seconds = second % 60;
    format!("{minutes:02}:{seconds:02}")
}

fn pomodoro_status(pomodoro_start: Option<DateTime<Local>>) -> Option<PomodoroStatus> {
    let start = pomodoro_start?;
    let now = Local::now();
    let elapsed_seconds = now.signed_duration_since(start).num_seconds().max(0);

    let (work_remaining, break_remaining, work_color, break_color) = if elapsed_seconds < 50 * 60 {
        (
            50 * 60 - elapsed_seconds,
            10 * 60,
            POMODORO_WORK_COLOR,
            POMODORO_INACTIVE_COLOR,
        )
    } else if elapsed_seconds < 60 * 60 {
        (
            0,
            60 * 60 - elapsed_seconds,
            POMODORO_INACTIVE_COLOR,
            POMODORO_BREAK_COLOR,
        )
    } else {
        (0, 0, POMODORO_INACTIVE_COLOR, POMODORO_INACTIVE_COLOR)
    };

    Some(PomodoroStatus {
        work_countdown: format_countdown(work_remaining),
        break_countdown: format_countdown(break_remaining),
        work_color,
        break_color,
    })
}

fn format_countdown(remaining_seconds: i64) -> String {
    let safe_seconds = remaining_seconds.max(0);
    let minutes = safe_seconds / 60;
    let seconds = safe_seconds % 60;
    format!("{minutes:02}:{seconds:02}")
}

fn pomodoro_finished(pomodoro_start: Option<DateTime<Local>>) -> bool {
    let Some(start) = pomodoro_start else {
        return false;
    };

    Local::now().signed_duration_since(start).num_seconds().max(0) >= 60 * 60
}

impl NoiseAudio {
    fn new() -> Self {
        Self {
            child: None,
            stop_signal: None,
            worker: None,
        }
    }

    fn play(&mut self, kind: NoiseKind, volume: f32) -> io::Result<()> {
        self.stop();

        let mut child = Command::new("aplay")
            .args(["-q", "-t", "raw", "-c", "2", "-r", "44100", "-f", "S16_LE"])
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()?;

        let Some(stdin) = child.stdin.take() else {
            return Err(io::Error::other("audio pipe unavailable"));
        };

        let stop_signal = Arc::new(AtomicBool::new(false));
        let worker_stop = Arc::clone(&stop_signal);
        let worker = thread::spawn(move || {
            stream_noise(stdin, kind, volume, worker_stop);
        });

        self.stop_signal = Some(stop_signal);
        self.worker = Some(worker);
        self.child = Some(child);
        Ok(())
    }

    fn stop(&mut self) {
        if let Some(stop_signal) = self.stop_signal.take() {
            stop_signal.store(true, Ordering::Relaxed);
        }
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
        if let Some(mut child) = self.child.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

impl Drop for NoiseAudio {
    fn drop(&mut self) {
        self.stop();
    }
}

fn stream_noise(mut output: impl Write, kind: NoiseKind, volume: f32, stop: Arc<AtomicBool>) {
    const FRAME_SAMPLES: usize = 1024;
    let mut rng: u64 = 0x1234_5678_9abc_def0;
    let mut brown = 0.0f32;
    let mut pink_b0 = 0.0f32;
    let mut pink_b1 = 0.0f32;
    let mut pink_b2 = 0.0f32;
    let mut pink_b3 = 0.0f32;
    let mut pink_b4 = 0.0f32;
    let mut pink_b5 = 0.0f32;
    let mut pink_b6 = 0.0f32;
    let mut buffer = Vec::with_capacity(FRAME_SAMPLES * 4);

    while !stop.load(Ordering::Relaxed) {
        buffer.clear();
        for _ in 0..FRAME_SAMPLES {
            let white = next_white(&mut rng);
            let mono = match kind {
                NoiseKind::White => white * 0.20,
                NoiseKind::Brown => {
                    brown = (brown + white * 0.018).clamp(-1.0, 1.0);
                    brown * 0.42
                }
                NoiseKind::Pink => {
                    pink_b0 = 0.99886 * pink_b0 + white * 0.0555179;
                    pink_b1 = 0.99332 * pink_b1 + white * 0.0750759;
                    pink_b2 = 0.96900 * pink_b2 + white * 0.153_852;
                    pink_b3 = 0.86650 * pink_b3 + white * 0.3104856;
                    pink_b4 = 0.55000 * pink_b4 + white * 0.5329522;
                    pink_b5 = -0.7616 * pink_b5 - white * 0.0168980;
                    let pink = pink_b0
                        + pink_b1
                        + pink_b2
                        + pink_b3
                        + pink_b4
                        + pink_b5
                        + pink_b6
                        + white * 0.5362;
                    pink_b6 = white * 0.115926;
                    pink * 0.08
                }
            };
            let sample = ((mono * volume).clamp(-1.0, 1.0) * i16::MAX as f32) as i16;
            let bytes = sample.to_le_bytes();
            buffer.extend_from_slice(&bytes);
            buffer.extend_from_slice(&bytes);
        }

        if output.write_all(&buffer).is_err() {
            break;
        }
    }

    let _ = output.flush();
}

fn next_white(rng: &mut u64) -> f32 {
    *rng ^= *rng << 13;
    *rng ^= *rng >> 7;
    *rng ^= *rng << 17;
    ((*rng as f64 / u64::MAX as f64) * 2.0 - 1.0) as f32
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
            state.pending_height_fit = true;
        }
    }
}

fn load_todos() -> io::Result<HashMap<NaiveDate, Vec<TodoItem>>> {
    let data_path = todo_file_path();
    let legacy_path = PathBuf::from(TODO_FILE_NAME);
    let path = if data_path.exists() || !legacy_path.exists() {
        data_path
    } else {
        legacy_path
    };

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
    let path = todo_file_path();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, json)?;
    Ok(())
}

fn todo_file_path() -> PathBuf {
    let data_home = env::var_os("XDG_DATA_HOME")
        .filter(|path| !path.is_empty())
        .map(PathBuf::from)
        .or_else(|| {
            env::var_os("HOME")
                .filter(|path| !path.is_empty())
                .map(|home| PathBuf::from(home).join(".local/share"))
        });

    data_home
        .unwrap_or_else(|| PathBuf::from("."))
        .join("tool-study-session")
        .join(TODO_FILE_NAME)
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
        state.window_fitted = false;
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
