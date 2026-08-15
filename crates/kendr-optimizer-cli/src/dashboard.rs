use std::io::{self, IsTerminal};

use crossterm::cursor::{Hide, Show};
use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use crossterm::execute;
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use ratatui::backend::CrosstermBackend;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph, Tabs, Wrap};
use ratatui::{Frame, Terminal};

use crate::ui;

const PANELS: [&str; 5] = ["Quick start", "Commands", "Harnesses", "Updates", "Trust"];
const SAFFRON: Color = Color::Rgb(ui::SAFFRON_RGB.0, ui::SAFFRON_RGB.1, ui::SAFFRON_RGB.2);
const DEEP_SAFFRON: Color = Color::Rgb(
    ui::DEEP_SAFFRON_RGB.0,
    ui::DEEP_SAFFRON_RGB.1,
    ui::DEEP_SAFFRON_RGB.2,
);
const INK: Color = Color::Rgb(ui::INK_RGB.0, ui::INK_RGB.1, ui::INK_RGB.2);

pub(crate) fn run() -> Result<(), Box<dyn std::error::Error>> {
    if !io::stdin().is_terminal() || !io::stdout().is_terminal() {
        return Err(io::Error::other(
            "the dashboard requires interactive stdin and stdout terminals; use `kendr-opt --help` for plain output",
        )
        .into());
    }
    if !ui::dashboard_available() {
        return Err(io::Error::other(
            "the dashboard is unavailable when TERM=dumb; use `kendr-opt --help` for plain output",
        )
        .into());
    }

    let _restore = TerminalRestore::enter()?;
    let backend = CrosstermBackend::new(io::stdout());
    let mut terminal = Terminal::new(backend)?;
    terminal.clear()?;

    let mut app = Dashboard::new(ui::dashboard_color_enabled());
    loop {
        terminal.draw(|frame| render(frame, &mut app))?;
        if let Event::Key(key) = event::read()?
            && matches!(key.kind, KeyEventKind::Press | KeyEventKind::Repeat)
            && app.handle_key(key)
        {
            break;
        }
    }
    Ok(())
}

struct TerminalRestore;

impl TerminalRestore {
    fn enter() -> io::Result<Self> {
        enable_raw_mode()?;
        let mut stdout = io::stdout();
        if let Err(error) = execute!(stdout, EnterAlternateScreen) {
            restore_terminal();
            return Err(error);
        }
        if let Err(error) = execute!(stdout, Hide) {
            restore_terminal();
            return Err(error);
        }
        Ok(Self)
    }
}

impl Drop for TerminalRestore {
    fn drop(&mut self) {
        restore_terminal();
    }
}

fn restore_terminal() {
    let _ = disable_raw_mode();
    let mut stdout = io::stdout();
    let _ = execute!(stdout, Show);
    let _ = execute!(stdout, LeaveAlternateScreen);
}

struct Dashboard {
    panel: usize,
    scroll: u16,
    max_scroll: u16,
    help: bool,
    color: bool,
}

impl Dashboard {
    fn new(color: bool) -> Self {
        Self {
            panel: 0,
            scroll: 0,
            max_scroll: 0,
            help: false,
            color,
        }
    }

    fn handle_key(&mut self, key: KeyEvent) -> bool {
        match key.code {
            KeyCode::Char('q') | KeyCode::Esc => return true,
            KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => return true,
            KeyCode::Right | KeyCode::Tab => self.select((self.panel + 1) % PANELS.len()),
            KeyCode::Left | KeyCode::BackTab => {
                self.select((self.panel + PANELS.len() - 1) % PANELS.len());
            }
            KeyCode::Down | KeyCode::Char('j') => {
                self.scroll = self.scroll.saturating_add(1).min(self.max_scroll);
            }
            KeyCode::Up | KeyCode::Char('k') => {
                self.scroll = self.scroll.saturating_sub(1);
            }
            KeyCode::Char('?') => {
                self.help = !self.help;
                self.scroll = 0;
            }
            _ => {}
        }
        false
    }

    fn select(&mut self, panel: usize) {
        self.panel = panel;
        self.scroll = 0;
        self.help = false;
    }

    fn accent(&self) -> Style {
        let style = Style::default().add_modifier(Modifier::BOLD);
        if self.color {
            style.fg(SAFFRON).bg(INK)
        } else {
            style
        }
    }

    fn border(&self) -> Style {
        if self.color {
            Style::default().fg(DEEP_SAFFRON)
        } else {
            Style::default()
        }
    }
}

