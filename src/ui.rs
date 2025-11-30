//! Galion ui using ratatui

use ratatui::crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers, poll};
use ratatui::layout::{Alignment, Margin, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::Line;
use ratatui::widgets::{
    Borders, Cell, HighlightSpacing, Row, Scrollbar, ScrollbarOrientation, ScrollbarState, Table,
    TableState, Wrap,
};

use ratatui::{
    DefaultTerminal, Frame,
    layout::{Constraint, Direction, Layout},
    style::Color,
    text::Text,
    widgets::{Block, Paragraph},
};
use serde_json::{Value, json};
use std::collections::HashMap;
use std::io;
use std::sync::mpsc::{Receiver, channel};
use std::time::Duration;
use time::{OffsetDateTime, macros::format_description};
use tokio::runtime::Runtime;
use tokio::sync::mpsc::UnboundedSender;
use tokio::sync::mpsc::unbounded_channel;
use tokio::task::JoinHandle;
use tokio::time::interval;

use crate::remote::RemoteConfiguration;
use crate::{GalionApp, GalionError};

/// rclone job type
type JobsInfo = HashMap<u64, String>;

/// Job statut
pub enum SyncJob {
    /// Exit
    Exit,
    /// Sync
    Sync(u64),
}

impl GalionApp {
    /// Run the galion ui
    /// # Errors
    /// Errors when ui errors
    pub fn run_tui(&self) -> Result<(), GalionError> {
        let (tx_sync, mut rx_sync) = unbounded_channel::<SyncJob>();
        let (tx_job, rx_jobs) = channel::<JobsInfo>();
        let rt = Runtime::new()?;
        let remotes = self.remotes();
        let rclone_arc = self.rclone.clone();
        let sync_handler: JoinHandle<Result<(), GalionError>> = rt.spawn(async move {
            let mut interval = interval(Duration::from_millis(750));
            loop {
                interval.tick().await;
                let rclone = rclone_arc
                    .lock()
                    .map_err(|e| GalionError::new(format!("Mutex poisoned: {e}")))?;
                let job_list = match rclone.job_list() {
                    Ok(list) => list,
                    Err(_) => continue,
                };
                let mut hash_map: JobsInfo = HashMap::new();
                for job_id in job_list.job_ids {
                    if let Ok(res) = rclone.job_status(job_id)
                        && let Some(Value::Bool(finished)) = res.get("finished")
                        && !finished
                    {
                        hash_map.insert(job_id, res.to_string());
                    }
                }
                tx_job
                    .send(hash_map)
                    .map_err(|e| GalionError::new(format!("Error send job status: {e}")))?;
                if let Ok(_i) = rx_sync.try_recv() {
                    match _i {
                        SyncJob::Exit => {
                            return Ok(());
                        }
                        SyncJob::Sync(_job_id) => {
                            let rclone_cloned = rclone_arc.clone();
                            tokio::task::spawn(async move {
                                let rclone = rclone_cloned.lock().map_err(|e| {
                                    GalionError::new(format!("Mutex poisoned: {e}"))
                                })?;
                                rclone.rc_noop(json!({"_async": true}))
                            });
                        }
                    }
                }
            }
        });

        let mut terminal = ratatui::init();
        let app_result = TuiApp::new(remotes, rx_jobs, tx_sync)
            .run(&mut terminal)
            .map_err(|e| GalionError::new(e.to_string()));
        sync_handler.abort();
        ratatui::restore();
        println!("  ~Galion~"); // Clean exit terminal
        app_result
    }
}

/// Galion Tui app
#[derive(Debug)]
pub struct TuiApp {
    /// app
    remotes: Vec<RemoteConfiguration>,
    /// receiver of job
    pub rx_jobs: Receiver<JobsInfo>,
    /// sender of sync job
    pub tx_sync: UnboundedSender<SyncJob>,
    /// Map of jobs
    pub jobs: JobsInfo,
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
    /// Debug frames
    debug_frame: Option<i64>,
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
    /// selectect row color
    selected_row_style_fg: Color,
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
            selected_row_style_fg: Color::Blue,
            buffer_bg: Color::Black,
        }
    }
}

/// Tiny helper
fn constraint_len_calculator(items: &[RemoteConfiguration]) -> (u16, u16, u16) {
    let mut longest_item_lens = (0, 0, 0);
    for item in items {
        let item_lens = item.to_table_row();
        longest_item_lens.0 = longest_item_lens.0.max(item_lens[0].len() as u16);
        longest_item_lens.1 = longest_item_lens.1.max(item_lens[1].len() as u16);
        longest_item_lens.2 = longest_item_lens.2.max(item_lens[2].len() as u16);
    }
    longest_item_lens
}

impl TuiApp {
    /// Tui App
    pub fn new(
        remotes: Vec<RemoteConfiguration>,
        rx_jobs: Receiver<JobsInfo>,
        tx_sync: UnboundedSender<SyncJob>,
    ) -> Self {
        let longest_item_lens = constraint_len_calculator(&remotes);
        let remotes_len = remotes.len();
        TuiApp {
            remotes,
            rx_jobs,
            tx_sync,
            jobs: Default::default(),
            exit: false,
            longest_item_lens,
            colors: Colors::default(),
            state: TableState::default().with_selected(0),
            scroll_state: ScrollbarState::new(remotes_len * ITEM_HEIGHT),
            debug_frame: None, // Some(0),
        }
    }

