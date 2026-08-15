use std::env;
#[cfg(windows)]
use std::ffi::OsStr;
use std::ffi::OsString;
use std::fmt;
use std::fs;
use std::io;
use std::net::{IpAddr, Ipv4Addr, SocketAddr, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use clap::ValueEnum;

const MANAGED_MARKER: &str = "Installed by Kendr Optimizer";
const CORE_ADDRESS: SocketAddr = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 7331);
const CLAUDE_BRIDGE_ADDRESS: SocketAddr = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 7332);

const OPENCODE_PLUGIN: &str =
    include_str!("../../../integrations/opencode/dist/kendr-optimizer.js");
const PI_PLUGIN: &str = include_str!("../../../integrations/pi-agent/dist/index.js");
const CLAUDE_PLUGIN_JSON: &str =
    include_str!("../../../integrations/claude-code/.claude-plugin/plugin.json");
const CLAUDE_HOOKS_JSON: &str = include_str!("../../../integrations/claude-code/hooks/hooks.json");
const CLAUDE_INDEX_JS: &str = include_str!("../../../integrations/claude-code/dist/index.js");
const CLAUDE_SERVER_JS: &str = include_str!("../../../integrations/claude-code/dist/server.js");
const OPENCLAW_PACKAGE_JSON: &str = include_str!("../../../integrations/openclaw/package.json");
const OPENCLAW_PLUGIN_JSON: &str =
    include_str!("../../../integrations/openclaw/openclaw.plugin.json");
const OPENCLAW_INDEX_JS: &str = include_str!("../../../integrations/openclaw/dist/index.js");
const HERMES_INIT_PY: &str =
    include_str!("../../../integrations/hermes-agent/src/kendr_hermes_plugin/__init__.py");
const HERMES_CLIENT_PY: &str =
    include_str!("../../../integrations/hermes-agent/src/kendr_hermes_plugin/client.py");
const HERMES_CODEC_PY: &str =
    include_str!("../../../integrations/hermes-agent/src/kendr_hermes_plugin/codec.py");
const HERMES_CONFIG_PY: &str =
    include_str!("../../../integrations/hermes-agent/src/kendr_hermes_plugin/config.py");
const HERMES_PLUGIN_YAML: &str =
    include_str!("../../../integrations/hermes-agent/src/kendr_hermes_plugin/plugin.yaml");

#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
pub(crate) enum Harness {
    #[value(name = "opencode")]
    OpenCode,
    #[value(name = "claude-code", alias = "claude")]
    ClaudeCode,
    #[value(name = "pi", alias = "pi-agent")]
    Pi,
    #[value(name = "openclaw")]
    OpenClaw,
    #[value(name = "hermes", alias = "hermes-agent")]
    Hermes,
}

impl Harness {
    const ALL: [Self; 5] = [
        Self::OpenCode,
        Self::ClaudeCode,
        Self::Pi,
        Self::OpenClaw,
        Self::Hermes,
    ];

    fn executable(self) -> &'static str {
        match self {
            Self::OpenCode => "opencode",
            Self::ClaudeCode => "claude",
            Self::Pi => "pi",
            Self::OpenClaw => "openclaw",
            Self::Hermes => "hermes",
        }
    }

    fn default_arguments(self) -> &'static [&'static str] {
        match self {
            Self::OpenClaw => &["tui"],
            _ => &[],
        }
    }
}

impl fmt::Display for Harness {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::OpenCode => "opencode",
            Self::ClaudeCode => "claude-code",
            Self::Pi => "pi",
            Self::OpenClaw => "openclaw",
            Self::Hermes => "hermes",
        })
    }
}

pub(crate) fn support_text() -> &'static str {
    "Supported automatic setup:\n\
  opencode     global local plugin\n\
  claude-code local plugin launched by `kendr-opt run claude-code`\n\
  pi           global extension\n\
  openclaw     managed local plugin and context-engine slot\n\
  hermes       user plugin\n\
\n\
OpenAI's coding CLI has no supported pre-dispatch context hook. NanoClaw requires its\n\
guarded source skill, and Claude Channels is a library integration."
}

pub(crate) fn setup(
    requested: Option<Harness>,
    force: bool,
) -> Result<Vec<String>, Box<dyn std::error::Error>> {
    let paths = InstallPaths::discover()?;
    let harnesses = match requested {
        Some(harness) => vec![harness],
        None => Harness::ALL
            .into_iter()
            .filter(|harness| command_exists(harness.executable()))
            .collect(),
    };
    if harnesses.is_empty() {
        return Err(other_error(
            "No supported harness was found on PATH. Install OpenCode, Claude Code, Pi, OpenClaw, or Hermes first.",
        ));
    }

    let mut messages = Vec::new();
    for harness in harnesses {
        messages.push(setup_one(harness, force, &paths)?);
    }
    Ok(messages)
}