fn render(frame: &mut Frame<'_>, app: &mut Dashboard) {
    let area = frame.area();
    if area.width < 28 || area.height < 7 {
        render_tiny(frame, area, app);
        return;
    }

    let compact = area.width < 72 || area.height < 18;
    let header_height = if compact { 1 } else { 3 };
    let tabs_height = if compact { 1 } else { 3 };
    let [header, tabs, body, footer] = Layout::vertical([
        Constraint::Length(header_height),
        Constraint::Length(tabs_height),
        Constraint::Min(3),
        Constraint::Length(1),
    ])
    .areas(area);

    render_header(frame, header, app, compact);
    render_tabs(frame, tabs, app, compact);
    render_body(frame, body, app);
    render_footer(frame, footer, app, compact);
}

fn render_tiny(frame: &mut Frame<'_>, area: Rect, app: &Dashboard) {
    let text = vec![
        Line::from(Span::styled("KENDR OPTIMIZER", app.accent())),
        Line::from("Terminal too small"),
        Line::from(format!("Need 28x7; now {}x{}", area.width, area.height)),
        Line::from("Resize or q/Esc to quit"),
    ];
    frame.render_widget(Paragraph::new(text).wrap(Wrap { trim: true }), area);
}

fn render_header(frame: &mut Frame<'_>, area: Rect, app: &Dashboard, compact: bool) {
    let line = if compact {
        Line::from(vec![
            Span::styled(" KENDR OPTIMIZER ", app.accent()),
            Span::raw(format!("v{}", env!("CARGO_PKG_VERSION"))),
        ])
    } else {
        Line::from(vec![
            Span::styled(" KENDR OPTIMIZER ", app.accent()),
            Span::raw("local, provider-neutral token reduction  "),
            Span::styled(format!("v{}", env!("CARGO_PKG_VERSION")), app.accent()),
        ])
    };
    let block = if compact {
        Block::default()
    } else {
        Block::default()
            .borders(Borders::BOTTOM)
            .border_style(app.border())
    };
    frame.render_widget(Paragraph::new(line).block(block), area);
}

fn render_tabs(frame: &mut Frame<'_>, area: Rect, app: &Dashboard, compact: bool) {
    if compact {
        let indicator = format!(
            " [{}/{}] {}  (left/right or Tab)",
            app.panel + 1,
            PANELS.len(),
            PANELS[app.panel]
        );
        frame.render_widget(Paragraph::new(indicator).style(app.accent()), area);
        return;
    }

    let tabs = Tabs::new(PANELS)
        .select(app.panel)
        .style(Style::default())
        .highlight_style(app.accent())
        .divider(Span::raw("  "))
        .padding(" ", " ")
        .block(
            Block::default()
                .borders(Borders::BOTTOM)
                .border_style(app.border()),
        );
    frame.render_widget(tabs, area);
}

fn render_body(frame: &mut Frame<'_>, area: Rect, app: &mut Dashboard) {
    let (title, lines) = if app.help {
        ("Keyboard help", help_lines(app))
    } else {
        (PANELS[app.panel], panel_lines(app))
    };
    let block = Block::default()
        .title(Span::styled(format!(" {title} "), app.accent()))
        .borders(Borders::ALL)
        .border_style(app.border());
    let inner_width = area.width.saturating_sub(2).max(1);
    let inner_height = area.height.saturating_sub(2) as usize;
    let line_count = wrapped_line_count(&lines, inner_width);
    let paragraph = Paragraph::new(lines)
        .block(block)
        .wrap(Wrap { trim: false });
    app.max_scroll = line_count
        .saturating_sub(inner_height)
        .min(u16::MAX as usize) as u16;
    app.scroll = app.scroll.min(app.max_scroll);
    frame.render_widget(paragraph.scroll((app.scroll, 0)), area);
}

fn wrapped_line_count(lines: &[Line<'_>], width: u16) -> usize {
    let width = usize::from(width.max(1));
    lines
        .iter()
        .map(|line| {
            let text = line
                .spans
                .iter()
                .map(|span| span.content.as_ref())
                .collect::<String>();
            wrapped_text_line_count(&text, width)
        })
        .sum()
}

fn wrapped_text_line_count(text: &str, width: usize) -> usize {
    if text.is_empty() {
        return 1;
    }

    let mut lines = 1usize;
    let mut occupied = 0usize;
    for segment in text.split_inclusive(char::is_whitespace) {
        let segment_width = Span::raw(segment).width();
        if segment_width <= width {
            if occupied > 0 && occupied.saturating_add(segment_width) > width {
                lines = lines.saturating_add(1);
                occupied = 0;
            }
            occupied = occupied.saturating_add(segment_width);
            continue;
        }

        if occupied > 0 {
            lines = lines.saturating_add(1);
        }
        lines = lines.saturating_add(segment_width.saturating_sub(1) / width);
        occupied = segment_width % width;
        if occupied == 0 {
            occupied = width;
        }
    }
    lines
}

fn render_footer(frame: &mut Frame<'_>, area: Rect, app: &Dashboard, compact: bool) {
    let text = if compact {
        " q quit  ? help  arrows navigate "
    } else {
        " q/Esc quit  |  left/right or Tab switch  |  up/down or j/k scroll  |  ? help "
    };
    frame.render_widget(Paragraph::new(text).style(app.accent()), area);
}

fn panel_lines(app: &Dashboard) -> Vec<Line<'static>> {
    match app.panel {
        0 => quick_start_lines(app),
        1 => command_lines(app),
        2 => harness_lines(app),
        3 => update_lines(app),
        _ => trust_lines(app),
    }
}

