//! Galion ui using ratatui

use ratatui::crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers, poll};
use ratatui::layout::{Alignment, Flex, Margin, Position, Rect};
use ratatui::style::{Modifier, Style, Stylize};
use ratatui::text::{Line, Span};
use ratatui::widgets::{
    Borders, Cell, Clear, HighlightSpacing, Row, Scrollbar, ScrollbarOrientation, ScrollbarState,
    Table, TableState, Wrap,
};
use ratatui::{
    DefaultTerminal, Frame,
    layout::{Constraint, Direction, Layout},
    style::Color,
    text::Text,
    widgets::{Block, Paragraph},
};
use serde_json::Value;
use std::collections::BTreeMap;
use std::fmt::Display;
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread::sleep;
use std::time::Duration;
use std::{io, thread};
use time::{OffsetDateTime, macros::format_description};

use crate::app::GalionConfig;
use crate::librclone::Rclone;
use crate::remote::{ConfigOrigin, EditRemote, RemoteConfiguration};
use crate::{GalionApp, GalionError};

/// [`SyncJob`] data
#[derive(Debug, Clone, PartialEq, PartialOrd, Ord, Eq)]
pub struct SyncJobData {
    /// sync job id
    job_id: u64,
    /// sync job name
    name: String,
    /// sync job src
    src: String,
    /// sync job dest
    dest: String,
}

/// rclone job type
pub type JobsList = BTreeMap<SyncJobData, JobState>;

/// Job statut
#[derive(Debug)]
pub enum ResultJob {
    /// Exit
    Exit,
    /// Sync
    Sync(JobsList),
}

/// Job statut
#[derive(Debug)]
pub enum SyncJob {
    /// Exit
    Exit,
    /// Sync
    Sync(SyncJobData),
}

/// Job status from rclone
#[derive(Debug, PartialEq, Clone, serde::Deserialize, serde::Serialize)]
pub struct JobStatus {
    /// success status
    success: bool,
    /// duration
    duration: f64,
    /// error
    error: String,
    /// start time
    #[serde(rename = "startTime")]
    start_time: String,

    /// Debug string
    debug_str: Option<String>,
}

impl Display for JobStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.error.is_empty() {
            write!(f, "success: {}, duration: {}", self.success, self.duration)
        } else {
            write!(
                f,
                "success: {} ({}), duration: {}",
                self.success, self.error, self.duration
            )
        }
    }
}

/// Job state
#[derive(Debug, PartialEq, Clone)]
pub enum JobState {
    /// Sent
    Sent,
    /// Waiting to finish
    Pending(JobStatus),
    /// Done
    Done(JobStatus),
}

impl JobState {
    /// Is this job waiting
    fn is_waiting(&self) -> bool {
        match self {
            Self::Sent | Self::Pending(_) => true,
            Self::Done(_) => false,
        }
    }

    /// Is this job an error
    fn success_color(&self) -> Color {
        match self {
            Self::Sent | Self::Pending(_) => Color::Blue,
            Self::Done(s) => {
                if s.success {
                    Color::Green
                } else {
                    Color::Red
                }
            }
        }
    }
}

impl Display for JobState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            JobState::Sent => write!(f, "sent"),
            JobState::Pending(job_status) => {
                write!(
                    f,
                    "waiting: start_time: {}",
                    job_status.start_time, // job_status.debug_str
                )
            }
            JobState::Done(job_status) => write!(f, "done: {job_status}"),
        }
    }
}

