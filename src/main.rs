use std::{
    env, fs,
    io::{self, Stdout},
    time::{Duration, Instant},
};

use crossterm::{
    event::{
        self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEventKind,
        KeyModifiers, MouseButton, MouseEventKind,
    },
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    backend::CrosstermBackend,
    buffer::Buffer,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    widgets::{Block, Borders},
    Frame, Terminal,
};

#[derive(Clone, Copy, PartialEq)]
enum Confirm {
    Save,
    Delete,
    Quit,
}

struct App {
    path: String,
    rows: Vec<Vec<String>>,
    widths: Vec<u16>,
    sel_row: usize,
    sel_col: usize,
    scroll_row: usize,
    col_page: usize,
    last_width: u16,
    editing: bool,
    edit_buf: String,
    dirty: bool,
    confirm: Option<Confirm>,
    msg: String,
    last_click: Option<(Instant, usize, usize)>,
    cell_rects: Vec<(Rect, usize, usize)>,
}

impl App {
    fn load(path: &str) -> io::Result<Self> {
        let mut rows: Vec<Vec<String>> = Vec::new();
        if fs::metadata(path).is_ok() {
            let mut rdr = csv::ReaderBuilder::new()
                .has_headers(false)
                .flexible(true)
                .from_path(path)
                .map_err(|e| io::Error::new(io::ErrorKind::Other, e))?;
            for rec in rdr.records() {
                let rec = rec.map_err(|e| io::Error::new(io::ErrorKind::Other, e))?;
                rows.push(rec.iter().map(|s| s.to_string()).collect());
            }
        }
        if rows.is_empty() {
            rows.push(vec![String::new()]);
        }
        let mut app = App {
            path: path.to_string(),
            rows,
            widths: Vec::new(),
            sel_row: 0,
            sel_col: 0,
            scroll_row: 0,
            col_page: 0,
            last_width: 80,
            editing: false,
            edit_buf: String::new(),
            dirty: false,
            confirm: None,
            msg: String::new(),
            last_click: None,
            cell_rects: Vec::new(),
        };
        app.ensure_trailing_empty_row_and_col();
        Ok(app)
    }

    fn recalc_widths(&mut self) {
        let ncols = self.ncols();
        let mut widths = vec![3u16; ncols];
        for r in &self.rows {
            for (i, c) in r.iter().enumerate() {
                let w = (c.chars().count() as u16 + 2).max(3);
                if w > widths[i] {
                    widths[i] = w;
                }
            }
        }
        self.widths = widths;
    }

    fn ensure_trailing_empty_row_and_col(&mut self) {
        let max_cols = self.rows.iter().map(|r| r.len()).max().unwrap_or(1);
        for r in self.rows.iter_mut() {
            while r.len() < max_cols {
                r.push(String::new());
            }
        }

        let last_col_has_data = self
            .rows
            .iter()
            .any(|r| !r.last().map_or(true, |s| s.is_empty()));
        if last_col_has_data {
            for r in self.rows.iter_mut() {
                r.push(String::new());
            }
        }

        let ncols = self.ncols();
        let last_row_has_data = self
            .rows
            .last()
            .map_or(true, |r| r.iter().any(|s| !s.is_empty()));
        if last_row_has_data {
            self.rows.push(vec![String::new(); ncols]);
        }

        self.recalc_widths();
    }

    fn ncols(&self) -> usize {
        self.rows.first().map(|r| r.len()).unwrap_or(1)
    }

    fn col_pages(&self, max_width: u16) -> Vec<std::ops::Range<usize>> {
        let ncols = self.ncols();
        if ncols == 0 || max_width == 0 {
            return vec![0..1];
        }
        let mut pages = Vec::new();
        let mut start = 0;
        while start < ncols {
            let mut end = start;
            let mut current_w = 0u16;
            while end < ncols {
                let mut col_w = self.widths.get(end).copied().unwrap_or(10);
                if self.editing && end == self.sel_col {
                    let edit_w = (self.edit_buf.chars().count() as u16 + 3).max(3);
                    if edit_w > col_w {
                        col_w = edit_w;
                    }
                }
                let sep_w = if end > start { 1 } else { 0 };
                if current_w + sep_w + col_w > max_width && end > start {
                    break;
                }
                current_w += sep_w + col_w;
                end += 1;
            }
            if end == start {
                end = start + 1;
            }
            pages.push(start..end);
            start = end;
        }
        if pages.is_empty() {
            pages.push(0..ncols);
        }
        pages
    }

