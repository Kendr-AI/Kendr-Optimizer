use std::collections::{BTreeMap, HashSet};
use std::env;
use std::error::Error;
use std::fmt;
use std::fs::{self, File, OpenOptions};
use std::io::{self, IsTerminal, Read, Write};
#[cfg(feature = "update-test-server")]
use std::net::IpAddr;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus, Stdio};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use clap::ValueEnum;
use flate2::read::GzDecoder;
use fs2::FileExt;
use reqwest::header::{ACCEPT, ETAG, IF_NONE_MATCH};
use reqwest::{Client, StatusCode, Url};
use semver::Version;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tempfile::{NamedTempFile, TempDir};

const REPOSITORY: &str = "Kendr-AI/Kendr-Optimizer";
const REPOSITORY_ID: u64 = 1_328_565_025;
const DEFAULT_API_BASE: &str = "https://api.github.com";
const API_VERSION: &str = "2026-03-10";
const CACHE_SCHEMA_VERSION: u32 = 1;
const INSTALL_RECEIPT_SCHEMA: &str = "kendr.install/v1";
const INSTALL_RECEIPT_NAME: &str = ".kendr-opt-install.json";
const SUCCESS_CHECK_INTERVAL: u64 = 24 * 60 * 60;
const FAILED_CHECK_INTERVAL: u64 = 6 * 60 * 60;
const NOTICE_INTERVAL: u64 = 24 * 60 * 60;
const PASSIVE_TIMEOUT: Duration = Duration::from_millis(900);
const EXPLICIT_CHECK_TIMEOUT: Duration = Duration::from_secs(30);
const DOWNLOAD_TIMEOUT: Duration = Duration::from_secs(120);
const MAX_METADATA_BYTES: usize = 2 * 1024 * 1024;
const MAX_CHECKSUM_BYTES: u64 = 1024 * 1024;
const MAX_ARCHIVE_BYTES: u64 = 128 * 1024 * 1024;
const MAX_ARCHIVE_MEMBER_BYTES: u64 = 64 * 1024 * 1024;
const MAX_RELEASE_PAGES: usize = 5;
const MAX_RECEIPT_BYTES: u64 = 64 * 1024;
const MAX_CACHE_BYTES: u64 = 256 * 1024;
const MAX_SMOKE_OUTPUT_BYTES: usize = 1024 * 1024;
const SMOKE_TIMEOUT: Duration = Duration::from_secs(10);

type AnyResult<T> = Result<T, Box<dyn Error>>;

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize, ValueEnum)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Channel {
    Stable,
    #[default]
    Preview,
}

impl fmt::Display for Channel {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Stable => "stable",
            Self::Preview => "preview",
        })
    }
}