impl GalionApp {
    /// Background thread to use rclone
    fn background_thread(
        rclone: &Rclone,
        tx_to_ui: &Sender<ResultJob>,
        rx_to_ui: &Receiver<SyncJob>,
    ) -> Result<(), GalionError> {
        let thread_loop = || -> Result<(), GalionError> {
            let mut tracking_jobs = JobsList::new();
            loop {
                let is_jobs_waiting = tracking_jobs.values().any(JobState::is_waiting);
                let res_job = if is_jobs_waiting {
                    for (job_sync_data, job_state) in tracking_jobs.clone() {
                        if let JobState::Done(_) = job_state {
                            // skip done job
                        } else if let Ok(value_job_status) = rclone.job_status(job_sync_data.job_id)
                        {
                            // println!("{:?}", value_job_status);
                            let is_finished = value_job_status.get("finished").cloned();
                            let debug_str = value_job_status.to_string();
                            let mut job_status: JobStatus =
                                serde_json::from_value(value_job_status)?;
                            job_status.debug_str = Some(debug_str);
                            if let Some(Value::Bool(finished)) = is_finished
                                && finished
                            {
                                tracking_jobs.insert(job_sync_data, JobState::Done(job_status));
                            } else {
                                tracking_jobs.insert(job_sync_data, JobState::Pending(job_status));
                            }
                        }
                    }
                    match tx_to_ui.send(ResultJob::Sync(tracking_jobs.clone())) {
                        Ok(a) => a,
                        Err(_) => return Ok(()),
                    }
                    match rx_to_ui.try_recv() {
                        Ok(job) => job,
                        Err(mpsc::TryRecvError::Empty) => {
                            sleep(Duration::from_millis(500));
                            continue;
                        }
                        Err(mpsc::TryRecvError::Disconnected) => return Ok(()),
                    }
                } else {
                    match rx_to_ui.recv() {
                        Ok(job) => job,
                        Err(_) => {
                            return Ok(());
                        }
                    }
                };
                match res_job {
                    SyncJob::Exit => {
                        return Ok(());
                    }
                    SyncJob::Sync(sync_data_received) => {
                        let job =
                            rclone.sync(&sync_data_received.src, &sync_data_received.dest, true)?;
                        if let Some(Value::Number(jobid)) = job.get("jobid")
                            && let Some(job_id) = jobid.as_u64()
                        {
                            let mut sync_data = sync_data_received.clone();
                            sync_data.job_id = job_id;
                            tracking_jobs.insert(sync_data, JobState::Sent);
                        }
                    }
                }
            }
        };
        match thread_loop() {
            Ok(()) => Ok(()),
            Err(err) => {
                eprintln!("Background thread crashed: {err}");
                if let Err(e) = tx_to_ui.send(ResultJob::Exit) {
                    eprintln!("Failed to stop UI {e}");
                }
                Err(GalionError::new(format!(
                    "Background thread crashed: {err}"
                )))
            }
        }
    }

    /// Run the galion ui
    /// # Errors
    /// Errors when ui errors
    pub fn run_tui(&mut self) -> Result<(), GalionError> {
        // thread scope assert that the thread will not outlive the function
        thread::scope(|s| {
            let rclone = &self.rclone;
            let (tx_to_thread, rx_to_ui) = mpsc::channel();
            let (tx_to_ui, rx_from_thread) = mpsc::channel();
            let sync_handler: thread::ScopedJoinHandle<'_, Result<(), GalionError>> =
                s.spawn(move || Self::background_thread(rclone, &tx_to_ui, &rx_to_ui));

            let mut terminal = ratatui::init();
            let app_result = TuiApp::new(&mut self.config, rx_from_thread, tx_to_thread)
                .run(&mut terminal)
                .map_err(|e| GalionError::new(e.to_string()));
            ratatui::restore(); // Clean exit terminal
            let thread_result = sync_handler
                .join()
                .map_err(|_e| "Error joining the thread")?; // join error
            thread_result?; // thread error
            if !self.galion_args.hide_banner {
                println!("  ~Galion~");
            }
            app_result
        })
    }
}

/// Galion Tui mode
#[derive(Debug)]
enum TuiMode {
    /// Normal mode
    Normal,
    /// Error mode
    Error(String),
    /// Delete mode - confirmation
    Delete,
    /// Edit string mode
    EditString(EditRemote),
}

/// Galion Tui app
#[derive(Debug)]
pub struct TuiApp<'a> {
    /// app
    app_config: &'a mut GalionConfig,
    /// receiver of job
    pub rx_from_thread: Receiver<ResultJob>,
    /// sender of sync job
    pub tx_to_thread: Sender<SyncJob>,
    /// Map of jobs
    pub jobs: JobsList,
    /// should exit
    exit: bool,
    /// longest item length
    longest_item_lens: (u16, u16, u16),
    /// colors
    colors: Colors,
    /// state of the table
    state: TableState,
    /// state of the scrollbar
    scroll_state: ScrollbarState,
    /// Error display
    mode: TuiMode,
}

/// Item size
const ITEM_HEIGHT: usize = 1;

