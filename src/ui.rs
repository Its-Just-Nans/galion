//! Galion ui using ratatui

use crossterm::event::KeyModifiers;
use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind};
use ratatui::crossterm::event::poll;
use ratatui::layout::{Margin, Rect};
use ratatui::style::{Modifier, Style};
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
use tokio::runtime::Runtime;
use tokio::sync::mpsc::UnboundedSender;
use tokio::sync::mpsc::unbounded_channel;
use tokio::time;

use crate::app::RemoteConfiguration;
use crate::rclone::Rclone;
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
        let mut terminal = ratatui::init();
        let (tx_sync, mut rx_sync) = unbounded_channel::<SyncJob>();
        let (tx_job, rx_jobs) = channel::<JobsInfo>();
        let rt = Runtime::new()?;

        let sync_handler = rt.spawn(async move {
            let job_checker = tokio::task::spawn(async move {
                let mut interval = time::interval(Duration::from_millis(1000));

                loop {
                    interval.tick().await;
                    let job_list = match Rclone::job_list() {
                        Ok(list) => list,
                        Err(_) => continue,
                    };
                    let mut hash_map: JobsInfo = HashMap::new();
                    for job_id in job_list.job_ids {
                        if let Ok(res) = Rclone::job_status(job_id)
                            && let Some(Value::Bool(finished)) = res.get("finished")
                            && !finished
                        {
                            hash_map.insert(job_id, res.to_string());
                        }
                    }
                    if let Err(_e) = tx_job.send(hash_map) {
                        break;
                    }
                }
            });
            while let Some(_i) = rx_sync.recv().await {
                match _i {
                    SyncJob::Exit => break,
                    SyncJob::Sync(job_id) => {
                        tokio::task::spawn(async move { Rclone::rc_noop(json!({"_async": true})) });
                    }
                }
            }
            job_checker.abort();
        });

        let app_result = TuiApp::new(self, rx_jobs, tx_sync).run(&mut terminal);
        sync_handler.abort();
        ratatui::restore();
        app_result.map_err(|e| GalionError::new(e.to_string()))
    }
}

/// Galion Tui app
#[derive(Debug)]
pub struct TuiApp<'a> {
    /// app
    app: &'a GalionApp,
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

impl<'a> TuiApp<'a> {
    /// Tui App
    pub fn new(
        app: &'a GalionApp,
        rx_jobs: Receiver<JobsInfo>,
        tx_sync: UnboundedSender<SyncJob>,
    ) -> Self {
        let remotes = app.remotes();
        TuiApp {
            app,
            rx_jobs,
            tx_sync,
            jobs: Default::default(),
            exit: false,
            longest_item_lens: constraint_len_calculator(&remotes),
            colors: Colors::default(),
            state: TableState::default().with_selected(0),
            scroll_state: ScrollbarState::new(remotes.len() * ITEM_HEIGHT),
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
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
            .split(frame.area());

        self.render_table(frame, chunks[0]);
        self.render_scrollbar(frame, chunks[0]);

        let job_block = Block::default()
            .borders(Borders::ALL)
            .style(Style::default());
        let job = Paragraph::new(Text::styled(
            format!("jobs: {:?}", self.jobs),
            Style::default().fg(Color::Green),
        ))
        .wrap(Wrap { trim: false })
        .block(job_block);
        frame.render_widget(job, chunks[1]);
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
                if i >= self.app.remotes().len() - 1 {
                    self.app.remotes().len() - 1
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
            // ignore
        }
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
        let remotes = self.app.remotes();
        let rows = remotes.iter().enumerate().map(|(i, data)| {
            let color = match i % 2 {
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