pub(crate) fn run(
    harness: Harness,
    arguments: &[OsString],
    force: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    if !command_exists(harness.executable()) {
        return Err(other_error(format!(
            "{} is not installed or is not on PATH",
            harness.executable()
        )));
    }

    let paths = InstallPaths::discover()?;
    setup_one(harness, force, &paths)?;

    let mut core = spawn_service_if_needed()?;
    let mut bridge = if harness == Harness::ClaudeCode {
        match spawn_claude_bridge_if_needed(&paths) {
            Ok(child) => child,
            Err(error) => {
                stop_child(&mut core);
                return Err(error);
            }
        }
    } else {
        None
    };

    let mut command = resolved_command(harness.executable())?;
    if harness == Harness::ClaudeCode {
        command
            .arg("--plugin-dir")
            .arg(paths.adapter_root.join("claude-code"));
    }
    if arguments.is_empty() {
        command.args(harness.default_arguments());
    } else {
        command.args(arguments);
    }

    let status = command.status();
    stop_child(&mut bridge);
    stop_child(&mut core);
    let status = status?;
    if !status.success() {
        return Err(other_error(format!(
            "{harness} exited with status {status}"
        )));
    }
    Ok(())
}

fn setup_one(
    harness: Harness,
    force: bool,
    paths: &InstallPaths,
) -> Result<String, Box<dyn std::error::Error>> {
    if !command_exists(harness.executable()) {
        return Err(other_error(format!(
            "{} is not installed or is not on PATH",
            harness.executable()
        )));
    }

    match harness {
        Harness::OpenCode => setup_opencode(paths, force)?,
        Harness::ClaudeCode => setup_claude_code(paths, force)?,
        Harness::Pi => setup_pi(paths, force)?,
        Harness::OpenClaw => setup_openclaw(paths, force)?,
        Harness::Hermes => setup_hermes(paths, force)?,
    }
    Ok(format!(
        "Configured {harness}. Start it with: kendr-opt run {harness}"
    ))
}

fn setup_opencode(paths: &InstallPaths, force: bool) -> Result<(), Box<dyn std::error::Error>> {
    let target = paths
        .opencode_config
        .join("plugins")
        .join("kendr-optimizer.js");
    write_marked_file(&target, OPENCODE_PLUGIN, "//", force)
}

fn setup_pi(paths: &InstallPaths, force: bool) -> Result<(), Box<dyn std::error::Error>> {
    let target = paths
        .home
        .join(".pi")
        .join("agent")
        .join("extensions")
        .join("kendr-optimizer.js");
    write_marked_file(&target, PI_PLUGIN, "//", force)
}

fn setup_claude_code(paths: &InstallPaths, force: bool) -> Result<(), Box<dyn std::error::Error>> {
    if !command_exists("node") {
        return Err(other_error(
            "Claude Code integration currently requires Node.js 22 or newer for its local hook bridge.",
        ));
    }
    let node_version = command_output("node", &["--version"])?;
    let node_major = node_version
        .trim()
        .trim_start_matches('v')
        .split('.')
        .next()
        .and_then(|value| value.parse::<u32>().ok())
        .unwrap_or(0);
    if node_major < 22 {
        return Err(other_error(format!(
            "Claude Code integration requires Node.js 22 or newer; found {}",
            node_version.trim()
        )));
    }
    let root = paths.adapter_root.join("claude-code");
    prepare_managed_directory(&root, force)?;
    write_file(&root.join(".claude-plugin/plugin.json"), CLAUDE_PLUGIN_JSON)?;
    write_file(&root.join("hooks/hooks.json"), CLAUDE_HOOKS_JSON)?;
    write_file(&root.join("dist/index.js"), CLAUDE_INDEX_JS)?;
    write_file(&root.join("dist/server.js"), CLAUDE_SERVER_JS)?;
    Ok(())
}

