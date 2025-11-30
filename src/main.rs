use anyhow::{Context, Result, anyhow};
use clap::{Parser, arg, command};
use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEventKind},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::widgets::Clear;
use ratatui::{
    Terminal,
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph, Wrap},
};
use std::fs;
use std::io::{self, Write};
use std::path::PathBuf;

/// Minimal unified-diff hunk representation and file headers
#[derive(Debug, Clone)]
struct Hunk {
    header: String,     // "@@ -a,b +c,d @@ optional"
    lines: Vec<String>, // the hunk body lines (start with ' ', '+', '-', or '\')
    file_idx: usize,    // index into files[]
    marked: bool,
    display: String, // short preview for list
}

#[derive(Debug, Clone)]
struct FileDiff {
    // All header-ish lines *starting from* `diff --git` (inclusive) up to the first "@@" or next "diff --git"
    headers: Vec<String>, // includes: diff --git, index, ---/+++ etc. (in original order)
    hunks: Vec<usize>,    // indices into hunks[]
    // For UI convenience:
    file_label: String, // e.g. "a/foo.c → b/foo.c"
}

/// Very simple unified-diff parser that’s resilient to extra metadata sections.
fn parse_unified_diff(input: &str) -> Result<(Vec<FileDiff>, Vec<Hunk>)> {
    let mut files: Vec<FileDiff> = Vec::new();
    let mut hunks: Vec<Hunk> = Vec::new();

    // Track “current file”
    let mut current_file: Option<usize> = None;
    let mut pending_headers: Vec<String> = Vec::new();

    // Track hunk capture
    let mut capturing_hunk = false;
    let mut hunk_header = String::new();
    let mut hunk_lines: Vec<String> = Vec::new();

    // A small helper to flush any open hunk
    let mut finish_hunk = |files: &mut Vec<FileDiff>,
                           hunks: &mut Vec<Hunk>,
                           current_file: Option<usize>,
                           hunk_header: &mut String,
                           hunk_lines: &mut Vec<String>| {
        if !hunk_header.is_empty() {
            let file_idx = current_file.expect("hunk without file");
            let preview = make_hunk_preview(hunk_header, hunk_lines);
            let idx = hunks.len();
            hunks.push(Hunk {
                header: std::mem::take(hunk_header),
                lines: std::mem::take(hunk_lines),
                file_idx,
                marked: false,
                display: preview,
            });
            files[file_idx].hunks.push(idx);
        }
    };

    // Emit new FileDiff from pending_headers when we see `diff --git` for a new file
    let mut start_new_file = |files: &mut Vec<FileDiff>, pending: &mut Vec<String>| {
        let label = extract_file_label(pending);
        files.push(FileDiff {
            headers: std::mem::take(pending),
            hunks: Vec::new(),
            file_label: label,
        });
        files.len() - 1
    };

    for line in input.lines() {
        if line.starts_with("diff --git ") {
            // If a hunk is open, close it
            if capturing_hunk {
                finish_hunk(
                    &mut files,
                    &mut hunks,
                    current_file,
                    &mut hunk_header,
                    &mut hunk_lines,
                );
                capturing_hunk = false;
            }
            // If we already have a current file, finalize its headers (already stored)
            if let Some(_) = current_file {
                // nothing to flush besides hunks; headers already on that file
            }
            // Start collecting headers for the *new* file
            // First, if previous pending headers exist without having been turned into a file, that’s weird;
            // but we’ll start fresh.
            pending_headers = vec![line.to_string()];
            current_file = None; // will be set when we hit first "@@" or when we encounter ---/+++; for safety we create file immediately
            // We can eagerly create the file now so any subsequent headers attach to it.
            let idx = start_new_file(&mut files, &mut pending_headers);
            current_file = Some(idx);
        } else if line.starts_with("@@ ") || line.starts_with("@@-") || line.starts_with("@@+") {
            // starting a hunk
            if capturing_hunk {
                // This should not happen in a normal diff, but close previous one defensively
                finish_hunk(
                    &mut files,
                    &mut hunks,
                    current_file,
                    &mut hunk_header,
                    &mut hunk_lines,
                );
                capturing_hunk = false;
            }
            if current_file.is_none() {
                // We didn’t see diff --git for some reason; start a synthetic file bucket
                let idx = start_new_file(&mut files, &mut pending_headers);
                current_file = Some(idx);
            }
            capturing_hunk = true;
            hunk_header = line.to_string();
            hunk_lines.clear();
        } else {
            // Either header-ish or hunk body
            if capturing_hunk {
                // Allow any line in hunk (including trailing “\ No newline at end of file”)
                hunk_lines.push(line.to_string());
            } else {
                // file headers: accumulate onto current file
                if current_file.is_none() {
                    pending_headers.push(line.to_string());
                } else {
                    files[current_file.unwrap()].headers.push(line.to_string());
                }
            }
        }
    }

    // Close tailing hunk if any
    if capturing_hunk {
        finish_hunk(
            &mut files,
            &mut hunks,
            current_file,
            &mut hunk_header,
            &mut hunk_lines,
        );
    }

    Ok((files, hunks))
}

