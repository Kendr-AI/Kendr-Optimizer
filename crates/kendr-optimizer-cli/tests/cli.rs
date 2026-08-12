use std::collections::HashSet;
use std::process::{Command, Output};

fn run_cli(arguments: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_kendr-opt"))
        .args(arguments)
        .output()
        .expect("kendr-opt should execute")
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
