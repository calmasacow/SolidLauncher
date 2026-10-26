//! Fetch the latest SolidCraft release and the package for this computer.
//!
//! The only download host is `https://github.com/tangency-labs/solidcraft/releases/download/`.
//! GitHub may redirect that URL to its asset storage; the request itself starts on the release URL.

use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::time::Duration;

use sha2::{Digest, Sha256};
use solid_launcher_core::{
    download_url_ok, parse_github_release, parse_sha256sums, pick_asset, PackageKind, Platform,
    Release, REPO,
};

use crate::install::{self, Installed};
use crate::USER_AGENT;

const MAX_JSON: u64 = 2 * 1024 * 1024;
const MAX_SUMS: u64 = 1024 * 1024;
const MAX_ARCHIVE: u64 = 400 * 1024 * 1024;

/// What the window shows before anyone clicks Install.
#[derive(Clone, Debug)]
pub struct Status {
    pub installed: Option<String>,
    pub latest: String,
    pub asset: String,
    pub notes: String,
    pub html_url: String,
    /// What Update would install: a tarball, an installer, a disk image.
    pub kind: PackageKind,
    /// `true` when nothing is installed, or the installed copy is older than `latest`.
    pub update_available: bool,
}

pub fn check_status() -> Result<Status, String> {
    let home = install::home_dir()?;
    let installed = install::detect(&home);
    let release = fetch_latest()?;
    let (asset, kind) = choose(&release)?;
    if asset.sha256.is_none() {
        return Err(format!(
            "{} was published without a SHA-256 checksum",
            asset.name
        ));
    }
    let latest_version = release.version.clone();
    let update_available = match (&installed, &latest_version) {
        (None, _) => true,
        (Some(current), Some(latest)) => latest > &current.version,
        (Some(_), None) => false,
    };
    Ok(Status {
        installed: installed.map(|item| item.version.to_string()),
        latest: release.version_label(),
        asset: asset.name.clone(),
        notes: release.notes,
        html_url: release.html_url,
        kind,
        update_available,
    })
}

/// Download the latest package for this computer, verify it, and install it.
///
/// `progress` is called with bytes received and the expected total (0 when unknown).
pub fn install_latest(progress: impl Fn(u64, u64)) -> Result<Installed, String> {
    let home = install::home_dir()?;
    let release = fetch_latest()?;
    let (asset, kind) = choose(&release)?;
    // `fetch_latest` refuses a release whose tag is not a version, so this is always set.
    let version = release
        .version
        .clone()
        .ok_or_else(|| format!("release tag {} is not a version", release.tag))?;
    let sha = asset
        .sha256
        .clone()
        .ok_or_else(|| format!("{} was published without a SHA-256 checksum", asset.name))?;
    if asset.size > MAX_ARCHIVE {
        return Err(format!(
            "{} is larger than SolidLauncher will download",
            asset.name
        ));
    }
    let cache = dirs::cache_dir()
        .ok_or("no cache directory")?
        .join("solid-launcher")
        .join("downloads");
    fs::create_dir_all(&cache)
        .map_err(|err| format!("could not create the download folder ({err})"))?;
    let dest = cache.join(&asset.name);
    download(&asset.url, &dest, &sha, asset.size, &progress)?;
    install::install_archive(&dest, &home, kind, &version)
}

fn choose(release: &Release) -> Result<(&solid_launcher_core::Asset, PackageKind), String> {
    let platform =
        Platform::current().ok_or("this computer is not a supported SolidCraft platform")?;
    pick_asset(release, platform, platform.install_kinds())
        .ok_or_else(|| "this release has no SolidCraft build for this computer".to_string())
}

fn fetch_latest() -> Result<Release, String> {
    let url = format!("https://api.github.com/repos/{REPO}/releases/latest");
    let body = get_text(&url, MAX_JSON, true)?;
    let mut release = parse_github_release(&body)?;
    if release.prerelease {
        return Err("the latest published SolidCraft release is a pre-release".to_string());
    }
    if release.version.is_none() {
        return Err(format!("release tag {} is not a version", release.tag));
    }
    if let Some(sums) = release.sums_asset().cloned() {
        let text = get_text(&sums.url, MAX_SUMS, false)?;
        release.apply_sums(&parse_sha256sums(&text))?;
    }
    Ok(release)
}

fn agent() -> ureq::Agent {
    ureq::AgentBuilder::new()
        .timeout_connect(Duration::from_secs(20))
        .timeout_read(Duration::from_secs(120))
        .redirects(8)
        .build()
}

fn call(url: &str, api: bool) -> Result<ureq::Response, String> {
    if !api && !download_url_ok(url) {
        return Err("refusing a download that is not a SolidCraft release file".to_string());
    }
    if api && !url.starts_with("https://api.github.com/repos/tangency-labs/solidcraft/") {
        return Err("refusing a request that is not the SolidCraft release API".to_string());
    }
    let request = agent()
        .get(url)
        .set("User-Agent", USER_AGENT)
        .set("Accept-Encoding", "identity");
    let request = if api {
        request.set("Accept", "application/vnd.github+json")
    } else {
        request
    };
    match request.call() {
        Ok(response) => Ok(response),
        Err(ureq::Error::Status(code, response)) => {
            let detail = response.into_string().unwrap_or_default();
            Err(format!("GitHub returned HTTP {code}: {}", squash(&detail)))
        }
        Err(err) => Err(format!("could not reach GitHub ({err})")),
    }
}