/// Tui Colors
#[derive(Debug)]
pub struct Colors {
    /// Normal color of the row
    pub normal_row_color: Color,
    /// Second color of the row
    pub alt_row_color: Color,
    /// row foreground
    pub row_fg: Color,
    /// selected column color
    pub selected_column_style_fg: Color,
    /// selected cell color
    pub selected_cell_style_fg: Color,
    /// buffer background
    pub buffer_bg: Color,
}

impl Default for Colors {
    fn default() -> Self {
        Colors {
            normal_row_color: Color::Gray,
            alt_row_color: Color::DarkGray,
            row_fg: Color::White,
            selected_column_style_fg: Color::Yellow,
            selected_cell_style_fg: Color::Cyan,
            buffer_bg: Color::Black,
        }
    }
}

/// Tiny helper
fn constraint_len_calculator(items: &[RemoteConfiguration]) -> (u16, u16, u16) {
    let mut longest_item_lens = (0, 0, 0);
    for item in items {
        let item_lens = item.to_table_row();
        longest_item_lens.0 = longest_item_lens
            .0
            .max(u16::try_from(item_lens[0].len()).unwrap_or(0));
        longest_item_lens.1 = longest_item_lens
            .1
            .max(u16::try_from(item_lens[1].len()).unwrap_or(0));
        longest_item_lens.2 = longest_item_lens
            .2
            .max(u16::try_from(item_lens[2].len()).unwrap_or(0));
    }
    longest_item_lens
}

impl<'a> TuiApp<'a> {
    /// UI poll time
    const REFRESH: Duration = Duration::from_millis(500);

    /// App name and version
    const APP: &'static str = concat!(env!("CARGO_PKG_NAME"), "@", env!("CARGO_PKG_VERSION"));

    /// Tui App
    pub fn new(
        app_config: &'a mut GalionConfig,
        rx_from_thread: Receiver<ResultJob>,
        tx_to_thread: Sender<SyncJob>,
    ) -> Self {
        let remotes = app_config.remotes();
        let longest_item_lens = constraint_len_calculator(remotes);
        let remotes_len = remotes.len();
        TuiApp {
            app_config,
            rx_from_thread,
            tx_to_thread,
            jobs: JobsList::default(),
            exit: false,
            longest_item_lens,
            colors: Colors::default(),
            state: TableState::default().with_selected(0),
            scroll_state: ScrollbarState::new(remotes_len * ITEM_HEIGHT),
            mode: TuiMode::Normal,
        }
    }

    /// runs the application's main loop until the user quits
    pub fn run(&mut self, terminal: &mut DefaultTerminal) -> io::Result<()> {
        while !self.exit {
            if let Ok(rx_from_thread) = self.rx_from_thread.try_recv() {
                match rx_from_thread {
                    ResultJob::Exit => self.exit = true,
                    ResultJob::Sync(jobs_list) => {
                        self.jobs = jobs_list;
                    }
                }
            }
            terminal.draw(|frame| self.draw(frame))?;
            self.handle_events()?;
        }
        Ok(())
    }

    /// Ratatui draw
    fn draw(&mut self, frame: &mut Frame<'_>) {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Min(1), Constraint::Length(1)])
            .split(frame.area());
        let sub_chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
            .split(chunks[0]);
        self.render_table(frame, sub_chunks[0]);
        self.render_scrollbar(frame, sub_chunks[0]);
        self.render_right_panel(frame, sub_chunks[1]);
        self.render_bottom_bar(frame, chunks[1]);
        self.render_popup(frame);
    }

    /// Render the popup error
    fn render_error_popup(&self, frame: &mut Frame<'_>) {
        let (title, content) = if let TuiMode::Error(error_msg) = &self.mode {
            ("Error", error_msg.as_ref())
        } else {
            ("Delete remote configuration", "Delete the config (y/n)")
        };
        let block = Block::bordered().title(title);
        let error_msg_widget = Paragraph::new(Line::from(content))
            .style(Style::default().bg(Color::Black).fg(Color::White))
            .block(block);
        let vertical = Layout::vertical([Constraint::Length(3)]).flex(Flex::Center);
        let horizontal = Layout::horizontal([Constraint::Percentage(40)]).flex(Flex::Center);
        let [area] = vertical.areas(frame.area());
        let [area] = horizontal.areas(area);
        frame.render_widget(Clear, area); //this clears out the background
        frame.render_widget(error_msg_widget, area);
    }