fn setup_openclaw(paths: &InstallPaths, force: bool) -> Result<(), Box<dyn std::error::Error>> {
    let host_version = command_output("openclaw", &["--version"])?;
    let host_version = parse_numeric_version(&host_version).ok_or_else(|| {
        other_error(format!(
            "Could not parse the OpenClaw version from {:?}",
            host_version.trim()
        ))
    })?;
    if host_version < (2026, 7, 1) || host_version.0 >= 2027 {
        return Err(other_error(format!(
            "OpenClaw {}.{}.{} is outside Kendr's audited range (>=2026.7.1, <2027). Update OpenClaw before setup.",
            host_version.0, host_version.1, host_version.2
        )));
    }

    let root = paths.adapter_root.join("openclaw");
    prepare_managed_directory(&root, force)?;
    write_file(&root.join("package.json"), OPENCLAW_PACKAGE_JSON)?;
    write_file(&root.join("openclaw.plugin.json"), OPENCLAW_PLUGIN_JSON)?;
    write_file(&root.join("dist/index.js"), OPENCLAW_INDEX_JS)?;

    let existing_slot = command_output(
        "openclaw",
        &["config", "get", "plugins.slots.contextEngine"],
    )
    .unwrap_or_default();
    let existing_slot = existing_slot.trim().trim_matches('"');
    if !force
        && !existing_slot.is_empty()
        && existing_slot != "null"
        && existing_slot != "legacy"
        && existing_slot != "kendr-optimizer"
    {
        return Err(other_error(format!(
            "OpenClaw already uses context engine {existing_slot:?}. Re-run with --force to replace that exclusive slot."
        )));
    }

    let installed_version = command_output(
        "openclaw",
        &["config", "get", "plugins.installs.kendr-optimizer.version"],
    )
    .unwrap_or_default();
    let installed_version = installed_version.trim().trim_matches('"');
    if !force
        && installed_version == env!("CARGO_PKG_VERSION")
        && existing_slot == "kendr-optimizer"
    {
        return Ok(());
    }
    let should_install = if installed_version.is_empty() || installed_version == "null" {
        true
    } else if force {
        run_checked("openclaw", &["plugins", "uninstall", "kendr-optimizer"])?;
        true
    } else if installed_version == env!("CARGO_PKG_VERSION") {
        false
    } else {
        return Err(other_error(format!(
            "OpenClaw has Kendr adapter {installed_version}; re-run with --force to replace it with {}.",
            env!("CARGO_PKG_VERSION")
        )));
    };
    if should_install {
        run_checked_os(
            "openclaw",
            &[
                OsString::from("plugins"),
                OsString::from("install"),
                root.into_os_string(),
            ],
        )?;
    }
    run_checked(
        "openclaw",
        &[
            "config",
            "set",
            "plugins.slots.contextEngine",
            "kendr-optimizer",
        ],
    )?;
    run_checked(
        "openclaw",
        &[
            "config",
            "set",
            "plugins.entries.kendr-optimizer.enabled",
            "true",
        ],
    )?;
    Ok(())
}

fn setup_hermes(paths: &InstallPaths, force: bool) -> Result<(), Box<dyn std::error::Error>> {
    let root = paths
        .home
        .join(".hermes")
        .join("plugins")
        .join("kendr-optimizer");
    prepare_managed_directory(&root, force)?;
    write_file(&root.join("__init__.py"), HERMES_INIT_PY)?;
    write_file(&root.join("client.py"), HERMES_CLIENT_PY)?;
    write_file(&root.join("codec.py"), HERMES_CODEC_PY)?;
    write_file(&root.join("config.py"), HERMES_CONFIG_PY)?;
    write_file(&root.join("plugin.yaml"), HERMES_PLUGIN_YAML)?;
    run_checked("hermes", &["plugins", "enable", "kendr-optimizer"])?;
    Ok(())
}

fn spawn_service_if_needed() -> Result<Option<Child>, Box<dyn std::error::Error>> {
    if port_is_open(CORE_ADDRESS) {
        return Ok(None);
    }
    let executable = env::current_exe()?;
    let mut child = Command::new(executable)
        .args(["serve", "--bind", "127.0.0.1:7331"])
        .env("RUST_LOG", "warn")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()?;
    if let Err(error) = wait_for_port(CORE_ADDRESS, Duration::from_secs(5)) {
        let _ = child.kill();
        let _ = child.wait();
        return Err(error);
    }
    Ok(Some(child))
}

fn spawn_claude_bridge_if_needed(
    paths: &InstallPaths,
) -> Result<Option<Child>, Box<dyn std::error::Error>> {
    if port_is_open(CLAUDE_BRIDGE_ADDRESS) {
        return Ok(None);
    }
    let mut command = resolved_command("node")?;
    let mut child = command
        .arg(paths.adapter_root.join("claude-code/dist/server.js"))
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()?;
    if let Err(error) = wait_for_port(CLAUDE_BRIDGE_ADDRESS, Duration::from_secs(5)) {
        let _ = child.kill();
        let _ = child.wait();
        return Err(error);
    }
    Ok(Some(child))
}

