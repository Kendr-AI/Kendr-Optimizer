use std::collections::HashSet;
use std::fs;
use std::net::{IpAddr, Ipv4Addr, SocketAddr, TcpStream};
use std::path::PathBuf;
use std::process::{Command, Output, Stdio};
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

fn run_cli(arguments: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_kendr-opt"))
        .args(arguments)
        .output()
        .expect("kendr-opt should execute")
}

#[test]
fn no_arguments_prints_plain_long_help_when_output_is_not_a_terminal() {
    let output = run_cli(&[]);
    assert!(output.status.success(), "no-argument help failed");
    assert!(output.stderr.is_empty(), "no-argument help wrote stderr");
    let stdout = String::from_utf8(output.stdout).expect("help should be UTF-8");
    assert!(stdout.contains("Usage:"));
    assert!(stdout.contains("dashboard"));
    assert!(stdout.contains("tui"));
    assert!(!stdout.contains('\u{1b}'), "piped help contained ANSI");
}

#[test]
fn explicit_dashboard_requires_an_interactive_terminal() {
    let output = run_cli(&["dashboard"]);
    assert!(!output.status.success(), "piped dashboard unexpectedly ran");
    assert!(output.stdout.is_empty(), "failed dashboard wrote stdout");
    let stderr = String::from_utf8(output.stderr).expect("error should be UTF-8");
    assert!(stderr.contains("requires interactive stdin and stdout"));
    assert!(!stderr.contains('\u{1b}'), "piped error contained ANSI");
}

#[test]
fn no_color_presence_wins_over_forced_color() {
    let output = Command::new(env!("CARGO_BIN_EXE_kendr-opt"))
        .arg("--help")
        .env("CLICOLOR_FORCE", "1")
        .env("NO_COLOR", "")
        .env_remove("CI")
        .env_remove("GITHUB_ACTIONS")
        .env_remove("TERM")
        .output()
        .expect("kendr-opt --help should execute");
    assert!(output.status.success());
    assert!(!output.stdout.contains(&0x1b));
    assert!(!output.stderr.contains(&0x1b));
}

#[test]
fn version_output_matches_package_metadata() {
    let output = run_cli(&["--version"]);
    assert!(output.status.success(), "kendr-opt --version failed");
    assert_eq!(
        String::from_utf8(output.stdout).expect("version output should be UTF-8"),
        format!("kendr-opt {}\n", env!("CARGO_PKG_VERSION"))
    );
    assert!(output.stderr.is_empty(), "version command wrote to stderr");
}

#[test]
fn engines_smoke_is_nonempty_deterministic_and_versioned() {
    let first = run_cli(&["engines", "--compact"]);
    assert!(first.status.success(), "kendr-opt engines failed");
    assert!(first.stderr.is_empty(), "engines command wrote to stderr");

    let second = run_cli(&["engines", "--compact"]);
    assert!(
        second.status.success(),
        "second kendr-opt engines run failed"
    );
    assert_eq!(
        first.stdout, second.stdout,
        "engine output is not deterministic"
    );

    let engines: serde_json::Value =
        serde_json::from_slice(&first.stdout).expect("engine output should be JSON");
    let engines = engines
        .as_array()
        .expect("engine output should be a JSON array");
    assert!(!engines.is_empty(), "engine output should not be empty");

    let mut identifiers = HashSet::new();
    for engine in engines {
        let engine = engine
            .as_object()
            .expect("engine entry should be an object");
        let identifier = engine["id"].as_str().expect("engine id should be a string");
        assert!(
            identifiers.insert(identifier),
            "duplicate engine id: {identifier}"
        );
        assert_eq!(
            engine["version"].as_str(),
            Some(env!("CARGO_PKG_VERSION")),
            "engine {identifier} does not match the CLI package version"
        );
        assert!(
            engine["summary"]
                .as_str()
                .is_some_and(|value| !value.is_empty()),
            "engine {identifier} has no summary"
        );
        assert!(
            engine["risk"].is_string(),
            "engine {identifier} has no risk"
        );
        assert!(
            engine["reversible"].is_boolean(),
            "engine {identifier} has no reversible flag"
        );
        assert!(
            engine["cache_safe"].is_boolean(),
            "engine {identifier} has no cache-safe flag"
        );
    }
}

#[test]
fn setup_list_is_read_only_and_names_supported_harnesses() {
    let output = run_cli(&["setup", "--list"]);
    assert!(output.status.success(), "kendr-opt setup --list failed");
    let stdout = String::from_utf8(output.stdout).expect("setup list should be UTF-8");
    for name in ["opencode", "claude-code", "pi", "openclaw", "hermes"] {
        assert!(stdout.contains(name), "setup list omitted {name}");
    }
    assert!(stdout.contains("OpenAI's coding CLI has no supported"));
    assert!(
        !stdout.contains('\u{1b}'),
        "piped setup list contained ANSI"
    );
}