    /// Render the popup error
    fn render_popup(&self, frame: &mut Frame<'_>) {
        match &self.mode {
            TuiMode::Error(_) | TuiMode::Delete => {
                self.render_error_popup(frame);
            }
            TuiMode::EditString(edit_string) => {
                let area = frame
                    .area()
                    .centered(Constraint::Percentage(30), Constraint::Length(8));
                frame.render_widget(Clear, area); //this clears out the background
                let block = Block::bordered().title("Edit");
                let inner_block_area = block.inner(area);
                frame.render_widget(block, area);
                let [
                    area_title_name,
                    area_name,
                    area_title_src,
                    area_src,
                    area_title_dest,
                    area_dest,
                ] = Layout::default()
                    .direction(Direction::Vertical)
                    .constraints([
                        Constraint::Length(1),
                        Constraint::Length(1),
                        Constraint::Length(1),
                        Constraint::Length(1),
                        Constraint::Length(1),
                        Constraint::Length(1),
                    ])
                    .areas(inner_block_area);
                let title_name =
                    Paragraph::new("Remote name").style(match edit_string.idx_string {
                        0 => Style::default().fg(Color::Yellow),
                        _ => Style::default(),
                    });
                let input_name = Paragraph::new(edit_string.remote_name.as_str()).style(
                    match edit_string.idx_string {
                        0 => Style::default().fg(Color::Yellow),
                        _ => Style::default(),
                    },
                );
                frame.render_widget(title_name, area_title_name);
                frame.render_widget(input_name, area_name);
                if edit_string.idx_string == 0 {
                    frame.set_cursor_position(Position::new(
                        // Draw the cursor at the current position in the input field.
                        // This position is can be controlled via the left and right arrow key
                        area_name.x + u16::try_from(edit_string.character_index).unwrap_or(0),
                        area_name.y,
                    ));
                }
                let title_src =
                    Paragraph::new("Remote source").style(match edit_string.idx_string {
                        1 => Style::default().fg(Color::Yellow),
                        _ => Style::default(),
                    });
                let input_src = Paragraph::new(edit_string.remote_src.as_str()).style(
                    match edit_string.idx_string {
                        1 => Style::default().fg(Color::Yellow),
                        _ => Style::default(),
                    },
                );
                frame.render_widget(title_src, area_title_src);
                frame.render_widget(input_src, area_src);
                if edit_string.idx_string == 1 {
                    frame.set_cursor_position(Position::new(
                        // Draw the cursor at the current position in the input field.
                        // This position is can be controlled via the left and right arrow key
                        area_src.x + u16::try_from(edit_string.character_index).unwrap_or(0),
                        area_src.y,
                    ));
                }
                let title_dest =
                    Paragraph::new("Remote destination").style(match edit_string.idx_string {
                        2 => Style::default().fg(Color::Yellow),
                        _ => Style::default(),
                    });
                let input_dest = Paragraph::new(edit_string.remote_dest.as_str()).style(
                    match edit_string.idx_string {
                        2 => Style::default().fg(Color::Yellow),
                        _ => Style::default(),
                    },
                );
                frame.render_widget(title_dest, area_title_dest);
                frame.render_widget(input_dest, area_dest);
                if edit_string.idx_string == 2 {
                    frame.set_cursor_position(Position::new(
                        // Draw the cursor at the current position in the input field.
                        // This position is can be controlled via the left and right arrow key
                        area_dest.x + u16::try_from(edit_string.character_index).unwrap_or(0),
                        area_dest.y,
                    ));
                }
            }
            TuiMode::Normal => {}
        }
    }

    /// updates the application's state based on user input
    fn handle_events(&mut self) -> io::Result<()> {
        if poll(Self::REFRESH)? {
            match event::read()? {
                // it's important to check that the event is a key press event as
                // crossterm also emits key release and repeat events on Windows.
                Event::Key(key_event) if key_event.kind == KeyEventKind::Press => {
                    self.handle_key_event(key_event);
                }
                _ => {}
            }
        }
        Ok(())
    }

    /// Add a new error
    fn new_error<S: Into<String>>(&mut self, msg: S) {
        self.mode = TuiMode::Error(msg.into());
    }