#[derive(Clone, Debug, Deserialize)]
struct ApiRepository {
    id: u64,
    full_name: String,
    private: bool,
    archived: bool,
    disabled: bool,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct ApiRelease {
    id: u64,
    tag_name: String,
    html_url: String,
    draft: bool,
    prerelease: bool,
    immutable: bool,
    published_at: Option<String>,
    assets: Vec<ApiAsset>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct ApiAsset {
    id: u64,
    name: String,
    size: u64,
    state: String,
    digest: Option<String>,
}

#[derive(Clone, Debug)]
struct EligibleRelease {
    source: ApiRelease,
    version: Version,
    archive: ApiAsset,
    checksums: ApiAsset,
}

impl EligibleRelease {
    fn cached(&self) -> CachedRelease {
        CachedRelease {
            version: self.version.to_string(),
            prerelease: self.source.prerelease,
            release_url: self.source.html_url.clone(),
            source: self.source.clone(),
        }
    }
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(default)]
struct UpdateCache {
    schema_version: u32,
    channel: Option<Channel>,
    etag: Option<String>,
    checked_at_unix: Option<u64>,
    next_check_after_unix: u64,
    latest: Option<CachedRelease>,
    last_notified_at_unix: Option<u64>,
    last_notified_current: Option<String>,
    last_notified_latest: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct CachedRelease {
    version: String,
    prerelease: bool,
    release_url: String,
    source: ApiRelease,
}

impl CachedRelease {
    fn eligible(&self, channel: Channel, target: &str) -> AnyResult<EligibleRelease> {
        let release = select_latest_release(vec![self.source.clone()], channel, target)?;
        if release.version.to_string() != self.version
            || release.source.prerelease != self.prerelease
            || release.source.html_url != self.release_url
        {
            return Err(other_error(
                "Cached GitHub release metadata is internally inconsistent",
            ));
        }
        Ok(release)
    }
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct InstallReceipt {
    schema_version: String,
    repository: String,
    install_method: String,
    target: String,
    version: String,
    channel: Channel,
}

#[derive(Debug, Serialize)]
struct UpdateReport {
    schema_version: &'static str,
    status: UpdateStatus,
    current_version: String,
    latest_version: String,
    channel: Channel,
    prerelease: bool,
    release_url: String,
    release_id: u64,
    immutable: bool,
    target: String,
    archive_name: String,
    archive_sha256: String,
    checked_at_unix: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    executable: Option<String>,
}

#[derive(Clone, Copy, Debug, Serialize)]
#[serde(rename_all = "snake_case")]
enum UpdateStatus {
    UpToDate,
    UpdateAvailable,
    Updated,
}

enum ReleasesResponse {
    Fresh {
        releases: Vec<ApiRelease>,
        etag: Option<String>,
    },
    NotModified,
}

struct UpdateLock(File);

impl Drop for UpdateLock {
    fn drop(&mut self) {
        let _ = FileExt::unlock(&self.0);
    }
}

pub(crate) async fn execute(
    check_only: bool,
    json: bool,
    channel: Option<Channel>,
    force: bool,
    reinstall: bool,
) -> AnyResult<()> {
    let channel = match channel {
        Some(channel) => channel,
        None => installed_channel()?.unwrap_or(Channel::Preview),
    };
    let current = Version::parse(env!("CARGO_PKG_VERSION"))?;
    let checked_at = unix_time()?;
    let mut cache = load_cache().unwrap_or_default();
    let latest = refresh_latest(&mut cache, channel, EXPLICIT_CHECK_TIMEOUT, check_only).await?;
    save_cache_best_effort(&cache);

    let is_newer = latest.version > current;
    let should_install = is_newer || (reinstall && latest.version == current);
    if check_only || !should_install {
        let status = if is_newer {
            UpdateStatus::UpdateAvailable
        } else {
            UpdateStatus::UpToDate
        };
        let report = report(status, &current, &latest, channel, checked_at, None)?;
        print_report(&report, json);
        return Ok(());
    }

    let executable = env::current_exe()?;
    validate_update_destination(&executable, force)?;
    let _lock = acquire_update_lock(&executable)?;
    let installed_path = download_verify_and_replace(&latest, &executable, channel).await?;

    cache.latest = Some(latest.cached());
    cache.channel = Some(channel);
    cache.checked_at_unix = Some(checked_at);
    cache.next_check_after_unix = checked_at.saturating_add(SUCCESS_CHECK_INTERVAL);
    cache.last_notified_at_unix = None;
    cache.last_notified_current = None;
    cache.last_notified_latest = None;
    save_cache_best_effort(&cache);

    let report = report(
        UpdateStatus::Updated,
        &current,
        &latest,
        channel,
        checked_at,
        Some(installed_path),
    )?;
    print_report(&report, json);
    Ok(())
}

pub(crate) async fn maybe_print_update_notice() {
    if !passive_check_allowed(
        io::stderr().is_terminal(),
        env::var_os("CI").is_some(),
        env::var_os("GITHUB_ACTIONS").is_some(),
        env_truthy("KENDR_NO_UPDATE_CHECK"),
    ) {
        return;
    }

    let Ok(now) = unix_time() else {
        return;
    };
    let mut cache = load_cache().unwrap_or_default();
    let channel = installed_channel()
        .ok()
        .flatten()
        .unwrap_or(Channel::Preview);
    let Ok(target) = platform_target() else {
        return;
    };
    if cache.channel != Some(channel) {
        cache = UpdateCache::default();
    }
    if cache
        .latest
        .as_ref()
        .is_some_and(|latest| latest.eligible(channel, target).is_err())
    {
        cache = UpdateCache::default();
    }

    if now >= cache.next_check_after_unix {
        match refresh_latest(&mut cache, channel, PASSIVE_TIMEOUT, true).await {
            Ok(_) => {}
            Err(_) => {
                cache.schema_version = CACHE_SCHEMA_VERSION;
                cache.channel = Some(channel);
                cache.next_check_after_unix = now.saturating_add(FAILED_CHECK_INTERVAL);
                save_cache_best_effort(&cache);
                return;
            }
        }
    }

    let Some(latest) = cache.latest.as_ref() else {
        save_cache_best_effort(&cache);
        return;
    };
    let Ok(current_version) = Version::parse(env!("CARGO_PKG_VERSION")) else {
        return;
    };
    let Ok(latest_version) = Version::parse(&latest.version) else {
        return;
    };
    if latest_version <= current_version {
        save_cache_best_effort(&cache);
        return;
    }

    let already_notified = cache.last_notified_current.as_deref()
        == Some(env!("CARGO_PKG_VERSION"))
        && cache.last_notified_latest.as_deref() == Some(latest.version.as_str())
        && cache
            .last_notified_at_unix
            .is_some_and(|last| now.saturating_sub(last) < NOTICE_INTERVAL);
    if !already_notified {
        eprintln!(
            "kendr-opt {} is available (installed {}); run `kendr-opt update` - {}",
            latest.version,
            env!("CARGO_PKG_VERSION"),
            latest.release_url
        );
        cache.last_notified_at_unix = Some(now);
        cache.last_notified_current = Some(env!("CARGO_PKG_VERSION").to_owned());
        cache.last_notified_latest = Some(latest.version.clone());
    }
    save_cache_best_effort(&cache);
}

fn passive_check_allowed(
    stderr_is_terminal: bool,
    ci: bool,
    github_actions: bool,
    disabled: bool,
) -> bool {
    stderr_is_terminal && !ci && !github_actions && !disabled
}

async fn refresh_latest(
    cache: &mut UpdateCache,
    channel: Channel,
    timeout: Duration,
    conditional: bool,
) -> AnyResult<EligibleRelease> {
    let etag = if conditional && cache.channel == Some(channel) {
        cache.etag.as_deref()
    } else {
        None
    };
    let result = tokio::time::timeout(timeout, fetch_latest(channel, etag, timeout))
        .await
        .map_err(|_| other_error("GitHub release check timed out"))??;
    let now = unix_time()?;

    let latest = match result {
        FetchLatest::Fresh { latest, etag } => {
            cache.latest = Some(latest.cached());
            cache.etag = etag;
            *latest
        }
        FetchLatest::NotModified => {
            let cached = cache.latest.as_ref().ok_or_else(|| {
                other_error("GitHub returned 304 without cached release metadata")
            })?;
            cached.eligible(channel, platform_target()?)?
        }
    };
    cache.schema_version = CACHE_SCHEMA_VERSION;
    cache.channel = Some(channel);
    cache.checked_at_unix = Some(now);
    cache.next_check_after_unix = now.saturating_add(SUCCESS_CHECK_INTERVAL);
    Ok(latest)
}

enum FetchLatest {
    Fresh {
        latest: Box<EligibleRelease>,
        etag: Option<String>,
    },
    NotModified,
}

async fn fetch_latest(
    channel: Channel,
    etag: Option<&str>,
    timeout: Duration,
) -> AnyResult<FetchLatest> {
    let base = api_base_url()?;
    let allow_insecure = insecure_loopback_enabled(&base);
    let client = metadata_client(timeout, &base, allow_insecure)?;
    verify_repository_identity(&client, &base).await?;
    match fetch_releases(&client, &base, etag).await? {
        ReleasesResponse::NotModified => Ok(FetchLatest::NotModified),
        ReleasesResponse::Fresh { releases, etag } => {
            let latest = select_latest_release(releases, channel, platform_target()?)?;
            Ok(FetchLatest::Fresh {
                latest: Box::new(latest),
                etag,
            })
        }
    }
}

fn report(
    status: UpdateStatus,
    current: &Version,
    latest: &EligibleRelease,
    channel: Channel,
    checked_at_unix: u64,
    executable: Option<PathBuf>,
) -> AnyResult<UpdateReport> {
    Ok(UpdateReport {
        schema_version: "kendr.update/v1",
        status,
        current_version: current.to_string(),
        latest_version: latest.version.to_string(),
        channel,
        prerelease: latest.source.prerelease,
        release_url: latest.source.html_url.clone(),
        release_id: latest.source.id,
        immutable: latest.source.immutable,
        target: platform_target()?.to_owned(),
        archive_name: latest.archive.name.clone(),
        archive_sha256: parse_github_digest(latest.archive.digest.as_deref())?,
        checked_at_unix,
        executable: executable.map(|path| path.display().to_string()),
    })
}

fn print_report(report: &UpdateReport, json: bool) {
    if json {
        println!(
            "{}",
            serde_json::to_string(report).expect("update report should serialize")
        );
        return;
    }
    match report.status {
        UpdateStatus::UpToDate => {
            if report.current_version == report.latest_version {
                println!("kendr-opt {} is up to date.", report.current_version);
            } else {
                println!(
                    "Installed kendr-opt {} is newer than the latest eligible {} release ({}).",
                    report.current_version, report.channel, report.latest_version
                );
            }
        }
        UpdateStatus::UpdateAvailable => {
            println!(
                "Update available: kendr-opt {} -> {}",
                report.current_version, report.latest_version
            );
            println!("Release: {}", report.release_url);
            println!("Run `kendr-opt update` to install it.");
        }
        UpdateStatus::Updated => {
            println!(
                "Updated kendr-opt {} -> {} at {}",
                report.current_version,
                report.latest_version,
                report
                    .executable
                    .as_deref()
                    .unwrap_or("the current executable")
            );
            println!("Bundled adapters refresh on the next `kendr-opt setup` or `kendr-opt run`.");
        }
    }
}

fn metadata_client(timeout: Duration, base: &Url, allow_insecure: bool) -> AnyResult<Client> {
    let origin = base.clone();
    let redirect_policy = reqwest::redirect::Policy::custom(move |attempt| {
        if attempt.previous().len() >= 5 {
            return attempt.error("too many update metadata redirects");
        }
        if secure_url(attempt.url(), allow_insecure) && same_origin(&origin, attempt.url()) {
            attempt.follow()
        } else {
            attempt.error("update metadata redirect left the configured API origin")
        }
    });
    build_http_client(timeout, redirect_policy)
}

fn asset_client(timeout: Duration, allow_insecure: bool) -> AnyResult<Client> {
    let redirect_policy = reqwest::redirect::Policy::custom(move |attempt| {
        if attempt.previous().len() >= 5 {
            return attempt.error("too many update asset redirects");
        }
        if trusted_asset_url(attempt.url(), allow_insecure) {
            attempt.follow()
        } else {
            attempt.error("update asset redirect left GitHub's HTTPS asset hosts")
        }
    });
    build_http_client(timeout, redirect_policy)
}

fn build_http_client(
    timeout: Duration,
    redirect_policy: reqwest::redirect::Policy,
) -> AnyResult<Client> {
    Ok(Client::builder()
        .user_agent(format!("kendr-opt/{}", env!("CARGO_PKG_VERSION")))
        .connect_timeout(timeout.min(Duration::from_secs(5)))
        .timeout(timeout)
        .min_tls_version(reqwest::tls::Version::TLS_1_2)
        .redirect(redirect_policy)
        .build()?)
}

fn api_base_url() -> AnyResult<Url> {
    if let Ok(value) = env::var("KENDR_UPDATE_API_URL") {
        #[cfg(not(feature = "update-test-server"))]
        {
            let _ = value;
            return Err(other_error(
                "This official kendr-opt build does not permit an update API override",
            ));
        }
        #[cfg(feature = "update-test-server")]
        {
            let url = Url::parse(value.trim_end_matches('/'))?;
            if !insecure_loopback_enabled(&url) {
                return Err(other_error(
                    "KENDR_UPDATE_API_URL is a build-time test feature restricted to numeric-loopback HTTP with KENDR_ALLOW_INSECURE=1",
                ));
            }
            return Ok(url);
        }
    }
    Ok(Url::parse(DEFAULT_API_BASE)?)
}

fn insecure_loopback_enabled(url: &Url) -> bool {
    #[cfg(feature = "update-test-server")]
    {
        no_url_userinfo(url)
            && url.scheme() == "http"
            && env_truthy("KENDR_ALLOW_INSECURE")
            && url
                .host_str()
                .and_then(|host| host.parse::<IpAddr>().ok())
                .is_some_and(|address| address.is_loopback())
    }
    #[cfg(not(feature = "update-test-server"))]
    {
        let _ = url;
        false
    }
}

fn update_test_server_active() -> bool {
    cfg!(feature = "update-test-server") && env::var_os("KENDR_UPDATE_API_URL").is_some()
}

fn secure_url(url: &Url, allow_insecure: bool) -> bool {
    no_url_userinfo(url)
        && (url.scheme() == "https" || (allow_insecure && insecure_loopback_enabled(url)))
}

fn trusted_asset_url(url: &Url, allow_insecure: bool) -> bool {
    if no_url_userinfo(url) && allow_insecure && insecure_loopback_enabled(url) {
        return true;
    }
    no_url_userinfo(url)
        && url.scheme() == "https"
        && url.host_str().is_some_and(|host| {
            host == "api.github.com"
                || host == "github.com"
                || host.ends_with(".githubusercontent.com")
        })
}

fn no_url_userinfo(url: &Url) -> bool {
    url.username().is_empty() && url.password().is_none()
}

fn api_url(base: &Url, suffix: &str) -> AnyResult<Url> {
    Ok(Url::parse(&format!(
        "{}/{}",
        base.as_str().trim_end_matches('/'),
        suffix.trim_start_matches('/')
    ))?)
}

async fn verify_repository_identity(client: &Client, base: &Url) -> AnyResult<()> {
    let url = api_url(base, &format!("repos/{REPOSITORY}"))?;
    let response = api_get(client, url, None).await?;
    let bytes = response_bytes(response, MAX_METADATA_BYTES).await?;
    let repository: ApiRepository = serde_json::from_slice(&bytes)?;
    if repository.id != REPOSITORY_ID
        || repository.full_name != REPOSITORY
        || repository.private
        || repository.archived
        || repository.disabled
    {
        return Err(other_error(
            "GitHub repository identity or status did not match the compiled Kendr update authority",
        ));
    }
    Ok(())
}

async fn fetch_releases(
    client: &Client,
    base: &Url,
    etag: Option<&str>,
) -> AnyResult<ReleasesResponse> {
    let mut next = Some(api_url(
        base,
        &format!("repos/{REPOSITORY}/releases?per_page=100"),
    )?);
    let mut page = 0usize;
    let mut releases = Vec::new();
    let mut response_etag = None;

    while let Some(url) = next.take() {
        page += 1;
        if page > MAX_RELEASE_PAGES {
            return Err(other_error(
                "GitHub release pagination exceeded the safety limit",
            ));
        }
        if !same_origin(base, &url) {
            return Err(other_error(
                "GitHub release pagination left the configured API origin",
            ));
        }
        let response = api_get(client, url, if page == 1 { etag } else { None }).await?;
        if page == 1 && response.status() == StatusCode::NOT_MODIFIED {
            return Ok(ReleasesResponse::NotModified);
        }
        if page == 1 {
            response_etag = response
                .headers()
                .get(ETAG)
                .and_then(|value| value.to_str().ok())
                .map(str::to_owned);
        }
        next = response
            .headers()
            .get(reqwest::header::LINK)
            .and_then(|value| value.to_str().ok())
            .and_then(next_link)
            .map(Url::parse)
            .transpose()?;
        let bytes = response_bytes(response, MAX_METADATA_BYTES).await?;
        releases.extend(serde_json::from_slice::<Vec<ApiRelease>>(&bytes)?);
    }
    Ok(ReleasesResponse::Fresh {
        releases,
        etag: response_etag,
    })
}

async fn fetch_release_by_id(
    client: &Client,
    base: &Url,
    release_id: u64,
) -> AnyResult<ApiRelease> {
    let url = api_url(base, &format!("repos/{REPOSITORY}/releases/{release_id}"))?;
    let response = api_get(client, url, None).await?;
    let bytes = response_bytes(response, MAX_METADATA_BYTES).await?;
    Ok(serde_json::from_slice(&bytes)?)
}

async fn api_get(client: &Client, url: Url, etag: Option<&str>) -> AnyResult<reqwest::Response> {
    let requested_url = url.clone();
    let mut request = client
        .get(url)
        .header(ACCEPT, "application/vnd.github+json")
        .header("X-GitHub-Api-Version", API_VERSION);
    if let Some(etag) = etag {
        request = request.header(IF_NONE_MATCH, etag);
    }
    let response = request.send().await?;
    if !same_origin(&requested_url, response.url()) {
        return Err(other_error(
            "GitHub update metadata redirected outside the configured API origin",
        ));
    }
    if response.status() == StatusCode::NOT_MODIFIED || response.status().is_success() {
        return Ok(response);
    }
    Err(http_status_error(&response))
}

fn http_status_error(response: &reqwest::Response) -> Box<dyn Error> {
    let remaining = response
        .headers()
        .get("x-ratelimit-remaining")
        .and_then(|value| value.to_str().ok())
        .unwrap_or("unknown");
    let reset = response
        .headers()
        .get("x-ratelimit-reset")
        .and_then(|value| value.to_str().ok())
        .unwrap_or("unknown");
    other_error(format!(
        "GitHub update request failed with status {} (rate-limit remaining: {remaining}, reset: {reset})",
        response.status()
    ))
}

async fn response_bytes(mut response: reqwest::Response, maximum: usize) -> AnyResult<Vec<u8>> {
    if response
        .content_length()
        .is_some_and(|length| length > maximum as u64)
    {
        return Err(other_error(
            "GitHub update metadata exceeded the size limit",
        ));
    }
    let mut bytes = Vec::new();
    while let Some(chunk) = response.chunk().await? {
        if bytes.len().saturating_add(chunk.len()) > maximum {
            return Err(other_error(
                "GitHub update metadata exceeded the size limit",
            ));
        }
        bytes.extend_from_slice(&chunk);
    }
    Ok(bytes)
}

fn next_link(header: &str) -> Option<&str> {
    header.split(',').find_map(|part| {
        let mut segments = part.trim().split(';');
        let url = segments.next()?.trim();
        let is_next = segments.any(|segment| segment.trim() == "rel=\"next\"");
        is_next
            .then(|| url.strip_prefix('<')?.strip_suffix('>'))
            .flatten()
    })
}

fn same_origin(left: &Url, right: &Url) -> bool {
    no_url_userinfo(left)
        && no_url_userinfo(right)
        && left.scheme() == right.scheme()
        && left.host_str() == right.host_str()
        && left.port_or_known_default() == right.port_or_known_default()
}

fn select_latest_release(
    releases: Vec<ApiRelease>,
    channel: Channel,
    target: &str,
) -> AnyResult<EligibleRelease> {
    let mut candidates = releases
        .into_iter()
        .filter_map(|release| {
            if release.draft || release.published_at.is_none() {
                return None;
            }
            let raw = release.tag_name.strip_prefix('v')?;
            let version = Version::parse(raw).ok()?;
            if !version.build.is_empty() || release.tag_name != format!("v{version}") {
                return None;
            }
            if !version.pre.is_empty() && !release.prerelease {
                return None;
            }
            if channel == Channel::Stable && (release.prerelease || !version.pre.is_empty()) {
                return None;
            }
            Some((version, release))
        })
        .collect::<Vec<_>>();
    candidates.sort_by(|left, right| left.0.cmp(&right.0));
    let (version, release) = candidates
        .pop()
        .ok_or_else(|| other_error(format!("No published {channel} Kendr release was found")))?;

    if !release.immutable {
        return Err(other_error(format!(
            "Release {} is mutable; Kendr refuses to auto-update until GitHub release immutability is enforced",
            release.tag_name
        )));
    }
    let expected_url = format!("https://github.com/{REPOSITORY}/releases/tag/v{version}");
    if !update_test_server_active() && release.html_url != expected_url {
        return Err(other_error(
            "GitHub release URL did not match the compiled Kendr repository",
        ));
    }

    let archive_name = archive_name(target)?;
    let assets = validate_release_assets(&release.assets)?;
    let archive = assets
        .get(&archive_name)
        .ok_or_else(|| {
            other_error(format!(
                "Release {} has no {archive_name}",
                release.tag_name
            ))
        })?
        .clone();
    let checksums = assets
        .get("SHA256SUMS")
        .ok_or_else(|| other_error(format!("Release {} has no SHA256SUMS", release.tag_name)))?
        .clone();
    Ok(EligibleRelease {
        source: release,
        version,
        archive,
        checksums,
    })
}

fn validate_release_assets(assets: &[ApiAsset]) -> AnyResult<BTreeMap<String, ApiAsset>> {
    let mut by_name = BTreeMap::new();
    let mut case_folded = HashSet::new();
    for asset in assets {
        if asset.id == 0
            || asset.size == 0
            || asset.size > MAX_ARCHIVE_BYTES
            || asset.state != "uploaded"
            || !safe_asset_name(&asset.name)
        {
            return Err(other_error(format!(
                "Release asset {:?} has invalid metadata",
                asset.name
            )));
        }
        parse_github_digest(asset.digest.as_deref())?;
        if !case_folded.insert(asset.name.to_ascii_lowercase())
            || by_name.insert(asset.name.clone(), asset.clone()).is_some()
        {
            return Err(other_error(format!(
                "Release contains a duplicate or case-colliding asset: {}",
                asset.name
            )));
        }
    }
    Ok(by_name)
}

fn safe_asset_name(name: &str) -> bool {
    !name.is_empty()
        && name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'.' | b'+' | b'-'))
}

fn parse_github_digest(value: Option<&str>) -> AnyResult<String> {
    let value = value.ok_or_else(|| other_error("GitHub release asset has no SHA-256 digest"))?;
    let digest = value
        .strip_prefix("sha256:")
        .ok_or_else(|| other_error("GitHub release asset digest is not SHA-256"))?;
    if digest.len() != 64
        || !digest
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err(other_error(
            "GitHub release asset has an invalid SHA-256 digest",
        ));
    }
    Ok(digest.to_owned())
}

async fn download_verify_and_replace(
    release: &EligibleRelease,
    executable: &Path,
    channel: Channel,
) -> AnyResult<PathBuf> {
    let base = api_base_url()?;
    let allow_insecure = insecure_loopback_enabled(&base);
    let metadata_client = metadata_client(DOWNLOAD_TIMEOUT, &base, allow_insecure)?;
    let asset_client = asset_client(DOWNLOAD_TIMEOUT, allow_insecure)?;
    let executable_directory = executable
        .parent()
        .ok_or_else(|| other_error("Current executable has no parent directory"))?;
    let temporary = TempDir::new_in(executable_directory)?;
    let checksum_path = temporary.path().join("SHA256SUMS");
    let archive_path = temporary.path().join(&release.archive.name);

    download_asset(
        &asset_client,
        &base,
        &release.checksums,
        &checksum_path,
        MAX_CHECKSUM_BYTES,
    )
    .await?;
    let checksum_digest = sha256_file(&checksum_path)?;
    if checksum_digest != parse_github_digest(release.checksums.digest.as_deref())? {
        return Err(other_error(
            "SHA256SUMS did not match GitHub's recorded digest",
        ));
    }
    let checksums = parse_checksum_manifest(&fs::read_to_string(&checksum_path)?)?;
    verify_checksum_manifest(&checksums, &release.source.assets)?;

    download_asset(
        &asset_client,
        &base,
        &release.archive,
        &archive_path,
        MAX_ARCHIVE_BYTES,
    )
    .await?;
    let archive_digest = sha256_file(&archive_path)?;
    let api_archive_digest = parse_github_digest(release.archive.digest.as_deref())?;
    if archive_digest != api_archive_digest {
        return Err(other_error(
            "Downloaded archive did not match GitHub's recorded digest",
        ));
    }
    if checksums.get(&release.archive.name) != Some(&archive_digest) {
        return Err(other_error("Downloaded archive did not match SHA256SUMS"));
    }

    let candidate = temporary.path().join(binary_filename(platform_target()?));
    extract_candidate(
        &archive_path,
        &candidate,
        &release.version,
        platform_target()?,
    )?;
    smoke_candidate(&candidate, &release.version)?;

    let refreshed = fetch_release_by_id(&metadata_client, &base, release.source.id).await?;
    if release_fingerprint(&refreshed)? != release_fingerprint(&release.source)? {
        return Err(other_error(
            "GitHub release metadata changed while the update was being verified",
        ));
    }

    install_verified_candidate(&candidate, executable, &release.version, channel)?;
    Ok(executable.to_path_buf())
}

async fn download_asset(
    client: &Client,
    base: &Url,
    asset: &ApiAsset,
    destination: &Path,
    maximum: u64,
) -> AnyResult<()> {
    if asset.size > maximum {
        return Err(other_error(format!(
            "Release asset {} exceeds the size limit",
            asset.name
        )));
    }
    let url = api_url(
        base,
        &format!("repos/{REPOSITORY}/releases/assets/{}", asset.id),
    )?;
    let mut response = client
        .get(url)
        .header(ACCEPT, "application/octet-stream")
        .header("X-GitHub-Api-Version", API_VERSION)
        .send()
        .await?;
    if !response.status().is_success() {
        return Err(http_status_error(&response));
    }
    if !trusted_asset_url(response.url(), insecure_loopback_enabled(base)) {
        return Err(other_error(
            "Release asset download ended outside GitHub's HTTPS asset hosts",
        ));
    }
    if response
        .content_length()
        .is_some_and(|length| length > maximum)
    {
        return Err(other_error(format!(
            "Release asset {} exceeds the size limit",
            asset.name
        )));
    }
    let mut output = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(destination)?;
    let mut written = 0u64;
    while let Some(chunk) = response.chunk().await? {
        written = written.saturating_add(chunk.len() as u64);
        if written > maximum {
            return Err(other_error(format!(
                "Release asset {} exceeds the size limit",
                asset.name
            )));
        }
        output.write_all(&chunk)?;
    }
    output.sync_all()?;
    if written != asset.size {
        return Err(other_error(format!(
            "Release asset {} size mismatch: expected {}, downloaded {}",
            asset.name, asset.size, written
        )));
    }
    Ok(())
}

fn release_fingerprint(release: &ApiRelease) -> AnyResult<String> {
    if !release.immutable || release.draft || release.published_at.is_none() {
        return Err(other_error("Release lost its immutable published status"));
    }
    let assets = validate_release_assets(&release.assets)?;
    let mut fingerprint = format!(
        "{}\n{}\n{}\n{}\n",
        release.id, release.tag_name, release.prerelease, release.immutable
    );
    for (name, asset) in assets {
        fingerprint.push_str(&format!(
            "{}:{}:{}:{}\n",
            name,
            asset.id,
            asset.size,
            parse_github_digest(asset.digest.as_deref())?
        ));
    }
    Ok(fingerprint)
}

fn parse_checksum_manifest(value: &str) -> AnyResult<BTreeMap<String, String>> {
    if !value.is_ascii() {
        return Err(other_error("SHA256SUMS is not ASCII"));
    }
    let mut checksums = BTreeMap::new();
    for line in value.lines() {
        let (digest, name) = line
            .split_once("  ")
            .ok_or_else(|| other_error(format!("Invalid SHA256SUMS line: {line:?}")))?;
        if digest.len() != 64
            || !digest
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
            || !safe_asset_name(name)
            || checksums
                .insert(name.to_owned(), digest.to_owned())
                .is_some()
        {
            return Err(other_error(format!("Invalid SHA256SUMS line: {line:?}")));
        }
    }
    Ok(checksums)
}

fn verify_checksum_manifest(
    checksums: &BTreeMap<String, String>,
    assets: &[ApiAsset],
) -> AnyResult<()> {
    let by_name = validate_release_assets(assets)?;
    let expected_names = by_name
        .keys()
        .filter(|name| name.as_str() != "SHA256SUMS")
        .cloned()
        .collect::<HashSet<_>>();
    let actual_names = checksums.keys().cloned().collect::<HashSet<_>>();
    if expected_names != actual_names {
        return Err(other_error(
            "SHA256SUMS does not cover the exact GitHub release asset set",
        ));
    }
    for name in expected_names {
        let asset = by_name
            .get(&name)
            .expect("expected checksum asset should exist");
        let expected = parse_github_digest(asset.digest.as_deref())?;
        if checksums.get(&name) != Some(&expected) {
            return Err(other_error(format!(
                "SHA256SUMS disagrees with GitHub's digest for {name}"
            )));
        }
    }
    Ok(())
}

fn sha256_file(path: &Path) -> AnyResult<String> {
    let mut file = File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buffer = vec![0u8; 1024 * 1024];
    loop {
        let read = file.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(hasher
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect())
}

fn extract_candidate(
    archive_path: &Path,
    candidate: &Path,
    version: &Version,
    target: &str,
) -> AnyResult<()> {
    if archive_path.extension().and_then(|value| value.to_str()) == Some("zip") {
        extract_zip_candidate(archive_path, candidate, version, target)
    } else {
        extract_tar_candidate(archive_path, candidate, version, target)
    }
}

fn expected_archive_members(version: &Version, target: &str) -> BTreeMap<String, u32> {
    let root = format!("kendr-opt-{version}-{target}");
    BTreeMap::from([
        (format!("{root}/{}", binary_filename(target)), 0o755),
        (format!("{root}/CHANGELOG.md"), 0o644),
        (format!("{root}/LICENSE"), 0o644),
        (format!("{root}/NOTICE"), 0o644),
        (format!("{root}/README.md"), 0o644),
        (format!("{root}/RUST_STDLIB_LICENSES.html"), 0o644),
        (format!("{root}/THIRD_PARTY_LICENSES.html"), 0o644),
    ])
}

fn extract_zip_candidate(
    archive_path: &Path,
    candidate: &Path,
    version: &Version,
    target: &str,
) -> AnyResult<()> {
    let expected = expected_archive_members(version, target);
    let binary_member = format!("kendr-opt-{version}-{target}/{}", binary_filename(target));
    let mut archive = zip::ZipArchive::new(File::open(archive_path)?)?;
    if archive.len() != expected.len() {
        return Err(other_error("Release ZIP has an unexpected archive layout"));
    }
    let mut seen = HashSet::new();
    let mut extracted = false;
    for index in 0..archive.len() {
        let mut entry = archive.by_index(index)?;
        let name = entry.name().to_owned();
        let enclosed = entry
            .enclosed_name()
            .ok_or_else(|| other_error("Release ZIP contains an unsafe path"))?;
        if enclosed.to_string_lossy().replace('\\', "/") != name
            || entry.is_dir()
            || !seen.insert(name.to_ascii_lowercase())
        {
            return Err(other_error(
                "Release ZIP contains an unsafe or duplicate member",
            ));
        }
        let expected_mode = expected
            .get(&name)
            .ok_or_else(|| other_error(format!("Unexpected release ZIP member: {name}")))?;
        let mode = entry
            .unix_mode()
            .ok_or_else(|| other_error(format!("Release ZIP member has no Unix mode: {name}")))?;
        let file_type = mode & 0o170000;
        if file_type != 0o100000
            || mode & 0o777 != *expected_mode
            || entry.size() > MAX_ARCHIVE_MEMBER_BYTES
        {
            return Err(other_error(format!("Invalid release ZIP member: {name}")));
        }
        if name == binary_member {
            let mut output = OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(candidate)?;
            io::copy(&mut entry, &mut output)?;
            output.sync_all()?;
            extracted = true;
        }
    }
    if !extracted {
        return Err(other_error(
            "Release ZIP did not contain the expected binary",
        ));
    }
    set_executable_permissions(candidate)?;
    Ok(())
}

fn extract_tar_candidate(
    archive_path: &Path,
    candidate: &Path,
    version: &Version,
    target: &str,
) -> AnyResult<()> {
    let expected = expected_archive_members(version, target);
    let binary_member = format!("kendr-opt-{version}-{target}/{}", binary_filename(target));
    let decoder = GzDecoder::new(File::open(archive_path)?);
    let mut archive = tar::Archive::new(decoder);
    let mut seen = HashSet::new();
    let mut count = 0usize;
    let mut extracted = false;
    for entry in archive.entries()? {
        let mut entry = entry?;
        count += 1;
        let path = entry.path()?.into_owned();
        let name = path
            .to_str()
            .ok_or_else(|| other_error("Release tar contains a non-UTF-8 path"))?
            .replace('\\', "/");
        if path.is_absolute()
            || path
                .components()
                .any(|component| matches!(component, std::path::Component::ParentDir))
            || !entry.header().entry_type().is_file()
            || !seen.insert(name.to_ascii_lowercase())
        {
            return Err(other_error(
                "Release tar contains an unsafe or duplicate member",
            ));
        }
        let expected_mode = expected
            .get(&name)
            .ok_or_else(|| other_error(format!("Unexpected release tar member: {name}")))?;
        if entry.header().mode()? & 0o777 != *expected_mode
            || entry.header().size()? > MAX_ARCHIVE_MEMBER_BYTES
        {
            return Err(other_error(format!("Invalid release tar member: {name}")));
        }
        if name == binary_member {
            let mut output = OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(candidate)?;
            io::copy(&mut entry, &mut output)?;
            output.sync_all()?;
            extracted = true;
        }
    }
    if count != expected.len() || !extracted {
        return Err(other_error("Release tar has an unexpected archive layout"));
    }
    set_executable_permissions(candidate)?;
    Ok(())
}

#[cfg(unix)]
fn set_executable_permissions(path: &Path) -> io::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(path, fs::Permissions::from_mode(0o755))
}

#[cfg(not(unix))]
fn set_executable_permissions(_path: &Path) -> io::Result<()> {
    Ok(())
}

fn smoke_candidate(candidate: &Path, expected_version: &Version) -> AnyResult<()> {
    let version = run_bounded_candidate(candidate, &["--version"])?;
    if !version.status.success()
        || String::from_utf8(version.stdout)?.trim() != format!("kendr-opt {expected_version}")
    {
        return Err(other_error("Downloaded binary version smoke test failed"));
    }
    let engines = run_bounded_candidate(candidate, &["engines", "--compact"])?;
    if !engines.status.success() {
        return Err(other_error("Downloaded binary engine smoke test failed"));
    }
    let engines: serde_json::Value = serde_json::from_slice(&engines.stdout)?;
    if engines.as_array().is_none_or(|engines| engines.is_empty()) {
        return Err(other_error(
            "Downloaded binary returned an empty engine list",
        ));
    }
    Ok(())
}

struct BoundedOutput {
    status: ExitStatus,
    stdout: Vec<u8>,
}

fn run_bounded_candidate(candidate: &Path, arguments: &[&str]) -> AnyResult<BoundedOutput> {
    let mut child = Command::new(candidate)
        .args(arguments)
        .env_clear()
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| other_error("Could not capture downloaded binary output"))?;
    let (sender, receiver) = std::sync::mpsc::sync_channel(1);
    thread::spawn(move || {
        let mut stdout = stdout;
        let mut captured = Vec::new();
        let mut exceeded = false;
        let mut buffer = [0u8; 8192];
        let result = loop {
            match stdout.read(&mut buffer) {
                Ok(0) => break Ok((captured, exceeded)),
                Ok(read) => {
                    let remaining = MAX_SMOKE_OUTPUT_BYTES.saturating_sub(captured.len());
                    captured.extend_from_slice(&buffer[..read.min(remaining)]);
                    exceeded |= read > remaining;
                }
                Err(error) => break Err(error),
            }
        };
        let _ = sender.send(result);
    });