fn quick_start_lines(app: &Dashboard) -> Vec<Line<'static>> {
    vec![
        lead("READ-ONLY GUIDE", app),
        text(
            "This dashboard explains the local CLI. It does not install, configure, update, or contact a provider.",
        ),
        blank(),
        step("1", "See compatible harnesses", app),
        command("kendr-opt setup --list", app),
        blank(),
        step("2", "Configure one installed harness", app),
        command("kendr-opt setup claude-code", app),
        blank(),
        step("3", "Launch it through Kendr", app),
        command("kendr-opt run claude-code", app),
        blank(),
        step("4", "Check the verified release channel", app),
        command("kendr-opt update --check", app),
    ]
}

fn command_lines(app: &Dashboard) -> Vec<Line<'static>> {
    vec![
        lead("HUMAN WORKFLOWS", app),
        entry("dashboard (tui)", "open this read-only guide", app),
        entry("setup", "install bundled harness adapters", app),
        entry("run", "start the local service and launch a harness", app),
        entry("update", "check or install a verified GitHub release", app),
        blank(),
        lead("MACHINE-READABLE WORKFLOWS", app),
        entry("analyze", "shadow optimization receipt as JSON", app),
        entry("optimize", "optimized envelope and receipt as JSON", app),
        entry(
            "restore",
            "recover an original envelope from a capsule",
            app,
        ),
        entry("observe", "compare provider usage with a baseline", app),
        entry("engines", "list native engines as JSON", app),
        entry("serve", "run the transform-only HTTP service", app),
        blank(),
        text("Use `kendr-opt <command> --help` for every flag and input/output contract."),
    ]
}

fn harness_lines(app: &Dashboard) -> Vec<Line<'static>> {
    vec![
        lead("AUTOMATIC SETUP", app),
        entry("claude-code", "local plugin; Node.js 22+ bridge", app),
        entry("opencode", "global local plugin", app),
        entry("pi", "global extension", app),
        entry("openclaw", "managed plugin and context-engine slot", app),
        entry("hermes", "user plugin", app),
        blank(),
        command("kendr-opt setup --list", app),
        command("kendr-opt run <harness> -- [harness arguments]", app),
        blank(),
        text("The host CLI keeps model selection, provider credentials, routing, and billing."),
    ]
}

fn update_lines(app: &Dashboard) -> Vec<Line<'static>> {
    vec![
        lead("EXPLICIT UPDATES", app),
        entry(
            "check",
            "read release metadata without replacing the CLI",
            app,
        ),
        command("kendr-opt update --check", app),
        command("kendr-opt update --check --json", app),
        blank(),
        entry(
            "install",
            "verify and replace the official installed CLI",
            app,
        ),
        command("kendr-opt update", app),
        blank(),
        text(
            "Updates require an eligible immutable GitHub release, matching digests, a safe archive, and candidate smoke tests.",
        ),
        text(
            "No update is installed passively. Setup/run notices are terminal-only and can be disabled with KENDR_NO_UPDATE_CHECK=1.",
        ),
    ]
}

fn trust_lines(app: &Dashboard) -> Vec<Line<'static>> {
    vec![
        lead("LOCAL BOUNDARY", app),
        entry(
            "Transform-only",
            "Kendr returns content to the host; it does not call an LLM",
            app,
        ),
        entry(
            "Provider-neutral",
            "provider choice and credentials stay with the host",
            app,
        ),
        entry(
            "Loopback",
            "the optional service defaults to 127.0.0.1:7331",
            app,
        ),
        entry(
            "Auditable",
            "typed receipts describe changes and validation",
            app,
        ),
        blank(),
        lead("THIS SCREEN", app),
        entry(
            "Read-only",
            "no files, configuration, releases, or prompts are changed",
            app,
        ),
        entry(
            "Offline",
            "opening the dashboard performs no network request",
            app,
        ),
        blank(),
        text(
            "Review the threat model and benchmark methodology before production use. Kendr Optimizer is pre-alpha.",
        ),
    ]
}