    fn prev_page(&mut self) {
        let pages = self.col_pages(self.last_width);
        if pages.is_empty() {
            return;
        }
        let current_p = pages
            .iter()
            .position(|r| r.contains(&self.sel_col))
            .unwrap_or(0);
        if current_p > 0 {
            let new_p = current_p - 1;
            self.sel_col = pages[new_p].start;
            self.col_page = new_p;
        }
    }

    fn next_page(&mut self) {
        let pages = self.col_pages(self.last_width);
        if pages.is_empty() {
            return;
        }
        let current_p = pages
            .iter()
            .position(|r| r.contains(&self.sel_col))
            .unwrap_or(0);
        if current_p + 1 < pages.len() {
            let new_p = current_p + 1;
            self.sel_col = pages[new_p].start;
            self.col_page = new_p;
        }
    }

    fn save(&mut self) -> io::Result<()> {
        let mut last_non_empty_row = 0;
        let mut last_non_empty_col = 0;
        let mut has_data = false;
        for (r, row) in self.rows.iter().enumerate() {
            for (c, val) in row.iter().enumerate() {
                if !val.is_empty() {
                    has_data = true;
                    if r > last_non_empty_row {
                        last_non_empty_row = r;
                    }
                    if c > last_non_empty_col {
                        last_non_empty_col = c;
                    }
                }
            }
        }
        let mut wtr = csv::WriterBuilder::new()
            .from_path(&self.path)
            .map_err(|e| io::Error::new(io::ErrorKind::Other, e))?;
        if has_data {
            for r in 0..=last_non_empty_row {
                if r < self.rows.len() {
                    let slice = &self.rows[r]
                        [..=last_non_empty_col.min(self.rows[r].len().saturating_sub(1))];
                    wtr.write_record(slice)
                        .map_err(|e| io::Error::new(io::ErrorKind::Other, e))?;
                }
            }
        } else {
            wtr.write_record(&[""])
                .map_err(|e| io::Error::new(io::ErrorKind::Other, e))?;
        }
        wtr.flush()?;
        self.dirty = false;
        self.msg = format!("Saved to {}", self.path);
        Ok(())
    }

    fn delete_cell(&mut self) {
        let col = self.sel_col;
        for r in self.sel_row..self.rows.len().saturating_sub(1) {
            let next = self.rows[r + 1][col].clone();
            self.rows[r][col] = next;
        }
        if let Some(last) = self.rows.last_mut() {
            last[col] = String::new();
        }
        self.dirty = true;
        self.ensure_trailing_empty_row_and_col();
        self.msg = "Cell deleted, column shifted up".into();
    }

    fn move_sel(&mut self, dr: isize, dc: isize) {
        let nr = self.rows.len() as isize;
        let nc = self.ncols() as isize;
        let mut r = self.sel_row as isize + dr;
        let mut c = self.sel_col as isize + dc;
        r = r.clamp(0, nr - 1);
        c = c.clamp(0, nc - 1);
        self.sel_row = r as usize;
        self.sel_col = c as usize;
    }
}