    let deadline = Instant::now() + SMOKE_TIMEOUT;
    let status = loop {
        if let Some(status) = child.try_wait()? {
            break status;
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            return Err(other_error("Downloaded binary smoke test timed out"));
        }
        thread::sleep(Duration::from_millis(20));
    };
    let (stdout, exceeded) = receiver
        .recv_timeout(Duration::from_secs(1))
        .map_err(|_| other_error("Downloaded binary kept its output stream open"))??;
    if exceeded {
        return Err(other_error(
            "Downloaded binary smoke-test output exceeded the size limit",
        ));
    }
    Ok(BoundedOutput { status, stdout })
}

fn install_verified_candidate(
    candidate: &Path,
    executable: &Path,
    version: &Version,
    channel: Channel,
) -> AnyResult<()> {
    let parent = executable
        .parent()
        .ok_or_else(|| other_error("Current executable has no parent directory"))?;
    let mut backup = NamedTempFile::new_in(parent)?;
    let mut installed = File::open(executable)?;
    io::copy(&mut installed, &mut backup)?;
    backup.flush()?;
    backup.as_file().sync_all()?;
    let (backup_file, backup_path) = backup.keep()?;
    drop(backup_file);
    sync_directory(parent)?;

    if let Err(error) = replace_executable(candidate, executable, true) {
        if !executable.exists() {
            fs::copy(&backup_path, executable).map_err(|restore_error| {
                other_error(format!(
                    "Executable replacement failed ({error}) and emergency restoration from {} also failed: {restore_error}",
                    backup_path.display()
                ))
            })?;
        }
        let _ = fs::remove_file(&backup_path);
        return Err(error.into());
    }
    let validation = sync_directory(parent)
        .map_err(|error| other_error(format!("Could not sync the update directory: {error}")))
        .and_then(|_| smoke_candidate(executable, version))
        .and_then(|_| write_install_receipt(executable, version, channel));
    if let Err(validation_error) = validation {
        match replace_executable(&backup_path, executable, false) {
            Ok(()) => {
                sync_directory(parent)?;
                let _ = fs::remove_file(&backup_path);
                return Err(other_error(format!(
                    "The replacement failed post-install validation and the previous kendr-opt was restored: {validation_error}"
                )));
            }
            Err(rollback_error) => {
                return Err(other_error(format!(
                    "The replacement failed post-install validation ({validation_error}); automatic rollback failed ({rollback_error}). The previous executable remains at {}",
                    backup_path.display()
                )));
            }
        }
    }

    if let Err(error) = fs::remove_file(&backup_path) {
        eprintln!(
            "kendr-opt: update succeeded, but the previous executable could not be removed from {}: {error}",
            backup_path.display()
        );
    }
    if let Err(error) = sync_directory(parent) {
        eprintln!(
            "kendr-opt: update succeeded, but the installation directory sync failed: {error}"
        );
    }
    Ok(())
}