fn help_lines(app: &Dashboard) -> Vec<Line<'static>> {
    vec![
        lead("NAVIGATION", app),
        entry("Left / Right", "previous or next panel", app),
        entry("Tab / BackTab", "next or previous panel", app),
        entry("Up / Down", "scroll the active panel", app),
        entry("j / k", "scroll down or up", app),
        entry("?", "return to the active panel", app),
        entry("q / Esc", "leave the dashboard", app),
        blank(),
        text("The terminal is restored automatically when the dashboard exits."),
    ]
}

fn lead(value: &'static str, app: &Dashboard) -> Line<'static> {
    Line::from(Span::styled(value, app.accent()))
}

fn step(number: &'static str, description: &'static str, app: &Dashboard) -> Line<'static> {
    Line::from(vec![
        Span::styled(format!("STEP {number}  "), app.accent()),
        Span::raw(description),
    ])
}

fn entry(label: &'static str, description: &'static str, app: &Dashboard) -> Line<'static> {
    Line::from(vec![
        Span::styled(format!("{label:<18}"), app.accent()),
        Span::raw(description),
    ])
}

fn command(value: &'static str, app: &Dashboard) -> Line<'static> {
    Line::from(vec![
        Span::styled("  $ ", app.accent()),
        Span::styled(value, Style::default().add_modifier(Modifier::BOLD)),
    ])
}

fn text(value: &'static str) -> Line<'static> {
    Line::from(value)
}

fn blank() -> Line<'static> {
    Line::default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::backend::TestBackend;

    #[test]
    fn wide_render_names_every_panel_and_uses_saffron() {
        let mut terminal = Terminal::new(TestBackend::new(110, 28)).unwrap();
        let mut app = Dashboard::new(true);
        terminal.draw(|frame| render(frame, &mut app)).unwrap();

        let buffer = terminal.backend().buffer();
        let rendered = buffer
            .content
            .iter()
            .map(|cell| cell.symbol())
            .collect::<String>();
        for title in PANELS {
            assert!(rendered.contains(title), "render omitted {title}");
        }
        assert!(rendered.contains("READ-ONLY GUIDE"));
        assert!(
            buffer
                .content
                .iter()
                .any(|cell| cell.fg == SAFFRON && cell.bg == INK)
        );
    }

    #[test]
    fn compact_render_and_all_panels_are_safe() {
        let mut terminal = Terminal::new(TestBackend::new(34, 10)).unwrap();
        for (panel, title) in PANELS.iter().enumerate() {
            let mut app = Dashboard::new(false);
            app.panel = panel;
            terminal.draw(|frame| render(frame, &mut app)).unwrap();
            let rendered = terminal
                .backend()
                .buffer()
                .content
                .iter()
                .map(|cell| cell.symbol())
                .collect::<String>();
            assert!(rendered.contains(title));
        }
    }

    #[test]
    fn tiny_render_requests_resize_without_advertising_hidden_controls() {
        let mut terminal = Terminal::new(TestBackend::new(27, 6)).unwrap();
        let mut app = Dashboard::new(false);
        terminal.draw(|frame| render(frame, &mut app)).unwrap();
        let rendered = terminal
            .backend()
            .buffer()
            .content
            .iter()
            .map(|cell| cell.symbol())
            .collect::<String>();
        assert!(rendered.contains("Terminal too small"));
        assert!(rendered.contains("Need 28x7"));
        assert!(!rendered.contains("? help"));
    }

    #[test]
    fn narrow_panels_can_scroll_to_their_final_line() {
        let mut terminal = Terminal::new(TestBackend::new(34, 10)).unwrap();
        let final_markers = [
            "--check",
            "contract.",
            "billing.",
            "KENDR_NO_UPDATE_CHECK=1.",
            "pre-alpha.",
        ];
        for (panel, marker) in final_markers.into_iter().enumerate() {
            let mut app = Dashboard::new(false);
            app.panel = panel;
            terminal.draw(|frame| render(frame, &mut app)).unwrap();
            app.scroll = app.max_scroll;
            terminal.draw(|frame| render(frame, &mut app)).unwrap();
            let rendered = terminal
                .backend()
                .buffer()
                .content
                .iter()
                .map(|cell| cell.symbol())
                .collect::<String>();
            assert!(
                rendered.contains(marker),
                "panel {} could not scroll to {marker:?}; max scroll {}",
                PANELS[panel],
                app.max_scroll
            );
        }
    }
}