fn ui(f: &mut Frame, app: &mut App) {
    let area = f.size();
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(3), Constraint::Length(1)])
        .split(area);
    let table_area = chunks[0];
    let status_area = chunks[1];

    let block = Block::default().borders(Borders::ALL);
    let inner = block.inner(table_area);

    let pages = app.col_pages(inner.width);
    app.last_width = inner.width;
    if let Some(p_idx) = pages.iter().position(|r| r.contains(&app.sel_col)) {
        app.col_page = p_idx;
    } else if app.col_page >= pages.len() {
        app.col_page = pages.len().saturating_sub(1);
    }

    let page_str = if pages.len() > 1 {
        format!(" [Page {}/{}]", app.col_page + 1, pages.len())
    } else {
        String::new()
    };

    let title = format!(
        " rcsv: {}{} {}",
        app.path,
        page_str,
        if app.dirty { "[modified]" } else { "" }
    );
    let block = block.title(title);
    f.render_widget(block, table_area);

    app.cell_rects.clear();

    if app.sel_row < app.scroll_row {
        app.scroll_row = app.sel_row;
    }
    let visible_rows = inner.height as usize;
    if visible_rows > 0 && app.sel_row >= app.scroll_row + visible_rows {
        app.scroll_row = app.sel_row + 1 - visible_rows;
    }

    let current_page = pages.get(app.col_page).cloned().unwrap_or(0..app.ncols());
    let buf: &mut Buffer = f.buffer_mut();

    for (ridx, row) in app.rows.iter().enumerate().skip(app.scroll_row) {
        let y = inner.y + (ridx - app.scroll_row) as u16;
        if y >= inner.y + inner.height {
            break;
        }
        let mut x = inner.x;
        for cidx in current_page.clone() {
            if cidx >= row.len() {
                break;
            }
            let mut width = app.widths.get(cidx).copied().unwrap_or(10);
            if app.editing && cidx == app.sel_col {
                let edit_w = (app.edit_buf.chars().count() as u16 + 3).max(3);
                if edit_w > width {
                    width = edit_w;
                }
            }
            if x >= inner.x + inner.width {
                break;
            }
            let avail = (inner.x + inner.width).saturating_sub(x);
            let w = width.min(avail);
            let is_sel = ridx == app.sel_row && cidx == app.sel_col;
            let content = if is_sel && app.editing {
                format!("{}_", app.edit_buf)
            } else {
                row[cidx].clone()
            };
            let mut text = content;
            if text.chars().count() as u16 > w.saturating_sub(1) && w < width {
                let take = w.saturating_sub(2).max(1) as usize;
                text = text.chars().take(take).collect::<String>() + "~";
            }
            let padded = format!("{:<width$}", text, width = w as usize);
            let mut style = Style::default();
            if ridx == 0 {
                style = style.add_modifier(Modifier::BOLD);
            }
            if is_sel {
                style = if app.editing {
                    Style::default().bg(Color::Yellow).fg(Color::Black)
                } else {
                    Style::default().bg(Color::Cyan).fg(Color::Black)
                };
            }
            buf.set_string(x, y, &padded, style);
            app.cell_rects.push((
                Rect {
                    x,
                    y,
                    width: w,
                    height: 1,
                },
                ridx,
                cidx,
            ));
            x += w;
            if x < inner.x + inner.width && cidx + 1 < current_page.end {
                buf.set_string(x, y, "|", Style::default().fg(Color::DarkGray));
                x += 1;
            }
        }
    }

    let status = if let Some(c) = app.confirm {
        match c {
            Confirm::Save => "Save changes? (y/n)".to_string(),
            Confirm::Delete => "Delete focused cell? (y/n)".to_string(),
            Confirm::Quit => "Save changes before exit? (y/n)".to_string(),
        }
    } else if app.editing {
        "Editing: Enter=confirm  Esc=cancel".to_string()
    } else if !app.msg.is_empty() {
        app.msg.clone()
    } else {
        "Arrows=move Alt+Left/Right=page Enter/DblClick=edit Ctrl+S=save Ctrl+D=delete Ctrl+Q=quit"
            .to_string()
    };
    buf.set_string(status_area.x, status_area.y, &status, Style::default());
}

fn hit_test(app: &App, x: u16, y: u16) -> Option<(usize, usize)> {
    for (rect, r, c) in &app.cell_rects {
        if x >= rect.x && x < rect.x + rect.width && y == rect.y {
            return Some((*r, *c));
        }
    }
    None
}

fn main() -> io::Result<()> {
    let args: Vec<String> = env::args().collect();
    if args.len() < 2 {
        eprintln!("usage: rcsv <file.csv>");
        std::process::exit(1);
    }
    let mut app = App::load(&args[1])?;

    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let res = run(&mut terminal, &mut app);

    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;

    res
}