#[cfg(unix)]
fn replace_executable(
    candidate: &Path,
    _executable: &Path,
    _destination_is_running: bool,
) -> io::Result<()> {
    self_replace::self_replace(candidate)
}

#[cfg(windows)]
fn replace_executable(
    candidate: &Path,
    executable: &Path,
    destination_is_running: bool,
) -> io::Result<()> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Storage::FileSystem::ReplaceFileW;

    fn wide_path(path: &Path) -> io::Result<Vec<u16>> {
        let mut value = path.as_os_str().encode_wide().collect::<Vec<_>>();
        if value.contains(&0) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "update path contains a NUL character",
            ));
        }
        value.push(0);
        Ok(value)
    }

    let parent = executable
        .parent()
        .ok_or_else(|| io::Error::other("update executable has no parent directory"))?;
    let replaced_file = tempfile::Builder::new()
        .prefix(".kendr-opt-replaced-")
        .suffix(".exe")
        .tempfile_in(parent)?
        .into_temp_path();
    let replaced_path = replaced_file.to_path_buf();
    replaced_file.close()?;

    let executable_wide = wide_path(executable)?;
    let candidate_wide = wide_path(candidate)?;
    let replaced_wide = wide_path(&replaced_path)?;
    let replaced = unsafe {
        ReplaceFileW(
            executable_wide.as_ptr(),
            candidate_wide.as_ptr(),
            replaced_wide.as_ptr(),
            0,
            std::ptr::null(),
            std::ptr::null(),
        )
    };
    if replaced == 0 {
        return Err(io::Error::last_os_error());
    }

    let cleanup = if destination_is_running {
        // ReplaceFileW moved this process's mapped image to `replaced_path`.
        // A known-good copy of that image waits for this process to exit and
        // removes it, avoiding Windows' anonymous `~RF*.TMP` leftovers.
        self_replace::self_delete_at(&replaced_path)
    } else {
        // Rollback replaces a candidate whose smoke-test child has exited, so
        // the displaced file is no longer mapped and can be removed now.
        fs::remove_file(&replaced_path)
    };
    if let Err(error) = cleanup {
        eprintln!(
            "kendr-opt: executable replacement succeeded, but cleanup of {} could not be scheduled: {error}",
            replaced_path.display()
        );
    }
    Ok(())
}