#[test]
fn serve_writes_plain_logs_to_stderr() {
    let root = temporary_directory("serve-log-streams");
    fs::create_dir_all(&root).unwrap();
    let stdout_path = root.join("stdout.log");
    let stderr_path = root.join("stderr.log");
    let stdout = fs::File::create(&stdout_path).unwrap();
    let stderr = fs::File::create(&stderr_path).unwrap();
    let mut child = Command::new(env!("CARGO_BIN_EXE_kendr-opt"))
        .args(["serve", "--bind", "127.0.0.1:0"])
        .env("RUST_LOG", "info")
        .stdin(Stdio::null())
        .stdout(Stdio::from(stdout))
        .stderr(Stdio::from(stderr))
        .spawn()
        .expect("kendr-opt serve should start");

    let mut ready = false;
    for _ in 0..80 {
        thread::sleep(Duration::from_millis(25));
        let log = fs::read_to_string(&stderr_path).unwrap_or_default();
        if log.contains("transform-only service listening") {
            ready = true;
            break;
        }
        if let Some(status) = child.try_wait().unwrap() {
            panic!("kendr-opt serve exited before listening: {status}; {log}");
        }
    }
    let _ = child.kill();
    let _ = child.wait();

    let stdout = fs::read(&stdout_path).unwrap();
    let stderr = fs::read_to_string(&stderr_path).unwrap();
    assert!(ready, "serve did not emit its listening log: {stderr}");
    assert!(stdout.is_empty(), "serve logs contaminated stdout");
    assert!(
        !stderr.contains('\u{1b}'),
        "non-TTY serve log contained ANSI"
    );
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn setup_opencode_installs_owned_bundle_without_touching_provider_config() {
    let root = temporary_directory("opencode-setup");
    let executable_directory = root.join("bin");
    let config_directory = root.join("config");
    fs::create_dir_all(&executable_directory).unwrap();
    let fake_opencode = executable_directory.join(if cfg!(windows) {
        "opencode.cmd"
    } else {
        "opencode"
    });
    fs::write(&fake_opencode, "").unwrap();
    let provider_config = config_directory.join("opencode/opencode.json");
    fs::create_dir_all(provider_config.parent().unwrap()).unwrap();
    fs::write(&provider_config, "{\"provider\":\"unchanged\"}\n").unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_kendr-opt"))
        .args(["setup", "opencode"])
        .env("PATH", &executable_directory)
        .env("PATHEXT", ".COM;.EXE;.BAT;.CMD")
        .env("HOME", &root)
        .env("USERPROFILE", &root)
        .env("XDG_CONFIG_HOME", &config_directory)
        .output()
        .expect("kendr-opt setup should execute");
    assert!(
        output.status.success(),
        "setup failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let plugin = config_directory.join("opencode/plugins/kendr-optimizer.js");
    let plugin_text = fs::read_to_string(plugin).unwrap();
    assert!(plugin_text.contains("Installed by Kendr Optimizer"));
    assert!(plugin_text.contains("KendrOptimizerPlugin"));
    assert_eq!(
        fs::read_to_string(provider_config).unwrap(),
        "{\"provider\":\"unchanged\"}\n"
    );
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn run_opencode_manages_the_local_service_around_the_host_process() {
    let address = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 7331);
    if TcpStream::connect_timeout(&address, std::time::Duration::from_millis(50)).is_ok() {
        return;
    }

    let root = temporary_directory("opencode-run");
    let executable_directory = root.join("bin");
    let config_directory = root.join("config");
    fs::create_dir_all(&executable_directory).unwrap();
    let fake_opencode = executable_directory.join(if cfg!(windows) {
        "opencode.exe"
    } else {
        "opencode"
    });
    fs::copy(env!("CARGO_BIN_EXE_kendr-opt"), &fake_opencode).unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_kendr-opt"))
        .args(["run", "opencode", "--", "--version"])
        .env("PATH", &executable_directory)
        .env("PATHEXT", ".COM;.EXE;.BAT;.CMD")
        .env("HOME", &root)
        .env("USERPROFILE", &root)
        .env("XDG_CONFIG_HOME", &config_directory)
        .output()
        .expect("kendr-opt run should execute");
    assert!(
        output.status.success(),
        "run failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(String::from_utf8_lossy(&output.stdout).contains(env!("CARGO_PKG_VERSION")));
    std::thread::sleep(std::time::Duration::from_millis(100));
    assert!(
        TcpStream::connect_timeout(&address, std::time::Duration::from_millis(50)).is_err(),
        "run left its optimizer process listening"
    );
    fs::remove_dir_all(root).unwrap();
}

fn temporary_directory(name: &str) -> PathBuf {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    std::env::temp_dir().join(format!(
        "kendr-optimizer-cli-{name}-{}-{nonce}",
        std::process::id()
    ))
}
