use std::collections::BTreeMap;
use std::ffi::OsStr;
use std::fs;
use std::io::{self, Read, Write};
use std::net::{Ipv4Addr, SocketAddr, TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use kendr_optimizer_contracts::OptimizeRequest;
use semver::Version;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use tempfile::TempDir;

const REPOSITORY_PATH: &str = "/repos/Kendr-AI/Kendr-Optimizer";
const RELEASES_PATH: &str = "/repos/Kendr-AI/Kendr-Optimizer/releases?per_page=100";

#[derive(Clone, Debug)]
struct RecordedRequest {
    method: String,
    target: String,
    headers: BTreeMap<String, String>,
}

struct MockResponse {
    status: u16,
    headers: Vec<(String, String)>,
    body: Vec<u8>,
}

impl MockResponse {
    fn json(body: String) -> Self {
        Self {
            status: 200,
            headers: vec![("Content-Type".to_owned(), "application/json".to_owned())],
            body: body.into_bytes(),
        }
    }

    fn not_modified() -> Self {
        Self {
            status: 304,
            headers: Vec::new(),
            body: Vec::new(),
        }
    }

    fn not_found() -> Self {
        Self {
            status: 404,
            headers: Vec::new(),
            body: b"not found".to_vec(),
        }
    }

    fn with_header(mut self, name: &str, value: &str) -> Self {
        self.headers.push((name.to_owned(), value.to_owned()));
        self
    }
}

struct MockServer {
    address: SocketAddr,
    requests: Arc<Mutex<Vec<RecordedRequest>>>,
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl MockServer {
    fn start(handler: impl Fn(&RecordedRequest) -> MockResponse + Send + Sync + 'static) -> Self {
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0))
            .expect("numeric-loopback mock server should bind");
        listener
            .set_nonblocking(true)
            .expect("mock listener should become nonblocking");
        let address = listener
            .local_addr()
            .expect("mock server should have an address");
        let requests = Arc::new(Mutex::new(Vec::new()));
        let stop = Arc::new(AtomicBool::new(false));
        let recorded = Arc::clone(&requests);
        let should_stop = Arc::clone(&stop);
        let handler = Arc::new(handler);

        let thread = thread::spawn(move || {
            while !should_stop.load(Ordering::Acquire) {
                match listener.accept() {
                    Ok((mut stream, _)) => {
                        stream
                            .set_nonblocking(false)
                            .expect("accepted mock connection should become blocking");
                        let Some(request) = read_request(&mut stream)
                            .expect("mock server should read the HTTP request")
                        else {
                            continue;
                        };
                        recorded
                            .lock()
                            .expect("request log should not be poisoned")
                            .push(request.clone());
                        let response = handler(&request);
                        write_response(&mut stream, response)
                            .expect("mock server should write the HTTP response");
                    }
                    Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                        thread::sleep(Duration::from_millis(2));
                    }
                    Err(error) => panic!("mock server failed to accept a connection: {error}"),
                }
            }
        });

        Self {
            address,
            requests,
            stop,
            thread: Some(thread),
        }
    }

    fn url(&self) -> String {
        format!("http://{}", self.address)
    }

    fn requests(&self) -> Vec<RecordedRequest> {
        self.requests
            .lock()
            .expect("request log should not be poisoned")
            .clone()
    }
}

impl Drop for MockServer {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        let _ = TcpStream::connect(self.address);
        if let Some(thread) = self.thread.take() {
            let joined = thread.join();
            if !std::thread::panicking() {
                joined.expect("mock server thread should stop cleanly");
            }
        }
    }
}

#[test]
fn check_json_reports_update_available_without_requesting_assets() {
    let latest = newer_version();
    let server = release_server(release_json(&latest, true), None, false);
    let cache = isolated_cache();

    let output = run_cli(&server, cache.path(), ["update", "--check", "--json"]);
    let report = successful_json(&output);

    assert_eq!(report["schema_version"], "kendr.update/v1");
    assert_eq!(report["status"], "update_available");
    assert_eq!(report["current_version"], env!("CARGO_PKG_VERSION"));
    assert_eq!(report["latest_version"], latest);
    assert_eq!(report["channel"], "preview");
    assert_eq!(report["prerelease"], false);
    assert!(report.get("executable").is_none());
    assert_metadata_only(&server, 1);
}