#[cfg(unix)]
fn sync_directory(path: &Path) -> io::Result<()> {
    File::open(path)?.sync_all()
}

#[cfg(not(unix))]
fn sync_directory(_path: &Path) -> io::Result<()> {
    Ok(())
}

fn platform_target() -> AnyResult<&'static str> {
    match (env::consts::OS, env::consts::ARCH) {
        ("linux", "x86_64") => Ok("x86_64-unknown-linux-musl"),
        ("linux", "aarch64") => Ok("aarch64-unknown-linux-musl"),
        ("macos", "x86_64") => Ok("x86_64-apple-darwin"),
        ("macos", "aarch64") => Ok("aarch64-apple-darwin"),
        ("windows", "x86_64") => Ok("x86_64-pc-windows-msvc"),
        (os, architecture) => Err(other_error(format!(
            "Unsupported update platform: {os}/{architecture}"
        ))),
    }
}

fn archive_name(target: &str) -> AnyResult<String> {
    let suffix = if target == "x86_64-pc-windows-msvc" {
        ".zip"
    } else if matches!(
        target,
        "x86_64-unknown-linux-musl"
            | "aarch64-unknown-linux-musl"
            | "x86_64-apple-darwin"
            | "aarch64-apple-darwin"
    ) {
        ".tar.gz"
    } else {
        return Err(other_error(format!("Unsupported release target: {target}")));
    };
    Ok(format!("kendr-opt-{target}{suffix}"))
}

