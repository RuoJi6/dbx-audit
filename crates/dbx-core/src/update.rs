use serde::{Deserialize, Serialize};

const LATEST_JSON_URLS: &[&str] = &[
    "https://update.hwdns.net/https://github.com/t8y2/dbx/releases/latest/download/latest.json",
    "https://gh-proxy.org/https://github.com/t8y2/dbx/releases/latest/download/latest.json",
    "https://github.com/t8y2/dbx/releases/latest/download/latest.json",
];
const GITHUB_RELEASE_API_PREFIX: &str = "https://api.github.com/repos/t8y2/dbx/releases/tags/v";
const RELEASE_URL_PREFIX: &str = "https://github.com/t8y2/dbx/releases/tag/v";

#[derive(Debug, Deserialize)]
pub struct TauriRelease {
    pub version: String,
    #[serde(default)]
    pub notes: Option<String>,
    #[serde(skip)]
    pub github: Option<GithubReleaseMetadata>,
}

#[derive(Debug, Deserialize)]
pub struct GithubReleaseMetadata {
    pub name: Option<String>,
    pub html_url: Option<String>,
    pub body: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct UpdateInfo {
    pub current_version: String,
    pub latest_version: String,
    pub update_available: bool,
    pub release_name: String,
    pub release_url: String,
    pub release_notes: String,
}

pub async fn fetch_latest_release() -> Result<TauriRelease, String> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .map_err(|e| format!("Failed to create HTTP client: {e}"))?;
    let mut last_err = String::new();
    for url in LATEST_JSON_URLS {
        match client
            .get(*url)
            .header(reqwest::header::USER_AGENT, "dbx-update-checker")
            .send()
            .await
            .and_then(|r| r.error_for_status())
        {
            Ok(resp) => {
                let mut release =
                    resp.json::<TauriRelease>().await.map_err(|e| format!("Failed to parse update response: {e}"))?;
                if let Ok(github) = fetch_github_release_metadata(&client, &release.version).await {
                    release.github = Some(github);
                }
                return Ok(release);
            }
            Err(e) => {
                last_err = format!("{e}");
            }
        }
    }
    Err(format!("Failed to check updates: {last_err}"))
}

async fn fetch_github_release_metadata(
    client: &reqwest::Client,
    version: &str,
) -> Result<GithubReleaseMetadata, String> {
    let url = format!("{GITHUB_RELEASE_API_PREFIX}{}", normalize_version(version));
    client
        .get(url)
        .header(reqwest::header::USER_AGENT, "dbx-update-checker")
        .send()
        .await
        .and_then(|r| r.error_for_status())
        .map_err(|e| format!("{e}"))?
        .json::<GithubReleaseMetadata>()
        .await
        .map_err(|e| format!("Failed to parse GitHub release response: {e}"))
}

pub fn build_update_info(release: TauriRelease, current_version: &str) -> UpdateInfo {
    let latest_version = normalize_version(&release.version);
    let github = release.github.as_ref();
    let release_notes = github
        .and_then(|metadata| non_empty(metadata.body.as_deref()))
        .map(ToOwned::to_owned)
        .or(release.notes)
        .unwrap_or_default();
    let release_name = github
        .and_then(|metadata| non_empty(metadata.name.as_deref()))
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| format!("DBX v{latest_version}"));
    let release_url = github
        .and_then(|metadata| non_empty(metadata.html_url.as_deref()))
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| format!("{RELEASE_URL_PREFIX}{latest_version}"));

    UpdateInfo {
        update_available: is_newer_version(&latest_version, current_version),
        current_version: current_version.to_string(),
        release_name,
        release_url,
        release_notes,
        latest_version,
    }
}

fn non_empty(value: Option<&str>) -> Option<&str> {
    value.and_then(|value| {
        let trimmed = value.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(value)
        }
    })
}

pub fn normalize_version(version: &str) -> String {
    version.trim().trim_start_matches('v').to_string()
}

pub fn parse_version(version: &str) -> Vec<u64> {
    normalize_version(version).split(['.', '-', '+']).map(|part| part.parse::<u64>().unwrap_or(0)).collect()
}

pub fn is_newer_version(latest: &str, current: &str) -> bool {
    let latest_parts = parse_version(latest);
    let current_parts = parse_version(current);
    let max_len = latest_parts.len().max(current_parts.len());

    for i in 0..max_len {
        let latest_part = *latest_parts.get(i).unwrap_or(&0);
        let current_part = *current_parts.get(i).unwrap_or(&0);
        if latest_part > current_part {
            return true;
        }
        if latest_part < current_part {
            return false;
        }
    }

    false
}

#[cfg(test)]
mod tests {
    use super::{build_update_info, is_newer_version, normalize_version, GithubReleaseMetadata, TauriRelease};

    #[test]
    fn normalizes_tag_versions() {
        assert_eq!(normalize_version("v1.2.3"), "1.2.3");
        assert_eq!(normalize_version(" 0.2.0 "), "0.2.0");
    }

    #[test]
    fn compares_semver_like_versions() {
        assert!(is_newer_version("0.2.1", "0.2.0"));
        assert!(is_newer_version("1.0.0", "0.9.9"));
        assert!(!is_newer_version("0.2.0", "0.2.0"));
        assert!(!is_newer_version("0.1.9", "0.2.0"));
    }

    #[test]
    fn update_info_prefers_github_release_metadata() {
        let release = TauriRelease {
            version: "0.5.3".to_string(),
            notes: Some("See the assets below to download and install.".to_string()),
            github: Some(GithubReleaseMetadata {
                name: Some("DBX v0.5.3".to_string()),
                html_url: Some("https://github.com/t8y2/dbx/releases/tag/v0.5.3".to_string()),
                body: Some("### 新功能\n\n真实发布说明".to_string()),
            }),
        };

        let info = build_update_info(release, "0.5.2");

        assert_eq!(info.release_name, "DBX v0.5.3");
        assert_eq!(info.release_url, "https://github.com/t8y2/dbx/releases/tag/v0.5.3");
        assert_eq!(info.release_notes, "### 新功能\n\n真实发布说明");
    }
}