#[test]
fn check_json_reports_up_to_date_without_requesting_assets() {
    let current = env!("CARGO_PKG_VERSION");
    let server = release_server(release_json(current, true), None, false);
    let cache = isolated_cache();

    let output = run_cli(&server, cache.path(), ["update", "--check", "--json"]);
    let report = successful_json(&output);

    assert_eq!(report["schema_version"], "kendr.update/v1");
    assert_eq!(report["status"], "up_to_date");
    assert_eq!(report["current_version"], current);
    assert_eq!(report["latest_version"], current);
    assert_eq!(report["channel"], "preview");
    assert_metadata_only(&server, 1);
}

#[test]
fn check_json_reuses_cached_release_on_etag_304() {
    let latest = newer_version();
    let etag = "\"kendr-update-test-v1\"";
    let server = release_server(release_json(&latest, true), Some(etag), true);
    let cache = isolated_cache();

    let first = run_cli(&server, cache.path(), ["update", "--check", "--json"]);
    assert_eq!(successful_json(&first)["status"], "update_available");
    assert!(cache.path().join("update.json").is_file());

    let second = run_cli(&server, cache.path(), ["update", "--check", "--json"]);
    let second_report = successful_json(&second);
    assert_eq!(second_report["status"], "update_available");
    assert_eq!(second_report["latest_version"], latest);

    let requests = server.requests();
    assert_eq!(requests.len(), 4, "unexpected requests: {requests:#?}");
    assert_eq!(requests[0].target, REPOSITORY_PATH);
    assert_eq!(requests[1].target, RELEASES_PATH);
    assert_eq!(requests[2].target, REPOSITORY_PATH);
    assert_eq!(requests[3].target, RELEASES_PATH);
    assert_eq!(
        requests[3].headers.get("if-none-match").map(String::as_str),
        Some(etag)
    );
    assert_no_asset_requests(&requests);
}

#[test]
fn check_rejects_mutable_release_without_requesting_assets() {
    let latest = newer_version();
    let server = release_server(release_json(&latest, false), None, false);
    let cache = isolated_cache();

    let output = run_cli(&server, cache.path(), ["update", "--check", "--json"]);

    assert!(!output.status.success(), "mutable release was accepted");
    assert!(
        output.stdout.is_empty(),
        "failed check wrote machine output"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("mutable") && stderr.contains("immutability"),
        "unexpected error: {stderr}"
    );
    assert_metadata_only(&server, 1);
}

#[test]
fn existing_machine_output_commands_do_not_contact_updater() {
    let server = MockServer::start(|_| MockResponse::not_found());
    let cache = isolated_cache();
    let examples = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples");
    let request_path = examples.join("request.json");
    let observation_path = examples.join("observation-unpaired.json");
    let recovery_path = write_recovery_fixture(cache.path(), &request_path);

    let commands = vec![
        vec!["--version".into()],
        vec!["--help".into()],
        vec!["engines".into(), "--compact".into()],
        vec![
            "analyze".into(),
            "--input".into(),
            request_path.as_os_str().to_owned(),
            "--compact".into(),
        ],
        vec![
            "optimize".into(),
            "--input".into(),
            request_path.as_os_str().to_owned(),
            "--compact".into(),
        ],
        vec![
            "restore".into(),
            "--input".into(),
            recovery_path.as_os_str().to_owned(),
            "--compact".into(),
        ],
        vec![
            "observe".into(),
            "--input".into(),
            observation_path.as_os_str().to_owned(),
            "--compact".into(),
        ],
        vec!["setup".into(), "--list".into()],
    ];

    for arguments in commands {
        let output = run_cli(&server, cache.path(), &arguments);
        assert!(
            output.status.success(),
            "kendr-opt {arguments:?} failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(
            !output.stdout.is_empty(),
            "kendr-opt {arguments:?} produced no output"
        );
    }

    thread::sleep(Duration::from_millis(25));
    assert!(
        server.requests().is_empty(),
        "machine-output command contacted the update server"
    );
    assert!(
        !cache.path().join("update.json").exists(),
        "machine-output command created an update cache"
    );
}