fn extract_file_label(headers: &[String]) -> String {
    // Try to synthesize something like "a/foo → b/foo" using ---/+++ or the diff --git line.
    let mut from = String::new();
    let mut to = String::new();
    for l in headers {
        if l.starts_with("diff --git ") {
            // format: diff --git a/foo b/foo
            let parts: Vec<_> = l.split_whitespace().collect();
            if parts.len() >= 4 {
                from = parts[2].to_string();
                to = parts[3].to_string();
            }
        } else if l.starts_with("--- ") {
            from = l.trim_start_matches("--- ").to_string();
        } else if l.starts_with("+++ ") {
            to = l.trim_start_matches("+++ ").to_string();
        }
    }
    if !from.is_empty() || !to.is_empty() {
        format!(
            "{} → {}",
            if from.is_empty() { "?" } else { &from },
            if to.is_empty() { "?" } else { &to }
        )
    } else {
        "file".into()
    }
}

fn make_hunk_preview(header: &str, lines: &[String]) -> String {
    let first_context = lines
        .iter()
        .find(|l| l.starts_with(' ') || l.starts_with('+') || l.starts_with('-'))
        .map(|s| s.trim())
        .unwrap_or("");
    let trimmed_header = header.trim().to_string();
    if first_context.is_empty() {
        trimmed_header
    } else {
        format!("{trimmed_header}  —  {first_context}")
    }
}

#[derive(Parser, Debug)]
#[command(
    version,
    about = "Select hunks from a patch in a TUI and write a filtered patch"
)]
struct Opts {
    /// Input patch file (unified diff)
    input: PathBuf,
    /// Output patch file to write whenever you press Space
    #[arg(short, long)]
    output: PathBuf,
}

struct App {
    files: Vec<FileDiff>,
    hunks: Vec<Hunk>,
    // Flattened list of (file_idx, hunk_idx) to present in UI order
    order: Vec<usize>, // indices into hunks[]
    cursor: usize,
    input_path: PathBuf,
    output_path: PathBuf,
    status: String,
    list_state: ListState,
}

impl App {
    fn new(files: Vec<FileDiff>, hunks: Vec<Hunk>, input: PathBuf, output: PathBuf) -> Self {
        let order: Vec<usize> = (0..hunks.len()).collect();
        let mut list_state = ListState::default();
        if !order.is_empty() {
            list_state.select(Some(0));
        }

        Self {
            files,
            hunks,
            order,
            cursor: 0,
            input_path: input,
            output_path: output,
            status: "↑/↓ to move, Space to toggle & SAVE, q to quit".into(),
            list_state,
        }
    }

    fn toggle_current_and_save(&mut self) -> Result<()> {
        if let Some(&idx) = self.order.get(self.cursor) {
            self.hunks[idx].marked = !self.hunks[idx].marked;
        }
        self.write_filtered_patch()
            .context("writing filtered patch after Space")?;
        let count = self.hunks.iter().filter(|h| h.marked).count();
        self.status = format!(
            "Saved {} selected hunk(s) → {}",
            count,
            self.output_path.display()
        );
        Ok(())
    }
    fn move_cursor(&mut self, dir: i32) {
        if self.order.is_empty() {
            self.cursor = 0;
            self.list_state.select(None);
            return;
        }
        let len = self.order.len() as i32;
        let mut cur = self.cursor as i32 + dir;
        if cur < 0 {
            cur = 0;
        }
        if cur >= len {
            cur = len - 1;
        }
        self.cursor = cur as usize;
        self.list_state.select(Some(self.cursor));
    }

    fn write_filtered_patch(&self) -> Result<()> {
        // Group selected hunks by file
        let mut out = String::new();
        for (fidx, f) in self.files.iter().enumerate() {
            let selected: Vec<&Hunk> = f
                .hunks
                .iter()
                .map(|&hidx| &self.hunks[hidx])
                .filter(|h| h.marked)
                .collect();
            if selected.is_empty() {
                continue;
            }
            // Write headers exactly as in the input
            for h in &f.headers {
                out.push_str(h);
                out.push('\n');
            }
            // Write selected hunks for this file
            for h in selected {
                out.push_str(&h.header);
                out.push('\n');
                for l in &h.lines {
                    out.push_str(l);
                    out.push('\n');
                }
            }
        }
        fs::write(&self.output_path, out)?;
        Ok(())
    }
}