fn binary_filename(target: &str) -> &'static str {
    if target.ends_with("windows-msvc") {
        "kendr-opt.exe"
    } else {
        "kendr-opt"
    }
}

fn validate_update_destination(executable: &Path, force: bool) -> AnyResult<()> {
    let metadata = fs::symlink_metadata(executable)?;
    if !metadata.file_type().is_file() || metadata.file_type().is_symlink() {
        return Err(other_error(format!(
            "Refusing to update a non-regular or symbolic-link executable: {}",
            executable.display()
        )));
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x400;
        if metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
            return Err(other_error(format!(
                "Refusing to update a reparse-point executable: {}",
                executable.display()
            )));
        }
    }

    let parent = executable
        .parent()
        .ok_or_else(|| other_error("Current executable has no parent directory"))?;
    let writable_probe = NamedTempFile::new_in(parent).map_err(|error| {
        other_error(format!(
            "Cannot stage an update beside {}: {error}. Kendr never elevates automatically.",
            executable.display()
        ))
    })?;
    drop(writable_probe);

    if force || valid_install_receipt(executable)? {
        return Ok(());
    }
    Err(other_error(format!(
        "{} has no official Kendr install receipt. Use its package manager, reinstall from the GitHub Release, or explicitly allow this standalone binary with `kendr-opt update --force`.",
        executable.display()
    )))
}

fn valid_install_receipt(executable: &Path) -> AnyResult<bool> {
    let Some(receipt) = read_install_receipt(executable)? else {
        return Ok(false);
    };
    let receipt_version = Version::parse(&receipt.version)?;
    if receipt_version != Version::parse(env!("CARGO_PKG_VERSION"))? {
        return Err(other_error(format!(
            "Install receipt {} records kendr-opt {}, but the running binary is {}",
            install_receipt_path(executable)?.display(),
            receipt_version,
            env!("CARGO_PKG_VERSION")
        )));
    }
    Ok(true)
}

fn installed_channel() -> AnyResult<Option<Channel>> {
    let executable = env::current_exe()?;
    let Some(receipt) = read_install_receipt(&executable)? else {
        return Ok(None);
    };
    if Version::parse(&receipt.version)? != Version::parse(env!("CARGO_PKG_VERSION"))? {
        return Err(other_error(format!(
            "Install receipt {} does not match the running kendr-opt version",
            install_receipt_path(&executable)?.display()
        )));
    }
    Ok(Some(receipt.channel))
}

