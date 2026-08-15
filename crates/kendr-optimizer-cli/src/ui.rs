use std::env;
use std::fmt::Display;
use std::io::{self, IsTerminal, Write};

use anstream::{AutoStream, ColorChoice};
use clap::builder::styling::{AnsiColor, Color, RgbColor, Style, Styles};

pub(crate) const SAFFRON_RGB: (u8, u8, u8) = (226, 113, 42);
pub(crate) const DEEP_SAFFRON_RGB: (u8, u8, u8) = (184, 85, 26);
pub(crate) const INK_RGB: (u8, u8, u8) = (43, 41, 37);

const SAFFRON: Color = Color::Rgb(RgbColor(SAFFRON_RGB.0, SAFFRON_RGB.1, SAFFRON_RGB.2));
const DEEP_SAFFRON: Color = Color::Rgb(RgbColor(
    DEEP_SAFFRON_RGB.0,
    DEEP_SAFFRON_RGB.1,
    DEEP_SAFFRON_RGB.2,
));
const INK: Color = Color::Rgb(RgbColor(INK_RGB.0, INK_RGB.1, INK_RGB.2));
const SAFFRON_BOLD: Style = Style::new()
    .fg_color(Some(SAFFRON))
    .bg_color(Some(INK))
    .bold();
const DEEP_SAFFRON_STYLE: Style = Style::new().fg_color(Some(DEEP_SAFFRON));
const SUCCESS: Style = Style::new()
    .fg_color(Some(Color::Ansi(AnsiColor::BrightGreen)))
    .bold();
const ERROR: Style = Style::new()
    .fg_color(Some(Color::Ansi(AnsiColor::BrightRed)))
    .bold();

pub(crate) const CLAP_STYLES: Styles = Styles::styled()
    .header(SAFFRON_BOLD)
    .usage(SAFFRON_BOLD)
    .literal(SAFFRON_BOLD)
    .placeholder(DEEP_SAFFRON_STYLE)
    .error(ERROR)
    .valid(SUCCESS)
    .invalid(ERROR);

pub(crate) fn clap_color_choice() -> clap::ColorChoice {
    if term_is_dumb() || automated_environment() || no_color_requested() {
        clap::ColorChoice::Never
    } else {
        clap::ColorChoice::Auto
    }
}

pub(crate) fn dashboard_available() -> bool {
    io::stdin().is_terminal() && io::stdout().is_terminal() && !term_is_dumb()
}

pub(crate) fn dashboard_color_enabled() -> bool {
    io::stdout().is_terminal() && color_environment_allowed()
}

pub(crate) fn stderr_color_enabled() -> bool {
    let stream = io::stderr();
    stream.is_terminal()
        && color_environment_allowed()
        && !matches!(AutoStream::choice(&stream), ColorChoice::Never)
}

fn stdout_color_enabled() -> bool {
    let stream = io::stdout();
    stream.is_terminal()
        && color_environment_allowed()
        && !matches!(AutoStream::choice(&stream), ColorChoice::Never)
}

fn color_environment_allowed() -> bool {
    !term_is_dumb() && !automated_environment() && !no_color_requested()
}

fn no_color_requested() -> bool {
    env::var_os("NO_COLOR").is_some()
}

fn term_is_dumb() -> bool {
    env::var_os("TERM").is_some_and(|value| value.to_string_lossy().eq_ignore_ascii_case("dumb"))
}

fn automated_environment() -> bool {
    env::var_os("CI").is_some() || env::var_os("GITHUB_ACTIONS").is_some()
}

pub(crate) fn print_setup_list(text: &str) -> io::Result<()> {
    if !stdout_color_enabled() {
        let mut stdout = io::stdout().lock();
        writeln!(stdout, "{text}")?;
        return Ok(());
    }

    let mut stdout = AutoStream::new(io::stdout(), ColorChoice::Always).lock();
    write_setup_list(&mut stdout, text, true)
}

pub(crate) fn print_setup_messages(messages: &[String]) -> io::Result<()> {
    if !stdout_color_enabled() {
        let mut stdout = io::stdout().lock();
        for message in messages {
            writeln!(stdout, "{message}")?;
        }
        return Ok(());
    }

    let mut stdout = AutoStream::new(io::stdout(), ColorChoice::Always).lock();
    write_setup_messages(&mut stdout, messages, true)
}

pub(crate) fn print_error(error: &dyn Display) {
    if !stderr_color_enabled() {
        eprintln!("kendr-opt: {error}");
        return;
    }

    let mut stderr = AutoStream::new(io::stderr(), ColorChoice::Always).lock();
    let _ = write_styled(&mut stderr, ERROR, "ERROR");
    let _ = write!(stderr, " ");
    let _ = write_styled(&mut stderr, SAFFRON_BOLD, "kendr-opt");
    let _ = writeln!(stderr, ": {error}");
}

fn write_setup_list(writer: &mut impl Write, text: &str, color: bool) -> io::Result<()> {
    if !color {
        return writeln!(writer, "{text}");
    }

    write_styled(writer, SAFFRON_BOLD, "KENDR / SETUP")?;
    writeln!(writer)?;
    for line in text.lines() {
        if line == "Supported automatic setup:" {
            write_styled(writer, DEEP_SAFFRON_STYLE, "AUTOMATIC")?;
            writeln!(writer)?;
        } else if let Some(rest) = line.strip_prefix("  ") {
            let split = rest.find(char::is_whitespace).unwrap_or(rest.len());
            let (harness, description) = rest.split_at(split);
            write!(writer, "  ")?;
            write_styled(writer, SAFFRON_BOLD, harness)?;
            writeln!(writer, "{description}")?;
        } else {
            writeln!(writer, "{line}")?;
        }
    }
    Ok(())
}

fn write_setup_messages(
    writer: &mut impl Write,
    messages: &[String],
    color: bool,
) -> io::Result<()> {
    if !color {
        for message in messages {
            writeln!(writer, "{message}")?;
        }
        return Ok(());
    }

    write_styled(writer, SAFFRON_BOLD, "KENDR / SETUP")?;
    writeln!(writer)?;
    for message in messages {
        write!(writer, "  ")?;
        write_styled(writer, SUCCESS, "OK")?;
        writeln!(writer, "  {message}")?;
    }
    Ok(())
}

fn write_styled(writer: &mut impl Write, style: Style, value: &str) -> io::Result<()> {
    write!(writer, "{}{value}{}", style.render(), style.render_reset())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plain_setup_output_has_no_terminal_escapes() {
        let mut output = Vec::new();
        write_setup_list(
            &mut output,
            "Supported automatic setup:\n  claude-code local plugin",
            false,
        )
        .unwrap();
        assert_eq!(
            String::from_utf8(output).unwrap(),
            "Supported automatic setup:\n  claude-code local plugin\n"
        );
    }

    #[test]
    fn styled_setup_output_uses_kendr_saffron_and_text_labels() {
        let mut output = Vec::new();
        write_setup_messages(&mut output, &["Configured claude-code.".into()], true).unwrap();
        let output = String::from_utf8(output).unwrap();
        assert!(output.contains("38;2;226;113;42"));
        assert!(output.contains("48;2;43;41;37"));
        assert!(output.contains("KENDR / SETUP"));
        assert!(output.contains("OK"));
        assert!(output.contains("Configured claude-code."));
    }
}