fn run(terminal: &mut Terminal<CrosstermBackend<Stdout>>, app: &mut App) -> io::Result<()> {
    loop {
        terminal.draw(|f| ui(f, app))?;

        if !event::poll(Duration::from_millis(200))? {
            continue;
        }
        match event::read()? {
            Event::Key(key) => {
                if key.kind != KeyEventKind::Press {
                    continue;
                }
                if let Some(c) = app.confirm {
                    match key.code {
                        KeyCode::Char('y') | KeyCode::Char('Y') => {
                            match c {
                                Confirm::Save => {
                                    let _ = app.save();
                                }
                                Confirm::Delete => app.delete_cell(),
                                Confirm::Quit => {
                                    let _ = app.save();
                                    break;
                                }
                            }
                            app.confirm = None;
                        }
                        KeyCode::Char('n') | KeyCode::Char('N') => {
                            if c == Confirm::Quit {
                                break;
                            }
                            app.confirm = None;
                            app.msg = "Cancelled".into();
                        }
                        KeyCode::Esc => {
                            app.confirm = None;
                            app.msg = "Cancelled".into();
                        }
                        _ => {}
                    }
                    continue;
                }

                if app.editing {
                    match key.code {
                        KeyCode::Enter => {
                            app.rows[app.sel_row][app.sel_col] = app.edit_buf.clone();
                            app.editing = false;
                            app.dirty = true;
                            app.ensure_trailing_empty_row_and_col();
                        }
                        KeyCode::Esc => {
                            app.editing = false;
                        }
                        KeyCode::Backspace => {
                            app.edit_buf.pop();
                            app.rows[app.sel_row][app.sel_col] = app.edit_buf.clone();
                            app.dirty = true;
                            app.ensure_trailing_empty_row_and_col();
                        }
                        KeyCode::Char(c) => {
                            app.edit_buf.push(c);
                            app.rows[app.sel_row][app.sel_col] = app.edit_buf.clone();
                            app.dirty = true;
                            app.ensure_trailing_empty_row_and_col();
                        }
                        _ => {}
                    }
                    continue;
                }

                match (key.code, key.modifiers) {
                    (KeyCode::Char('s'), KeyModifiers::CONTROL) => {
                        app.confirm = Some(Confirm::Save);
                    }
                    (KeyCode::Char('d'), KeyModifiers::CONTROL) => {
                        app.confirm = Some(Confirm::Delete);
                    }
                    (KeyCode::Char('q') | KeyCode::Char('Q'), KeyModifiers::CONTROL)
                    | (KeyCode::Char('q'), KeyModifiers::NONE) => {
                        if app.dirty {
                            app.confirm = Some(Confirm::Quit);
                        } else {
                            break;
                        }
                    }
                    (KeyCode::Left, m) if m.contains(KeyModifiers::ALT) => {
                        app.prev_page();
                    }
                    (KeyCode::Right, m) if m.contains(KeyModifiers::ALT) => {
                        app.next_page();
                    }
                    (KeyCode::Up, _) => app.move_sel(-1, 0),
                    (KeyCode::Down, _) => app.move_sel(1, 0),
                    (KeyCode::Left, _) => app.move_sel(0, -1),
                    (KeyCode::Right, _) => app.move_sel(0, 1),
                    (KeyCode::Enter, _) => {
                        app.editing = true;
                        app.edit_buf = app.rows[app.sel_row][app.sel_col].clone();
                    }
                    _ => {}
                }
            }
            Event::Mouse(m) => {
                if let MouseEventKind::Down(MouseButton::Left) = m.kind {
                    if let Some((r, c)) = hit_test(app, m.column, m.row) {
                        let now = Instant::now();
                        let is_double = matches!(
                            app.last_click,
                            Some((t, lr, lc)) if lr == r && lc == c && now.duration_since(t) < Duration::from_millis(400)
                        );
                        app.sel_row = r;
                        app.sel_col = c;
                        if is_double && app.confirm.is_none() {
                            app.editing = true;
                            app.edit_buf = app.rows[r][c].clone();
                            app.last_click = None;
                        } else {
                            app.last_click = Some((now, r, c));
                        }
                    }
                }
            }
            _ => {}
        }
    }
    Ok(())
}