fn read_install_receipt(executable: &Path) -> AnyResult<Option<InstallReceipt>> {
    let path = install_receipt_path(executable)?;
    let metadata = match fs::symlink_metadata(&path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(Box::new(error)),
    };
    if !metadata.file_type().is_file()
        || metadata.file_type().is_symlink()
        || metadata.len() > MAX_RECEIPT_BYTES
    {
        return Err(other_error(format!(
            "Install receipt {} is not a bounded regular file",
            path.display()
        )));
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x400;
        if metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
            return Err(other_error(format!(
                "Install receipt {} is a reparse point",
                path.display()
            )));
        }
    }
    let file = File::open(&path)?;
    let opened_metadata = file.metadata()?;
    if !opened_metadata.is_file() || opened_metadata.len() > MAX_RECEIPT_BYTES {
        return Err(other_error(format!(
            "Install receipt {} changed while it was being opened",
            path.display()
        )));
    }
    let mut bytes = Vec::with_capacity(opened_metadata.len() as usize);
    file.take(MAX_RECEIPT_BYTES.saturating_add(1))
        .read_to_end(&mut bytes)?;
    if bytes.len() as u64 > MAX_RECEIPT_BYTES {
        return Err(other_error(format!(
            "Install receipt {} exceeded the size limit while reading",
            path.display()
        )));
    }
    let receipt: InstallReceipt = serde_json::from_slice(&bytes)?;
    if receipt.schema_version != INSTALL_RECEIPT_SCHEMA
        || receipt.repository != REPOSITORY
        || receipt.install_method != "github-release"
        || receipt.target != platform_target()?
    {
        return Err(other_error(format!(
            "Install receipt {} does not identify this official Kendr release target",
            path.display()
        )));
    }
    Version::parse(&receipt.version)?;
    Ok(Some(receipt))
}

fn install_receipt_path(executable: &Path) -> AnyResult<PathBuf> {
    Ok(executable
        .parent()
        .ok_or_else(|| other_error("Current executable has no parent directory"))?
        .join(INSTALL_RECEIPT_NAME))
}

fn write_install_receipt(executable: &Path, version: &Version, channel: Channel) -> AnyResult<()> {
    let path = install_receipt_path(executable)?;
    let parent = path
        .parent()
        .ok_or_else(|| other_error("Install receipt has no parent directory"))?;
    if let Ok(metadata) = fs::symlink_metadata(&path) {
        if !metadata.file_type().is_file() || metadata.file_type().is_symlink() {
            return Err(other_error(format!(
                "Refusing to replace a non-regular install receipt: {}",
                path.display()
            )));
        }
        #[cfg(windows)]
        {
            use std::os::windows::fs::MetadataExt;
            const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x400;
            if metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
                return Err(other_error(format!(
                    "Refusing to replace a reparse-point install receipt: {}",
                    path.display()
                )));
            }
        }
    }
    let receipt = InstallReceipt {
        schema_version: INSTALL_RECEIPT_SCHEMA.to_owned(),
        repository: REPOSITORY.to_owned(),
        install_method: "github-release".to_owned(),
        target: platform_target()?.to_owned(),
        version: version.to_string(),
        channel,
    };
    let mut temporary = NamedTempFile::new_in(parent)?;
    serde_json::to_writer(&mut temporary, &receipt)?;
    temporary.write_all(b"\n")?;
    temporary.as_file().sync_all()?;
    temporary.persist(&path)?;
    Ok(())
}

fn acquire_update_lock(executable: &Path) -> AnyResult<UpdateLock> {
    let directory = executable
        .parent()
        .ok_or_else(|| other_error("Current executable has no parent directory"))?;
    let lock_path = directory.join(".kendr-opt-update.lock");
    if let Ok(metadata) = fs::symlink_metadata(&lock_path) {
        if !metadata.file_type().is_file() || metadata.file_type().is_symlink() {
            return Err(other_error(format!(
                "Refusing an unsafe update lock path: {}",
                lock_path.display()
            )));
        }
        #[cfg(windows)]
        {
            use std::os::windows::fs::MetadataExt;
            const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x400;
            if metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
                return Err(other_error(format!(
                    "Refusing a reparse-point update lock: {}",
                    lock_path.display()
                )));
            }
        }
    }
    let lock = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(&lock_path)?;
    FileExt::try_lock_exclusive(&lock)
        .map_err(|_| other_error("Another kendr-opt update is already running"))?;
    Ok(UpdateLock(lock))
}

fn cache_directory() -> Option<PathBuf> {
    if let Some(path) = env::var_os("KENDR_UPDATE_CACHE_DIR") {
        return Some(PathBuf::from(path));
    }
    if let Some(path) = env::var_os("KENDR_HOME") {
        return Some(PathBuf::from(path).join("cache"));
    }
    if cfg!(windows) {
        return env::var_os("LOCALAPPDATA")
            .map(PathBuf::from)
            .map(|path| path.join("Kendr/cache"));
    }
    if let Some(path) = env::var_os("XDG_CACHE_HOME") {
        return Some(PathBuf::from(path).join("kendr"));
    }
    env::var_os("HOME")
        .map(PathBuf::from)
        .map(|path| path.join(".cache/kendr"))
}

fn cache_path() -> Option<PathBuf> {
    cache_directory().map(|directory| directory.join("update.json"))
}

fn load_cache() -> AnyResult<UpdateCache> {
    let Some(path) = cache_path() else {
        return Ok(UpdateCache::default());
    };
    if !path.is_file() {
        return Ok(UpdateCache::default());
    }
    if fs::metadata(&path)?.len() > MAX_CACHE_BYTES {
        return Err(other_error("Update cache exceeded the size limit"));
    }
    let cache: UpdateCache = serde_json::from_slice(&fs::read(path)?)?;
    if cache.schema_version != CACHE_SCHEMA_VERSION {
        return Ok(UpdateCache::default());
    }
    Ok(cache)
}

fn save_cache_best_effort(cache: &UpdateCache) {
    let _ = save_cache(cache);
}

fn save_cache(cache: &UpdateCache) -> AnyResult<()> {
    let Some(path) = cache_path() else {
        return Ok(());
    };
    let parent = path
        .parent()
        .ok_or_else(|| other_error("Update cache has no parent directory"))?;
    fs::create_dir_all(parent)?;
    let mut temporary = NamedTempFile::new_in(parent)?;
    serde_json::to_writer(&mut temporary, cache)?;
    temporary.write_all(b"\n")?;
    temporary.as_file().sync_all()?;
    temporary.persist(path)?;
    Ok(())
}

fn unix_time() -> AnyResult<u64> {
    Ok(SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs())
}

fn env_truthy(name: &str) -> bool {
    env::var(name).is_ok_and(|value| {
        matches!(
            value.to_ascii_lowercase().as_str(),
            "1" | "true" | "yes" | "on"
        )
    })
}

fn other_error(message: impl Into<String>) -> Box<dyn Error> {
    Box::new(io::Error::other(message.into()))
}

#[cfg(test)]
mod tests {
    use super::*;

    const TEST_TARGET: &str = "x86_64-pc-windows-msvc";

    fn digest(byte: char) -> String {
        format!("sha256:{}", byte.to_string().repeat(64))
    }

    fn asset(id: u64, name: &str, byte: char) -> ApiAsset {
        ApiAsset {
            id,
            name: name.to_owned(),
            size: 10,
            state: "uploaded".to_owned(),
            digest: Some(digest(byte)),
        }
    }

    fn release(version: &str, prerelease: bool, immutable: bool) -> ApiRelease {
        ApiRelease {
            id: version.bytes().map(u64::from).sum(),
            tag_name: format!("v{version}"),
            html_url: format!("https://github.com/{REPOSITORY}/releases/tag/v{version}"),
            draft: false,
            prerelease,
            immutable,
            published_at: Some("2026-08-16T00:00:00Z".to_owned()),
            assets: vec![
                asset(1, "kendr-opt-x86_64-pc-windows-msvc.zip", 'a'),
                asset(2, "SHA256SUMS", 'b'),
            ],
        }
    }

