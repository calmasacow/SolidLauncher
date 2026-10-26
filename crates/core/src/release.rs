//! GitHub releases for `tangency-labs/solidcraft`.
//!
//! The JSON is untrusted. Download links must stay under that repository's release downloads.

use semver::Version;

use crate::version;

/// GitHub `owner/repo` SolidCraft publishes from.
pub const REPO: &str = "tangency-labs/solidcraft";

/// One file attached to a release.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Asset {
    pub name: String,
    /// Bytes, or 0 when the release did not say.
    pub size: u64,
    /// `https://github.com/tangency-labs/solidcraft/releases/download/<tag>/<name>`.
    pub url: String,
    /// Lowercase hex SHA-256, when GitHub or `SHA256SUMS.txt` provided one.
    pub sha256: Option<String>,
}

/// A published SolidCraft release.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Release {
    pub tag: String,
    pub version: Option<Version>,
    pub html_url: String,
    pub notes: String,
    pub prerelease: bool,
    pub assets: Vec<Asset>,
}

impl Release {
    pub fn version_label(&self) -> String {
        self.version
            .as_ref()
            .map_or_else(|| self.tag.clone(), ToString::to_string)
    }

    /// Fills checksums from `SHA256SUMS.txt` where the asset does not already have one.
    ///
    /// An asset that already has a digest must agree with the sums file. A mismatch refuses
    /// the download and clears the stored digest.
    pub fn apply_sums(&mut self, sums: &[(String, String)]) -> Result<(), String> {
        for asset in &mut self.assets {
            let Some(from_file) = sums
                .iter()
                .find(|(_, name)| name == &asset.name)
                .map(|(hash, _)| hash.clone())
            else {
                continue;
            };
            if let Some(existing) = &asset.sha256 {
                if existing != &from_file {
                    asset.sha256 = None;
                    return Err(format!("{} has two different SHA-256 digests", asset.name));
                }
            } else {
                asset.sha256 = Some(from_file);
            }
        }
        Ok(())
    }

    pub fn sums_asset(&self) -> Option<&Asset> {
        self.assets
            .iter()
            .find(|asset| asset.name == "SHA256SUMS.txt")
    }
}

const MAX_NOTES: usize = 8 * 1024;