fn wait_for_port(address: SocketAddr, timeout: Duration) -> Result<(), Box<dyn std::error::Error>> {
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        if port_is_open(address) {
            return Ok(());
        }
        thread::sleep(Duration::from_millis(50));
    }
    Err(other_error(format!(
        "local service did not become ready at {address}"
    )))
}

fn port_is_open(address: SocketAddr) -> bool {
    TcpStream::connect_timeout(&address, Duration::from_millis(50)).is_ok()
}

fn stop_child(child: &mut Option<Child>) {
    if let Some(mut child) = child.take() {
        let _ = child.kill();
        let _ = child.wait();
    }
}

fn prepare_managed_directory(
    directory: &Path,
    force: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    let marker = directory.join(".kendr-managed");
    if directory.exists() && !marker.is_file() && directory.read_dir()?.next().is_some() && !force {
        return Err(other_error(format!(
            "Refusing to replace unmanaged directory {}. Re-run with --force only if Kendr should own it.",
            directory.display()
        )));
    }
    fs::create_dir_all(directory)?;
    write_file(&marker, &format!("{MANAGED_MARKER}.\n"))?;
    Ok(())
}

fn write_marked_file(
    path: &Path,
    contents: &str,
    comment: &str,
    force: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    if path.is_file() {
        let existing = fs::read_to_string(path)?;
        if !existing.contains(MANAGED_MARKER) && !force {
            return Err(other_error(format!(
                "Refusing to replace unmanaged file {}. Re-run with --force only if Kendr should own it.",
                path.display()
            )));
        }
    }
    write_file(path, &format!("{comment} {MANAGED_MARKER}.\n{contents}"))?;
    Ok(())
}

fn write_file(path: &Path, contents: &str) -> Result<(), Box<dyn std::error::Error>> {
    let parent = path
        .parent()
        .ok_or_else(|| other_error(format!("path has no parent: {}", path.display())))?;
    fs::create_dir_all(parent)?;
    fs::write(path, contents.as_bytes())?;
    Ok(())
}

fn run_checked(program: &str, arguments: &[&str]) -> Result<(), Box<dyn std::error::Error>> {
    let arguments: Vec<OsString> = arguments.iter().map(OsString::from).collect();
    run_checked_os(program, &arguments)
}

fn run_checked_os(program: &str, arguments: &[OsString]) -> Result<(), Box<dyn std::error::Error>> {
    let output = resolved_command(program)?.args(arguments).output()?;
    if output.status.success() {
        return Ok(());
    }
    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_owned();
    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_owned();
    let detail = if !stderr.is_empty() { stderr } else { stdout };
    Err(other_error(format!(
        "{program} configuration failed with status {}{}",
        output.status,
        if detail.is_empty() {
            String::new()
        } else {
            format!(": {detail}")
        }
    )))
}