    /// send a job
    fn send_job(&mut self) {
        let current_selected_job = if let Some(idx) = self.state.selected() {
            if let Some(remote) = self.app_config.remotes().get(idx) {
                remote
            } else {
                self.new_error(format!("No remote configuration at index {idx} in remotes"));
                return;
            }
        } else {
            self.new_error("No remote configuration selected");
            return;
        };
        let Some(remote_src) = &current_selected_job.remote_src else {
            self.new_error("Remote doesn't have a source - press e for edit");
            return;
        };
        let Some(remote_dest) = &current_selected_job.remote_dest else {
            self.new_error("Remote doesn't have a destination - press e for edit");
            return;
        };
        let sync_job = SyncJobData {
            name: current_selected_job.remote_name.clone(),
            src: remote_src.clone(),
            dest: remote_dest.clone(),
            job_id: 0, // fake job id
        };
        if let Err(_e) = self.tx_to_thread.send(SyncJob::Sync(sync_job)) {
            // ignore
        }
    }

    /// Ratatui handle key for normal mode
    fn handle_key_event_normal_mode(&mut self, key_event: KeyEvent) {
        match key_event.code {
            KeyCode::Char('q') | KeyCode::Esc => {
                self.exit();
            }
            KeyCode::Right => self.send_job(),
            KeyCode::Char('r') | KeyCode::Delete | KeyCode::Backspace => {
                if let Some(idx) = self.state.selected()
                    && let Some(config) = self.app_config.remotes().get(idx)
                {
                    if config.config_origin == ConfigOrigin::RcloneConfig {
                        self.new_error("Cannot delete a remote from the rclone config");
                    } else {
                        self.mode = TuiMode::Delete;
                    }
                } else {
                    self.new_error("Cannot delete the config");
                }
            }
            KeyCode::Char('d') => {
                if let Some(idx) = self.state.selected()
                    && let Some(config) = self.app_config.remotes().get(idx)
                {
                    if config.config_origin == ConfigOrigin::RcloneConfig {
                        self.new_error("Cannot duplicate a rclone config - try to edit it");
                    } else {
                        self.app_config
                            .remote_configurations
                            .insert(0, config.clone());
                    }
                } else {
                    self.new_error("Cannot duplicate the config");
                }
            }
            KeyCode::Char('j') | KeyCode::Down => {
                // Select new row
                let i = match self.state.selected() {
                    Some(i) => {
                        if i >= self.app_config.remotes().len() - 1 {
                            self.app_config.remotes().len() - 1
                        } else {
                            i + 1
                        }
                    }
                    None => 0,
                };
                self.state.select(Some(i));
                self.scroll_state = self.scroll_state.position(i * ITEM_HEIGHT);
            }
            KeyCode::Char('k') | KeyCode::Up => {
                // Select previous row
                let i = match self.state.selected() {
                    Some(i) => {
                        if i == 0 {
                            0
                        } else {
                            i - 1
                        }
                    }
                    None => 0,
                };
                self.state.select(Some(i));
                self.scroll_state = self.scroll_state.position(i * ITEM_HEIGHT);
            }
            KeyCode::Char('e') => {
                if let Some(idx) = self.state.selected()
                    && let Some(config) = self.app_config.remotes().get(idx)
                {
                    self.mode = TuiMode::EditString(EditRemote {
                        idx_string: 0,
                        character_index: 0,
                        remote_name: config.remote_name.clone(),
                        remote_src: config.remote_src.clone().unwrap_or_default(),
                        remote_dest: config.remote_dest.clone().unwrap_or_default(),
                    });
                } else {
                    self.new_error("Cannot edit");
                }
            }
            _ => {}
        }
    }