fn get_text(url: &str, max_bytes: u64, api: bool) -> Result<String, String> {
    let response = call(url, api)?;
    read_limited(response, max_bytes).and_then(|bytes| {
        String::from_utf8(bytes).map_err(|_| "GitHub sent a response that is not text".to_string())
    })
}

fn read_limited(response: ureq::Response, max_bytes: u64) -> Result<Vec<u8>, String> {
    if let Some(length) = content_length(&response) {
        if length > max_bytes {
            return Err("GitHub's answer is larger than SolidLauncher will read".to_string());
        }
    }
    let mut reader = response.into_reader();
    let mut body = Vec::new();
    let mut chunk = [0_u8; 16 * 1024];
    loop {
        let read = reader
            .read(&mut chunk)
            .map_err(|err| format!("the download stopped ({err})"))?;
        if read == 0 {
            break;
        }
        if body.len() as u64 + read as u64 > max_bytes {
            return Err("GitHub's answer is larger than SolidLauncher will read".to_string());
        }
        body.extend_from_slice(&chunk[..read]);
    }
    Ok(body)
}

fn download(
    url: &str,
    dest: &Path,
    sha256: &str,
    expected_size: u64,
    progress: &impl Fn(u64, u64),
) -> Result<(), String> {
    if dest.is_file() && file_sha256(dest)?.eq_ignore_ascii_case(sha256) {
        let len = fs::metadata(dest).map(|meta| meta.len()).unwrap_or(0);
        progress(len, expected_size.max(len));
        return Ok(());
    }
    let response = call(url, false)?;
    if let Some(length) = content_length(&response) {
        if expected_size != 0 && length != expected_size {
            return Err("the download size does not match the release".to_string());
        }
        if length > MAX_ARCHIVE {
            return Err("the download is larger than SolidLauncher will store".to_string());
        }
    }
    let total = content_length(&response).unwrap_or(expected_size);
    if let Some(parent) = dest.parent() {
        fs::create_dir_all(parent)
            .map_err(|err| format!("could not create the download folder ({err})"))?;
    }
    let partial = partial_path(dest);
    let mut file =
        File::create(&partial).map_err(|err| format!("could not save the download ({err})"))?;
    let mut reader = response.into_reader();
    let mut hasher = Sha256::new();
    let mut received = 0_u64;
    let mut chunk = [0_u8; 64 * 1024];
    loop {
        let read = reader.read(&mut chunk).map_err(|err| {
            let _ = fs::remove_file(&partial);
            format!("the download stopped ({err})")
        })?;
        if read == 0 {
            break;
        }
        received = received.saturating_add(read as u64);
        if received > MAX_ARCHIVE {
            let _ = fs::remove_file(&partial);
            return Err("the download is larger than SolidLauncher will store".to_string());
        }
        hasher.update(&chunk[..read]);
        if file.write_all(&chunk[..read]).is_err() {
            let _ = fs::remove_file(&partial);
            return Err("could not save the download".to_string());
        }
        progress(received, total);
    }
    if expected_size != 0 && received != expected_size {
        let _ = fs::remove_file(&partial);
        return Err("the download ended before the whole file arrived".to_string());
    }
    let digest = hex_encode(&hasher.finalize());
    if !digest.eq_ignore_ascii_case(sha256) {
        let _ = fs::remove_file(&partial);
        return Err("the download does not match the published SHA-256".to_string());
    }
    file.sync_all()
        .map_err(|err| format!("could not finish saving the download ({err})"))?;
    drop(file);
    fs::rename(&partial, dest)
        .map_err(|err| format!("could not finish saving the download ({err})"))?;
    Ok(())
}

fn file_sha256(path: &Path) -> Result<String, String> {
    let mut file =
        File::open(path).map_err(|err| format!("could not read {} ({err})", path.display()))?;
    let mut hasher = Sha256::new();
    let mut chunk = [0_u8; 64 * 1024];
    loop {
        let read = file
            .read(&mut chunk)
            .map_err(|err| format!("could not read the download ({err})"))?;
        if read == 0 {
            break;
        }
        hasher.update(&chunk[..read]);
    }
    Ok(hex_encode(&hasher.finalize()))
}

fn content_length(response: &ureq::Response) -> Option<u64> {
    response
        .header("content-length")
        .and_then(|value| value.parse().ok())
}

fn partial_path(dest: &Path) -> PathBuf {
    let mut name = dest.file_name().unwrap_or_default().to_os_string();
    name.push(".partial");
    dest.with_file_name(name)
}

fn hex_encode(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push(HEX[(byte >> 4) as usize] as char);
        out.push(HEX[(byte & 0xf) as usize] as char);
    }
    out
}

fn squash(text: &str) -> String {
    let one_line: String = text
        .chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .take(180)
        .collect();
    one_line.trim().to_string()
}