    /// runs the application's main loop until the user quits
    pub fn run(&mut self, terminal: &mut DefaultTerminal) -> io::Result<()> {
        while !self.exit {
            if let Ok(jobs_list) = self.rx_jobs.try_recv() {
                self.jobs = jobs_list;
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
        self.render_helper(frame, chunks[1]);
    }

    /// updates the application's state based on user input
    fn handle_events(&mut self) -> io::Result<()> {
        if poll(Duration::from_millis(1000))? {
            match event::read()? {
                // it's important to check that the event is a key press event as
                // crossterm also emits key release and repeat events on Windows.
                Event::Key(key_event) if key_event.kind == KeyEventKind::Press => {
                    self.handle_key_event(key_event)
                }
                _ => {}
            };
        }
        Ok(())
    }

    /// send a job
    fn send_job(&mut self) {
        if let Err(_e) = self.tx_sync.send(SyncJob::Sync(0)) {
            // ignore
        }
    }

    /// Ratatui handle key
    fn handle_key_event(&mut self, key_event: KeyEvent) {
        match key_event.code {
            KeyCode::Char('q') => self.exit(),
            KeyCode::Char('c') if key_event.modifiers.contains(KeyModifiers::CONTROL) => {
                self.exit()
            }
            KeyCode::Right => self.send_job(),
            KeyCode::Char('j') | KeyCode::Down => self.next_row(),
            KeyCode::Char('k') | KeyCode::Up => self.previous_row(),
            _ => {}
        }
    }

    /// Select new row
    pub fn next_row(&mut self) {
        let i = match self.state.selected() {
            Some(i) => {
                if i >= self.remotes.len() - 1 {
                    self.remotes.len() - 1
                } else {
                    i + 1
                }
            }
            None => 0,
        };
        self.state.select(Some(i));
        self.scroll_state = self.scroll_state.position(i * ITEM_HEIGHT);
    }

    /// Select previous row
    pub fn previous_row(&mut self) {
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

    /// exit
    fn exit(&mut self) {
        self.exit = true;
        if let Err(_e) = self.tx_sync.send(SyncJob::Exit) {
            // println!("{}", _e); //TODO
        }
    }

    /// Render helper
    fn render_helper(&mut self, frame: &mut Frame<'_>, area: Rect) {
        let [left_area, right_area] = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Min(1), Constraint::Length(20)])
            .areas(area);
        let left_text = Line::from("qsdfqs");
        let now = OffsetDateTime::now_local().unwrap_or_else(|_| OffsetDateTime::now_utc());
        let format = format_description!("[year]-[month]-[day] [hour]:[minute]:[second]");
        let date_str = now.format(&format).unwrap();
        let right_text = Line::from(date_str);
        let left_widget =
            Paragraph::new(left_text).style(Style::default().bg(Color::Black).fg(Color::White));
        let right_widget = Paragraph::new(right_text)
            .alignment(Alignment::Right)
            .style(Style::default().bg(Color::Black).fg(Color::White));
        frame.render_widget(left_widget, left_area);
        frame.render_widget(right_widget, right_area);
    }

    /// Render right panel
    fn render_right_panel(&mut self, frame: &mut Frame<'_>, area: Rect) {
        let job_block = Block::default()
            .borders(Borders::ALL)
            .style(Style::default());
        let job_text = if self.jobs.is_empty() {
            let mut str_to_show = format!(
                "{}\nNothing to do, just sailing",
                GalionApp::logo_random_waves()
            );
            if let Some(debug_frame) = &mut self.debug_frame {
                *debug_frame += 1;
                str_to_show.push_str(&format!("\n{:?}", debug_frame));
            }
            str_to_show
        } else {
            format!("jobs: {:?}", self.jobs)
        };
        let job_paragraph =
            Paragraph::new(Text::styled(job_text, Style::default().fg(Color::Green)))
                .wrap(Wrap { trim: false })
                .block(job_block);
        frame.render_widget(job_paragraph, area);
    }

    /// Ratatui render table
    fn render_table(&mut self, frame: &mut Frame<'_>, area: Rect) {
        let header_style = Style::default();
        let selected_row_style = Style::default()
            .add_modifier(Modifier::REVERSED)
            .fg(self.colors.selected_row_style_fg);
        let selected_col_style = Style::default().fg(self.colors.selected_column_style_fg);
        let selected_cell_style = Style::default()
            .add_modifier(Modifier::REVERSED)
            .fg(self.colors.selected_cell_style_fg);

        let header = ["name", "src", "dest"]
            .into_iter()
            .map(Cell::from)
            .collect::<Row<'_>>()
            .style(header_style)
            .height(1);
        let rows = self.remotes.iter().enumerate().map(|(i, data)| {
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