    /// Ratatui handle key
    fn handle_key_event(&mut self, key_event: KeyEvent) {
        // Handle CRTL + c
        match key_event.code {
            KeyCode::Char('c') if key_event.modifiers.contains(KeyModifiers::CONTROL) => {
                self.exit();
                return;
            }
            _ => {}
        }
        match &mut self.mode {
            TuiMode::Normal => self.handle_key_event_normal_mode(key_event),
            TuiMode::Error(_) => match key_event.code {
                KeyCode::Char('q') | KeyCode::Esc => {
                    self.mode = TuiMode::Normal;
                }
                _ => {}
            },
            TuiMode::Delete => match key_event.code {
                KeyCode::Char('q' | 'n') | KeyCode::Esc => {
                    self.mode = TuiMode::Normal;
                }
                KeyCode::Char('y') | KeyCode::Enter => {
                    if let Some(idx) = self.state.selected()
                        && let Some(config) = self.app_config.remotes().get(idx)
                    {
                        if config.config_origin == ConfigOrigin::RcloneConfig {
                            self.new_error("Cannot delete a remote from the rclone config");
                            return;
                        }
                        self.app_config.remote_configurations.remove(idx);
                        if let Err(e) = self.app_config.save_config() {
                            self.new_error(format!(
                                "Failed to save the config after remote deletion {e}"
                            ));
                        } else {
                            self.mode = TuiMode::Normal;
                        }
                    }
                }
                _ => {}
            },
            TuiMode::EditString(edit_string) => match key_event.code {
                KeyCode::Esc => {
                    self.mode = TuiMode::Normal;
                }
                KeyCode::Down | KeyCode::Tab => {
                    if edit_string.idx_string != 2 {
                        edit_string.idx_string += 1;
                        edit_string.reset_char_index();
                    }
                }
                KeyCode::Up => {
                    if edit_string.idx_string != 0 {
                        edit_string.idx_string -= 1;
                        edit_string.reset_char_index();
                    }
                }
                KeyCode::Enter => {
                    let new_remote = edit_string.finish();
                    if let Some(idx) = self.state.selected()
                        && let Some(config) = self.app_config.remote_configurations.get_mut(idx)
                    {
                        if config.config_origin == ConfigOrigin::GalionConfig {
                            *config = new_remote;
                        } else {
                            self.app_config.remote_configurations.insert(0, new_remote);
                        }
                        if let Err(e) = self.app_config.save_config() {
                            self.new_error(format!("Error save the config {e}"));
                        } else {
                            self.mode = TuiMode::Normal;
                        }
                    } else {
                        self.new_error("Cannot edit remote");
                    }
                }
                KeyCode::Left => edit_string.move_cursor_left(),
                KeyCode::Right => edit_string.move_cursor_right(),
                KeyCode::Char(to_insert) => edit_string.enter_char(to_insert),
                KeyCode::Backspace => edit_string.delete_char(),
                _ => {}
            },
        }
    }

    /// exit
    fn exit(&mut self) {
        self.exit = true;
        if let Err(_e) = self.tx_to_thread.send(SyncJob::Exit) {
            // background thread already exited?
            // eprintln!("{}", _e);
        }
    }