fn is_hex64(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

/// Prefix every SolidCraft download must start with.
pub fn download_prefix() -> String {
    format!("https://github.com/{REPO}/releases/download/")
}

/// `url` is exactly `https://github.com/<repo>/releases/download/<tag>/<name>`.
pub fn download_url_ok(url: &str) -> bool {
    let Some(rest) = url.strip_prefix(&download_prefix()) else {
        return false;
    };
    if rest.contains(['?', '#', '\\']) || contains_encoded_separator(rest) {
        return false;
    }
    let mut parts = rest.split('/');
    let (Some(tag), Some(name), None) = (parts.next(), parts.next(), parts.next()) else {
        return false;
    };
    !tag.is_empty()
        && tag != "."
        && tag != ".."
        && tag.len() <= 64
        && valid_file_name(name)
        && !tag
            .chars()
            .any(|c| !(c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-' | '+')))
}

fn contains_encoded_separator(rest: &str) -> bool {
    let bytes = rest.as_bytes();
    bytes.windows(3).any(|window| {
        window[0] == b'%' && matches!(&window[1..].to_ascii_lowercase()[..], b"2f" | b"5c")
    })
}

/// A file name we are willing to save: no directories and no control characters.
pub fn valid_file_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 200
        && name != "."
        && name != ".."
        && !name.starts_with('.')
        && !name.chars().any(|c| {
            c.is_control() || matches!(c, '/' | '\\' | ':' | '<' | '>' | '"' | '|' | '?' | '*')
        })
}

/// Reads `GET /repos/tangency-labs/solidcraft/releases/latest`.
pub fn parse_github_release(json: &str) -> Result<Release, String> {
    let value: serde_json::Value = serde_json::from_str(json)
        .map_err(|err| format!("unreadable answer from GitHub ({err})"))?;
    if let Some(message) = value["message"].as_str() {
        return Err(if message == "Not Found" {
            "SolidCraft has no published release".to_string()
        } else {
            format!("GitHub says: {}", truncate(message, 200))
        });
    }
    if value["draft"].as_bool().unwrap_or(false) {
        return Err("the latest SolidCraft release is still a draft".to_string());
    }
    let tag = value["tag_name"]
        .as_str()
        .filter(|tag| !tag.is_empty() && tag.len() <= 64)
        .ok_or("no release found")?
        .to_string();
    let html_url = format!("https://github.com/{REPO}/releases/tag/{tag}");
    let assets = value["assets"]
        .as_array()
        .map(|list| {
            list.iter()
                .filter_map(|asset| {
                    let name = asset["name"]
                        .as_str()
                        .filter(|name| valid_file_name(name))?
                        .to_string();
                    let url = asset["browser_download_url"]
                        .as_str()
                        .filter(|url| download_url_ok(url))?
                        .to_string();
                    let sha256 = asset["digest"]
                        .as_str()
                        .and_then(|digest| digest.strip_prefix("sha256:"))
                        .filter(|hash| is_hex64(hash))
                        .map(str::to_ascii_lowercase);
                    Some(Asset {
                        name,
                        size: asset["size"].as_u64().unwrap_or(0),
                        url,
                        sha256,
                    })
                })
                .collect()
        })
        .unwrap_or_default();
    Ok(Release {
        version: version::parse(&tag),
        tag,
        html_url,
        notes: value["body"]
            .as_str()
            .map(|body| truncate(body, MAX_NOTES))
            .unwrap_or_default(),
        prerelease: value["prerelease"].as_bool().unwrap_or(false),
        assets,
    })
}

/// Reads `sha256sum` output: `<hex>  <name>` or `<hex> *<name>` per line.
pub fn parse_sha256sums(text: &str) -> Vec<(String, String)> {
    text.lines()
        .filter_map(|line| {
            let (hash, rest) = line.trim().split_once(char::is_whitespace)?;
            let name = rest.trim().trim_start_matches('*');
            (is_hex64(hash) && valid_file_name(name))
                .then(|| (hash.to_ascii_lowercase(), name.to_string()))
        })
        .collect()
}

fn truncate(text: &str, max: usize) -> String {
    if text.len() <= max {
        return text.to_string();
    }
    let mut end = max;
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    let mut cut = text[..end].to_string();
    cut.push('…');
    cut
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_a_real_release_shape_and_drops_a_foreign_url() {
        let release =
            parse_github_release(include_str!("../tests/fixtures/solidcraft-release.json"))
                .expect("fixture");
        assert_eq!(release.version_label(), "0.22.1");
        assert!(!release.prerelease);
        let names: Vec<_> = release
            .assets
            .iter()
            .map(|asset| asset.name.as_str())
            .collect();
        assert!(names.contains(&"solidcraft-0.22.1-linux-x86_64.tar.gz"));
        assert!(!names.iter().any(|name| name.contains("evil")));
        let tarball = release
            .assets
            .iter()
            .find(|asset| asset.name.ends_with(".tar.gz"))
            .expect("tarball");
        assert_eq!(
            tarball.sha256.as_deref(),
            Some("20f40696e4d936adeaea8328fb145bcde4c151b5748cc67857072aa21aacb132")
        );
    }

    #[test]
    fn rejects_a_download_that_climbs_out_of_the_repo() {
        assert!(download_url_ok(
            "https://github.com/tangency-labs/solidcraft/releases/download/v0.22.1/solidcraft-0.22.1-linux-x86_64.tar.gz"
        ));
        assert!(!download_url_ok(
            "https://github.com/tangency-labs/solidcraft/releases/download/../../../attacker/repo/releases/download/v1/x"
        ));
        assert!(!download_url_ok(
            "https://example.com/solidcraft-0.22.1-linux-x86_64.tar.gz"
        ));
        assert!(!download_url_ok(
            "https://github.com/tangency-labs/solidcraft/releases/download/v0.22.1/solidcraft-0.22.1-linux-x86_64.tar.gz%2F..%2Fevil"
        ));
    }

    #[test]
    fn sums_fill_a_missing_digest_and_refuse_a_disagreement() {
        let mut release =
            parse_github_release(include_str!("../tests/fixtures/solidcraft-release.json"))
                .expect("fixture");
        let sums = parse_sha256sums(
            "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa  solidcraft-0.22.1-freebsd-x86_64.tar.gz\n",
        );
        release.apply_sums(&sums).expect("fill");
        let freebsd = release
            .assets
            .iter()
            .find(|asset| asset.name.contains("freebsd"))
            .expect("freebsd");
        assert_eq!(
            freebsd.sha256.as_deref(),
            Some("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa")
        );

        let mut conflict = release.clone();
        let bad = parse_sha256sums(
            "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb  solidcraft-0.22.1-linux-x86_64.tar.gz\n",
        );
        assert!(conflict.apply_sums(&bad).is_err());
    }
}