fn command_output(program: &str, arguments: &[&str]) -> io::Result<String> {
    let output = resolved_command(program)?.args(arguments).output()?;
    if !output.status.success() {
        return Ok(String::new());
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

fn command_exists(program: &str) -> bool {
    resolve_command(program).is_some()
}

fn parse_numeric_version(value: &str) -> Option<(u32, u32, u32)> {
    value.split_whitespace().find_map(|candidate| {
        let candidate = candidate.trim_start_matches('v');
        let mut components = candidate.split('.');
        let major = components.next()?.parse().ok()?;
        let minor = components.next()?.parse().ok()?;
        let patch = components
            .next()?
            .split(|character: char| !character.is_ascii_digit())
            .next()?
            .parse()
            .ok()?;
        Some((major, minor, patch))
    })
}

fn resolve_command(program: &str) -> Option<PathBuf> {
    let path = Path::new(program);
    if path.components().count() > 1 {
        return path.is_file().then(|| path.to_path_buf());
    }
    let search_path = env::var_os("PATH")?;
    let extensions: Vec<OsString> = if cfg!(windows) {
        env::var_os("PATHEXT")
            .unwrap_or_else(|| OsString::from(".COM;.EXE;.BAT;.CMD"))
            .to_string_lossy()
            .split(';')
            .filter(|value| !value.is_empty())
            .map(OsString::from)
            .collect()
    } else {
        vec![OsString::new()]
    };
    env::split_paths(&search_path).find_map(|directory| {
        extensions.iter().find_map(|extension| {
            let mut name = OsString::from(program);
            name.push(extension);
            let candidate = directory.join(name);
            candidate.is_file().then_some(candidate)
        })
    })
}

fn resolved_command(program: &str) -> io::Result<Command> {
    let resolved = resolve_command(program).ok_or_else(|| {
        io::Error::new(io::ErrorKind::NotFound, format!("{program} is not on PATH"))
    })?;

    #[cfg(windows)]
    {
        let extension = resolved
            .extension()
            .and_then(|value| value.to_str())
            .unwrap_or_default();
        if extension.eq_ignore_ascii_case("cmd") || extension.eq_ignore_ascii_case("bat") {
            let mut command =
                Command::new(env::var_os("COMSPEC").unwrap_or_else(|| OsString::from("cmd.exe")));
            command.args([
                OsStr::new("/d"),
                OsStr::new("/s"),
                OsStr::new("/c"),
                OsStr::new("call"),
            ]);
            command.arg(resolved);
            return Ok(command);
        }
        if extension.eq_ignore_ascii_case("ps1") {
            let mut command = Command::new("powershell.exe");
            command.args([
                "-NoLogo",
                "-NoProfile",
                "-NonInteractive",
                "-ExecutionPolicy",
                "Bypass",
                "-File",
            ]);
            command.arg(resolved);
            return Ok(command);
        }
    }

    Ok(Command::new(resolved))
}

struct InstallPaths {
    home: PathBuf,
    opencode_config: PathBuf,
    adapter_root: PathBuf,
}

impl InstallPaths {
    fn discover() -> Result<Self, Box<dyn std::error::Error>> {
        let home = env::var_os("HOME")
            .or_else(|| env::var_os("USERPROFILE"))
            .map(PathBuf::from)
            .ok_or_else(|| other_error("Could not determine the user home directory"))?;
        let opencode_config = env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| home.join(".config"))
            .join("opencode");
        let adapter_root = if let Some(root) = env::var_os("KENDR_HOME") {
            PathBuf::from(root).join("adapters")
        } else if cfg!(windows) {
            env::var_os("LOCALAPPDATA")
                .map(PathBuf::from)
                .unwrap_or_else(|| home.join("AppData/Local"))
                .join("Kendr/adapters")
        } else {
            env::var_os("XDG_DATA_HOME")
                .map(PathBuf::from)
                .unwrap_or_else(|| home.join(".local/share"))
                .join("kendr/adapters")
        };
        Ok(Self {
            home,
            opencode_config,
            adapter_root,
        })
    }
}

fn other_error(message: impl Into<String>) -> Box<dyn std::error::Error> {
    Box::new(io::Error::other(message.into()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temporary_directory(name: &str) -> PathBuf {
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        env::temp_dir().join(format!(
            "kendr-optimizer-{name}-{}-{nonce}",
            std::process::id(),
        ))
    }

    #[test]
    fn managed_file_does_not_replace_unowned_content_without_force() {
        let root = temporary_directory("managed-file");
        let path = root.join("plugin.js");
        fs::create_dir_all(&root).unwrap();
        fs::write(&path, "user content\n").unwrap();

        let error = write_marked_file(&path, "plugin\n", "//", false).unwrap_err();
        assert!(error.to_string().contains("unmanaged file"));
        assert_eq!(fs::read_to_string(&path).unwrap(), "user content\n");

        write_marked_file(&path, "plugin\n", "//", true).unwrap();
        assert!(fs::read_to_string(&path).unwrap().contains(MANAGED_MARKER));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn managed_directory_requires_marker_before_reuse() {
        let root = temporary_directory("managed-directory");
        fs::create_dir_all(&root).unwrap();
        fs::write(root.join("existing"), "user content\n").unwrap();

        let error = prepare_managed_directory(&root, false).unwrap_err();
        assert!(error.to_string().contains("unmanaged directory"));
        prepare_managed_directory(&root, true).unwrap();
        assert!(root.join(".kendr-managed").is_file());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn support_text_names_automatic_and_unsupported_hosts() {
        let text = support_text();
        assert!(text.contains("opencode"));
        assert!(text.contains("claude-code"));
        assert!(text.contains("OpenAI's coding CLI has no supported"));
    }

    #[test]
    fn parses_plain_and_decorated_host_versions() {
        assert_eq!(parse_numeric_version("2026.7.2\n"), Some((2026, 7, 2)));
        assert_eq!(
            parse_numeric_version("OpenClaw v2026.7.1-2 ready"),
            Some((2026, 7, 1))
        );
        assert_eq!(parse_numeric_version("unknown"), None);
    }
}