fn main() -> Result<()> {
    let opts = Opts::parse();
    let input_text = fs::read_to_string(&opts.input)
        .with_context(|| format!("failed to read {}", opts.input.display()))?
        .replace("\r\n", "\n")
        .replace('\t', "    ");
    let (files, hunks) = parse_unified_diff(&input_text)?;
    if hunks.is_empty() {
        return Err(anyhow!("No hunks found in {}", opts.input.display()));
    }

    // Prepare app
    let mut app = App::new(files, hunks, opts.input, opts.output);

    // TUI setup
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let res = run_app(&mut terminal, &mut app);

    // TUI teardown
    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;

    if let Err(e) = res {
        eprintln!("error: {e:?}");
        std::process::exit(1);
    }
    Ok(())
}

fn run_app<B: ratatui::backend::Backend>(terminal: &mut Terminal<B>, app: &mut App) -> Result<()> {
    loop {
        terminal.draw(|f| {
            let area = f.area();
            // Vertical: [main, status]
            let v = Layout::default()
                .direction(Direction::Vertical)
                .constraints([Constraint::Min(1), Constraint::Length(3)].as_ref())
                .split(area);
            // Horizontal inside main: [list, preview]
            let h = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([Constraint::Percentage(45), Constraint::Percentage(55)].as_ref())
                .split(v[0]);

            // Build list items
            let items: Vec<ListItem> = app
                .order
                .iter()
                .enumerate()
                .map(|(i, &hidx)| {
                    let h = &app.hunks[hidx];
                    let prefix = if h.marked { "[x]" } else { "[ ]" };
                    let line = Line::from(vec![
                        Span::raw(format!("{prefix} ")),
                        Span::styled(
                            &app.files[h.file_idx].file_label,
                            Style::default().add_modifier(Modifier::BOLD),
                        ),
                        Span::raw("  "),
                        Span::raw(&h.display),
                    ]);
                    let mut item = ListItem::new(line);
                    if i == app.cursor {
                        item = item.style(Style::default().add_modifier(Modifier::REVERSED));
                    }
                    item
                })
                .collect();

            let list =
                List::new(items).block(Block::default().title("Hunks").borders(Borders::ALL));

            f.render_stateful_widget(list, h[0], &mut app.list_state);

            // === Right-hand PREVIEW ===
            let mut preview_lines: Vec<Line> = Vec::new();
            if let Some(&hidx) = app.order.get(app.cursor) {
                let hunk = &app.hunks[hidx];
                // Header line
                preview_lines.push(Line::from(Span::styled(
                    hunk.header.clone(),
                    Style::default().add_modifier(Modifier::BOLD),
                )));
                // Body lines with colorization by first char
                for l in &hunk.lines {
                    let (style, text) = match l.chars().next() {
                        Some('+') => (Style::default().fg(Color::Green), l.clone()),
                        Some('-') => (Style::default().fg(Color::Red), l.clone()),
                        Some(' ') => (Style::default(), l.clone()),
                        Some('\\') => (Style::default().fg(Color::Gray), l.clone()),
                        _ => (Style::default(), l.clone()),
                    };
                    preview_lines.push(Line::from(Span::styled(text, style)));
                }
            } else {
                preview_lines.push(Line::from("No hunk selected"));
            }

            let preview_area = h[1];

            // Clear the preview area so old content disappears
            f.render_widget(Clear, preview_area);

            let preview = Paragraph::new(preview_lines)
                .wrap(Wrap { trim: false })
                .block(Block::default().title("Preview").borders(Borders::ALL));

            f.render_widget(preview, preview_area);

            let help = Paragraph::new(vec![
                Line::from(app.status.clone()),
                Line::from("Keys: ↑/↓ or j/k = move • Space/Enter = toggle & save • q = quit"),
            ])
            .block(Block::default().borders(Borders::ALL).title("Status"));
            f.render_widget(help, v[1]);
        })?;

        // Input
        if event::poll(std::time::Duration::from_millis(250))? {
            if let Event::Key(key) = event::read()? {
                // Only react to "press" (not repeats)
                if key.kind != KeyEventKind::Press {
                    continue;
                }
                match key.code {
                    KeyCode::Char('q') => {
                        // Quit. Nothing else to do; file has been kept updated on every Space.
                        return Ok(());
                    }
                    KeyCode::Up => app.move_cursor(-1),
                    KeyCode::Down => app.move_cursor(1),
                    KeyCode::Char('k') => app.move_cursor(-1),
                    KeyCode::Char('j') => app.move_cursor(1),
                    KeyCode::Char(' ') | KeyCode::Enter => {
                        if let Err(e) = app.toggle_current_and_save() {
                            app.status = format!("ERROR: {e:#}");
                        }
                    }
                    _ => {}
                }
            }
        }
    }
}