fn release_server(release_body: String, etag: Option<&str>, honor_conditional: bool) -> MockServer {
    let etag = etag.map(str::to_owned);
    MockServer::start(move |request| match request.target.as_str() {
        REPOSITORY_PATH => MockResponse::json(repository_json()),
        RELEASES_PATH => {
            if honor_conditional
                && etag.as_deref().is_some()
                && request.headers.get("if-none-match") == etag.as_ref()
            {
                MockResponse::not_modified()
            } else {
                let response = MockResponse::json(release_body.clone());
                if let Some(etag) = etag.as_deref() {
                    response.with_header("ETag", etag)
                } else {
                    response
                }
            }
        }
        _ => MockResponse::not_found(),
    })
}

fn repository_json() -> String {
    json!({
        "id": 1_328_565_025_u64,
        "full_name": "Kendr-AI/Kendr-Optimizer",
        "private": false,
        "archived": false,
        "disabled": false
    })
    .to_string()
}

fn release_json(version: &str, immutable: bool) -> String {
    let version = Version::parse(version).expect("test release version should be valid semver");
    json!([{
        "id": 9001,
        "tag_name": format!("v{version}"),
        "html_url": format!(
            "https://github.com/Kendr-AI/Kendr-Optimizer/releases/tag/v{version}"
        ),
        "draft": false,
        "prerelease": !version.pre.is_empty(),
        "immutable": immutable,
        "published_at": "2026-08-16T00:00:00Z",
        "assets": [
            {
                "id": 9002,
                "name": archive_name(),
                "size": 1024,
                "state": "uploaded",
                "digest": format!("sha256:{}", "a".repeat(64))
            },
            {
                "id": 9003,
                "name": "SHA256SUMS",
                "size": 128,
                "state": "uploaded",
                "digest": format!("sha256:{}", "b".repeat(64))
            }
        ]
    }])
    .to_string()
}

fn newer_version() -> String {
    let current =
        Version::parse(env!("CARGO_PKG_VERSION")).expect("package version should be valid semver");
    format!("{}.0.0", current.major + 1)
}

fn archive_name() -> String {
    let target = match (std::env::consts::OS, std::env::consts::ARCH) {
        ("linux", "x86_64") => "x86_64-unknown-linux-musl",
        ("linux", "aarch64") => "aarch64-unknown-linux-musl",
        ("macos", "x86_64") => "x86_64-apple-darwin",
        ("macos", "aarch64") => "aarch64-apple-darwin",
        ("windows", "x86_64") => "x86_64-pc-windows-msvc",
        (os, architecture) => panic!("unsupported updater test target: {os}/{architecture}"),
    };
    let suffix = if target.ends_with("windows-msvc") {
        ".zip"
    } else {
        ".tar.gz"
    };
    format!("kendr-opt-{target}{suffix}")
}

fn isolated_cache() -> TempDir {
    tempfile::Builder::new()
        .prefix("kendr-update-integration-")
        .tempdir()
        .expect("isolated update cache should be created")
}

fn run_cli<I, S>(server: &MockServer, cache: &Path, arguments: I) -> Output
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    Command::new(env!("CARGO_BIN_EXE_kendr-opt"))
        .args(arguments)
        .env("KENDR_UPDATE_API_URL", server.url())
        .env("KENDR_ALLOW_INSECURE", "1")
        .env("KENDR_UPDATE_CACHE_DIR", cache)
        .env("NO_PROXY", "127.0.0.1")
        .env("no_proxy", "127.0.0.1")
        .output()
        .expect("kendr-opt should execute")
}