    /// Render bottom bar
    fn render_bottom_bar(&mut self, frame: &mut Frame<'_>, area: Rect) {
        let [left_area, right_area] = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Min(1), Constraint::Length(50)])
            .areas(area);

        let bg_color = if let TuiMode::Error(_) = &self.mode {
            Color::Red
        } else {
            Color::Black
        };
        let text_helper = match &self.mode {
            TuiMode::Error(_e) => vec!["(esc)".bold(), " close error".into()],
            TuiMode::Normal => {
                vec![
                    "(esc)".bold(),
                    " leave | ".into(),
                    "(arrow_up/arrow_down)".bold(),
                    " select | ".into(),
                    "(arrow_right)".bold(),
                    " launch job | ".into(),
                    "(r)".bold(),
                    " remove | ".into(),
                    "(e)".bold(),
                    " edit | ".into(),
                    "(d)".bold(),
                    " duplicate".into(),
                ]
            }
            TuiMode::EditString(_) => vec![
                "(esc)".bold(),
                " leave | ".into(),
                "(arrow_up/arrow_down)".bold(),
                " select | ".into(),
                "(enter)".bold(),
                " save".into(),
            ],
            TuiMode::Delete => vec![
                "(esc/n)".bold(),
                " cancel | ".into(),
                "(y)".bold(),
                " delete".into(),
            ],
        };
        let left_text = Line::from(text_helper);
        let now = OffsetDateTime::now_local().unwrap_or_else(|_| OffsetDateTime::now_utc());
        let format = format_description!("[year]-[month]-[day] [hour]:[minute]:[second]");
        let date_str = now
            .format(&format)
            .unwrap_or("Unable to format date".to_string());
        let right_text = Line::from(format!("{} - {}", Self::APP, date_str));
        let left_widget =
            Paragraph::new(left_text).style(Style::default().bg(bg_color).fg(Color::White));
        let right_widget = Paragraph::new(right_text)
            .alignment(Alignment::Right)
            .style(Style::default().bg(bg_color).fg(Color::White));
        frame.render_widget(left_widget, left_area);
        frame.render_widget(right_widget, right_area);
    }

    /// Render right panel
    fn render_right_panel(&mut self, frame: &mut Frame<'_>, area: Rect) {
        let job_block = Block::default()
            .borders(Borders::ALL)
            .style(Style::default());
        let job_text: Vec<Line<'_>> = if self.jobs.is_empty() {
            let str_to_show = match self.mode {
                TuiMode::Normal => GalionApp::logo_random_waves(),
                _ => GalionApp::logo_waves(),
            };
            str_to_show
                .lines()
                .map(|s| Line::from(String::from(s)))
                .chain(std::iter::once(Line::from("Nothing to do, just sailing")))
                .collect()
        } else {
            let mut str_to_show = Vec::new();
            // Show latest jobs first
            for (one_job_data, state) in self.jobs.iter().rev() {
                let job_string = format!(
                    "job {} ({}): {}\n",
                    one_job_data.name, one_job_data.job_id, state
                );
                str_to_show.push(Line::from(Span::styled(
                    job_string,
                    Style::default().fg(state.success_color()),
                )));
            }
            str_to_show
        };
        let job_paragraph = Paragraph::new(Text::from(job_text))
            .wrap(Wrap { trim: false })
            .block(job_block);
        frame.render_widget(job_paragraph, area);
    }

    /// Ratatui render table
    fn render_table(&mut self, frame: &mut Frame<'_>, area: Rect) {
        let header_style = Style::default();
        let bg_color_selected = if let TuiMode::Error(_err_str) = &self.mode {
            Color::Red
        } else {
            Color::Blue
        };
        let selected_row_style = Style::default()
            .add_modifier(Modifier::REVERSED)
            .fg(bg_color_selected);
        let selected_col_style = Style::default().fg(self.colors.selected_column_style_fg);
        let selected_cell_style = Style::default()
            .add_modifier(Modifier::REVERSED)
            .fg(self.colors.selected_cell_style_fg);

        let header = ["name/origin", "src", "dest"]
            .into_iter()
            .map(Cell::from)
            .collect::<Row<'_>>()
            .style(header_style)
            .height(1);
        let rows = self
            .app_config
            .remotes()
            .iter()
            .enumerate()
            .map(|(i, data)| {
                let _color = match i % 2 {
                    0 => self.colors.normal_row_color,
                    _ => self.colors.alt_row_color,
                };
                let item = data.to_table_row();
                item.into_iter()
                    .map(|content| Cell::from(Text::from(format!("\n{content}\n"))))
                    .collect::<Row<'_>>()
                    .style(Style::new().fg(self.colors.row_fg).bg(self.colors.row_fg))
                    .height(4)
            });
        let bar = " █ ";
        let t = Table::new(
            rows,
            [
                // + 1 is for padding.
                Constraint::Length(self.longest_item_lens.0 + 1),
                Constraint::Min(self.longest_item_lens.1 + 1),
                Constraint::Min(self.longest_item_lens.2),
            ],
        )
        .header(header)
        .row_highlight_style(selected_row_style)
        .column_highlight_style(selected_col_style)
        .cell_highlight_style(selected_cell_style)
        .highlight_symbol(Text::from(vec![
            "".into(),
            bar.into(),
            bar.into(),
            "".into(),
        ]))
        .highlight_spacing(HighlightSpacing::Always);
        frame.render_stateful_widget(t, area, &mut self.state);
    }

    /// Ratatui render scrollbar
    fn render_scrollbar(&mut self, frame: &mut Frame<'_>, area: Rect) {
        frame.render_stateful_widget(
            Scrollbar::default()
                .orientation(ScrollbarOrientation::VerticalRight)
                .begin_symbol(None)
                .end_symbol(None)
                .style(
                    Style::default()
                        .fg(self.colors.buffer_bg)
                        .bg(self.colors.buffer_bg),
                )
                .track_style(Style::default().fg(self.colors.buffer_bg).bg(Color::White)),
            area.inner(Margin {
                vertical: 1,
                horizontal: 1,
            }),
            &mut self.scroll_state,
        );
    }
}