    #[test]
    fn passive_checks_require_an_interactive_non_ci_terminal() {
        assert!(passive_check_allowed(true, false, false, false));
        assert!(!passive_check_allowed(false, false, false, false));
        assert!(!passive_check_allowed(true, true, false, false));
        assert!(!passive_check_allowed(true, false, true, false));
        assert!(!passive_check_allowed(true, false, false, true));
    }

    #[test]
    fn release_selection_uses_semver_and_channel_rules() {
        let preview = select_latest_release(
            vec![
                release("0.1.9", true, true),
                release("0.1.10", true, true),
                release("0.1.8", false, true),
            ],
            Channel::Preview,
            TEST_TARGET,
        )
        .unwrap();
        assert_eq!(preview.version, Version::parse("0.1.10").unwrap());

        let stable = select_latest_release(
            vec![release("0.1.10", true, true), release("0.1.8", false, true)],
            Channel::Stable,
            TEST_TARGET,
        )
        .unwrap();
        assert_eq!(stable.version, Version::parse("0.1.8").unwrap());
    }

    #[test]
    fn release_selection_rejects_mutable_and_incomplete_releases() {
        let mutable = select_latest_release(
            vec![release("0.2.0", true, false)],
            Channel::Preview,
            TEST_TARGET,
        )
        .unwrap_err()
        .to_string();
        assert!(mutable.contains("mutable"));

        let mut incomplete = release("0.2.0", true, true);
        incomplete.assets.retain(|asset| asset.name == "SHA256SUMS");
        let missing = select_latest_release(vec![incomplete], Channel::Preview, TEST_TARGET)
            .unwrap_err()
            .to_string();
        assert!(missing.contains("has no kendr-opt-x86_64-pc-windows-msvc.zip"));
    }

    #[test]
    fn release_selection_rejects_drafts_noncanonical_tags_and_build_metadata() {
        let mut draft = release("9.0.0", true, true);
        draft.draft = true;
        let mut noncanonical = release("1.2.3", true, true);
        noncanonical.tag_name = "v1.2.03".to_owned();
        let metadata = release("1.2.3+untrusted", true, true);
        let error = select_latest_release(
            vec![draft, noncanonical, metadata],
            Channel::Preview,
            TEST_TARGET,
        )
        .unwrap_err()
        .to_string();
        assert!(error.contains("No published preview"));
    }

    #[test]
    fn cached_release_is_revalidated_before_use() {
        let selected = select_latest_release(
            vec![release("0.3.0", true, true)],
            Channel::Preview,
            TEST_TARGET,
        )
        .unwrap();
        let mut cached = selected.cached();
        assert_eq!(
            cached
                .eligible(Channel::Preview, TEST_TARGET)
                .unwrap()
                .version,
            Version::parse("0.3.0").unwrap()
        );
        cached.release_url.push_str("/tampered");
        assert!(cached.eligible(Channel::Preview, TEST_TARGET).is_err());
    }

    #[test]
    fn redirect_trust_rejects_cross_origin_and_userinfo_urls() {
        let api = Url::parse("https://api.github.com/repos/Kendr-AI/Kendr-Optimizer").unwrap();
        let same_api = Url::parse("https://api.github.com/repos/Kendr-AI/other").unwrap();
        let other_host = Url::parse("https://example.com/then-back-to-github").unwrap();
        let userinfo = Url::parse("https://user@example.com/asset").unwrap();
        let release_asset = Url::parse(
            "https://release-assets.githubusercontent.com/github-production-release-asset",
        )
        .unwrap();

        assert!(same_origin(&api, &same_api));
        assert!(!same_origin(&api, &other_host));
        assert!(!secure_url(&userinfo, false));
        assert!(trusted_asset_url(&release_asset, false));
        assert!(!trusted_asset_url(&other_host, false));
        assert!(!trusted_asset_url(&userinfo, false));
    }

    #[test]
    fn release_assets_reject_invalid_digests_and_case_collisions() {
        let mut invalid = asset(1, "archive.zip", 'a');
        invalid.digest = Some("sha256:ABC".to_owned());
        assert!(validate_release_assets(&[invalid]).is_err());
        assert!(
            validate_release_assets(&[asset(1, "archive.zip", 'a'), asset(2, "ARCHIVE.ZIP", 'b'),])
                .is_err()
        );
    }

    #[test]
    fn checksum_manifest_requires_exact_names_and_github_digests() {
        let assets = vec![asset(1, "archive.zip", 'a'), asset(2, "SHA256SUMS", 'b')];
        let parsed =
            parse_checksum_manifest(&format!("{}  archive.zip\n", "a".repeat(64))).unwrap();
        verify_checksum_manifest(&parsed, &assets).unwrap();

        let extra = parse_checksum_manifest(&format!(
            "{}  archive.zip\n{}  extra.zip\n",
            "a".repeat(64),
            "c".repeat(64)
        ))
        .unwrap();
        assert!(verify_checksum_manifest(&extra, &assets).is_err());
        assert!(parse_checksum_manifest("abcd archive.zip\n").is_err());
    }

    #[test]
    fn pagination_parser_returns_only_the_next_link() {
        let header = concat!(
            "<https://api.github.com/page/1>; rel=\"prev\", ",
            "<https://api.github.com/page/3>; rel=\"next\""
        );
        assert_eq!(next_link(header), Some("https://api.github.com/page/3"));
        assert_eq!(
            next_link("<https://api.github.com/page/1>; rel=\"last\""),
            None
        );
    }

    #[test]
    fn release_archive_contract_has_exact_members_and_modes() {
        let version = Version::parse("1.2.3").unwrap();
        let members = expected_archive_members(&version, TEST_TARGET);
        assert_eq!(members.len(), 7);
        assert_eq!(
            members.get("kendr-opt-1.2.3-x86_64-pc-windows-msvc/kendr-opt.exe"),
            Some(&0o755)
        );
        assert!(
            members
                .iter()
                .filter(|(name, _)| !name.ends_with("kendr-opt.exe"))
                .all(|(_, mode)| *mode == 0o644)
        );
        assert_eq!(
            archive_name("x86_64-unknown-linux-musl").unwrap(),
            "kendr-opt-x86_64-unknown-linux-musl.tar.gz"
        );
        assert_eq!(
            archive_name(TEST_TARGET).unwrap(),
            "kendr-opt-x86_64-pc-windows-msvc.zip"
        );
        assert!(archive_name("unsupported-target").is_err());
    }

    #[test]
    fn install_receipt_round_trips_and_must_match_the_running_version() {
        let directory = tempfile::tempdir().unwrap();
        let executable = directory
            .path()
            .join(binary_filename(platform_target().unwrap()));
        fs::write(&executable, b"fixture").unwrap();
        let version = Version::parse(env!("CARGO_PKG_VERSION")).unwrap();

        write_install_receipt(&executable, &version, Channel::Stable).unwrap();
        assert!(valid_install_receipt(&executable).unwrap());
        assert_eq!(
            read_install_receipt(&executable).unwrap().unwrap().channel,
            Channel::Stable
        );

        write_install_receipt(&executable, &version, Channel::Preview).unwrap();
        assert_eq!(
            read_install_receipt(&executable).unwrap().unwrap().channel,
            Channel::Preview
        );

        let path = install_receipt_path(&executable).unwrap();
        let mut receipt: serde_json::Value =
            serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        receipt["version"] = serde_json::Value::String("99.0.0".to_owned());
        fs::write(path, serde_json::to_vec(&receipt).unwrap()).unwrap();
        assert!(valid_install_receipt(&executable).is_err());
    }

    #[cfg(windows)]
    #[test]
    fn windows_replace_file_keeps_a_valid_destination() {
        let directory = tempfile::tempdir().unwrap();
        let executable = directory.path().join("kendr-opt.exe");
        let candidate = directory.path().join("candidate.exe");
        fs::write(&executable, b"old").unwrap();
        fs::write(&candidate, b"new").unwrap();

        replace_executable(&candidate, &executable, false).unwrap();
        assert_eq!(fs::read(&executable).unwrap(), b"new");
        assert!(!candidate.exists());
        assert_eq!(fs::read_dir(directory.path()).unwrap().count(), 1);
    }
}