fn successful_json(output: &Output) -> Value {
    assert!(
        output.status.success(),
        "update check failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(output.stderr.is_empty(), "successful check wrote to stderr");
    serde_json::from_slice(&output.stdout).expect("update check should emit one JSON object")
}

fn assert_metadata_only(server: &MockServer, release_requests: usize) {
    let requests = server.requests();
    assert_eq!(
        requests.len(),
        release_requests * 2,
        "unexpected requests: {requests:#?}"
    );
    for pair in requests.chunks_exact(2) {
        assert_eq!(pair[0].method, "GET");
        assert_eq!(pair[0].target, REPOSITORY_PATH);
        assert_eq!(pair[1].method, "GET");
        assert_eq!(pair[1].target, RELEASES_PATH);
    }
    assert_no_asset_requests(&requests);
}

fn assert_no_asset_requests(requests: &[RecordedRequest]) {
    assert!(
        requests
            .iter()
            .all(|request| !request.target.contains("/releases/assets/")),
        "check-only command requested a release asset: {requests:#?}"
    );
}

fn write_recovery_fixture(directory: &Path, request_path: &Path) -> PathBuf {
    let request: OptimizeRequest = serde_json::from_slice(
        &fs::read(request_path).expect("example optimize request should be readable"),
    )
    .expect("example optimize request should be valid");
    let serialized = serde_json::to_vec(&request.content)
        .expect("example content should serialize deterministically");
    let digest = Sha256::digest(serialized)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    let recovery = json!({
        "request_id": request.request_id,
        "original_sha256": digest,
        "records": [{
            "engine_id": "integration-test",
            "scope": "envelope",
            "marker": "not-used-by-restore",
            "original": request.content
        }]
    });
    let path = directory.join("recovery.json");
    fs::write(
        &path,
        serde_json::to_vec(&recovery).expect("recovery fixture should serialize"),
    )
    .expect("recovery fixture should be written");
    path
}

fn read_request(stream: &mut TcpStream) -> io::Result<Option<RecordedRequest>> {
    stream.set_read_timeout(Some(Duration::from_secs(2)))?;
    let mut bytes = Vec::new();
    let mut buffer = [0_u8; 1024];
    while !bytes.windows(4).any(|window| window == b"\r\n\r\n") {
        let read = stream.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        bytes.extend_from_slice(&buffer[..read]);
        if bytes.len() > 64 * 1024 {
            return Err(io::Error::other("mock request headers were too large"));
        }
    }
    if bytes.is_empty() {
        return Ok(None);
    }
    let text = std::str::from_utf8(&bytes)
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
    let mut lines = text.split("\r\n");
    let request_line = lines
        .next()
        .ok_or_else(|| io::Error::other("mock request had no request line"))?;
    let mut parts = request_line.split_whitespace();
    let method = parts
        .next()
        .ok_or_else(|| io::Error::other("mock request had no method"))?
        .to_owned();
    let target = parts
        .next()
        .ok_or_else(|| io::Error::other("mock request had no target"))?
        .to_owned();
    let mut headers = BTreeMap::new();
    for line in lines.take_while(|line| !line.is_empty()) {
        let Some((name, value)) = line.split_once(':') else {
            continue;
        };
        headers.insert(name.trim().to_ascii_lowercase(), value.trim().to_owned());
    }
    Ok(Some(RecordedRequest {
        method,
        target,
        headers,
    }))
}

fn write_response(stream: &mut TcpStream, response: MockResponse) -> io::Result<()> {
    let reason = match response.status {
        200 => "OK",
        304 => "Not Modified",
        404 => "Not Found",
        status => {
            return Err(io::Error::other(format!(
                "unsupported mock status {status}"
            )));
        }
    };
    write!(stream, "HTTP/1.1 {} {}\r\n", response.status, reason)?;
    for (name, value) in response.headers {
        write!(stream, "{name}: {value}\r\n")?;
    }
    write!(
        stream,
        "Content-Length: {}\r\nConnection: close\r\n\r\n",
        response.body.len()
    )?;
    stream.write_all(&response.body)?;
    stream.flush()
}
