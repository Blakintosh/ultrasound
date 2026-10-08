//! The release feed: versions, the bundle asset and its digest.

use serde::Deserialize;
use std::fmt;

pub const RELEASES_URL: &str = "https://github.com/Blakintosh/ultrasound/releases/tag/";

/// A release version, `X.Y.Z`. Pre-release and build suffixes are not releases and do not parse.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Version {
    major: u64,
    minor: u64,
    patch: u64,
}

impl Version {
    pub fn parse(text: &str) -> Option<Self> {
        let text = text.strip_prefix('v').unwrap_or(text);
        let mut parts = text.split('.').map(|part| {
            if part.is_empty() || !part.bytes().all(|b| b.is_ascii_digit()) {
                None
            } else {
                part.parse().ok()
            }
        });
        let version = Version {
            major: parts.next()??,
            minor: parts.next()??,
            patch: parts.next()??,
        };
        parts.next().is_none().then_some(version)
    }

    pub fn current() -> Self {
        Self::parse(env!("CARGO_PKG_VERSION")).expect("package version is X.Y.Z")
    }
}

impl fmt::Display for Version {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}.{}.{}", self.major, self.minor, self.patch)
    }
}

#[derive(Debug, PartialEq, Eq)]
pub struct Release {
    pub version: Version,
    pub url: String,
    pub bundle: Option<Bundle>,
    /// The download URL of `ultrasound-setup.exe`, when the release has one.
    pub installer: Option<String>,
}

#[derive(Debug, PartialEq, Eq)]
pub struct Bundle {
    pub name: String,
    pub url: String,
    /// `None` when the asset has no usable `sha256:` digest; such a bundle is never installed.
    pub sha256: Option<[u8; 32]>,
}

#[derive(Deserialize)]
struct ApiRelease {
    tag_name: String,
    html_url: Option<String>,
    #[serde(default)]
    draft: bool,
    #[serde(default)]
    prerelease: bool,
    #[serde(default)]
    assets: Vec<ApiAsset>,
}

#[derive(Deserialize)]
struct ApiAsset {
    name: String,
    browser_download_url: String,
    digest: Option<String>,
}

pub fn bundle_name(version: Version) -> String {
    format!("ultrasound-{version}-win-x64.zip")
}

const INSTALLER_NAME: &str = "ultrasound-setup.exe";

/// Parses a `releases/latest` response. Drafts, pre-releases and non-`X.Y.Z` tags are rejected.
pub fn parse_release(json: &str) -> Result<Release, String> {
    let api: ApiRelease = serde_json::from_str(json).map_err(|e| format!("invalid release JSON: {e}"))?;
    if api.draft || api.prerelease {
        return Err("not a full release".into());
    }
    let version = Version::parse(&api.tag_name).ok_or_else(|| format!("unsupported tag '{}'", api.tag_name))?;
    let name = bundle_name(version);
    let installer = api.assets.iter().find(|a| a.name == INSTALLER_NAME).map(|a| a.browser_download_url.clone());
    let bundle = api.assets.into_iter().find(|a| a.name == name).map(|a| Bundle {
        sha256: a.digest.as_deref().and_then(parse_digest),
        name: a.name,
        url: a.browser_download_url,
    });
    let url = api
        .html_url
        .filter(|u| !u.is_empty())
        .unwrap_or_else(|| format!("{RELEASES_URL}v{version}"));
    Ok(Release { version, url, bundle, installer })
}

/// Parses an asset digest of the form `sha256:<64 hex digits>`.
pub fn parse_digest(text: &str) -> Option<[u8; 32]> {
    let hex = text.trim().strip_prefix("sha256:")?;
    if hex.len() != 64 {
        return None;
    }
    let mut out = [0u8; 32];
    for (i, byte) in out.iter_mut().enumerate() {
        *byte = u8::from_str_radix(hex.get(i * 2..i * 2 + 2)?, 16).ok()?;
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v(text: &str) -> Version {
        Version::parse(text).unwrap()
    }

    #[test]
    fn versions_parse_and_compare_numerically() {
        assert_eq!(v("v0.3.1").to_string(), "0.3.1");
        assert!(v("0.3.10") > v("0.3.9"));
        assert!(v("1.0.0") > v("0.99.99"));
        assert!(v("0.4.0") > v("0.3.99"));
        assert_eq!(v("v1.2.3"), v("1.2.3"));
        for bad in ["", "1", "1.2", "1.2.3.4", "1.2.x", "1.2.3-beta.1", "1.2.3+build", "v", "1..3", "-1.2.3", "+1.2.3"] {
            assert_eq!(Version::parse(bad), None, "{bad}");
        }
        assert_eq!(Version::current().to_string(), env!("CARGO_PKG_VERSION"));
    }

    #[test]
    fn digests_parse_only_full_sha256() {
        let hex = "00ff".repeat(16);
        let parsed = parse_digest(&format!("sha256:{hex}")).unwrap();
        assert_eq!(parsed[0], 0x00);
        assert_eq!(parsed[1], 0xff);
        assert_eq!(parse_digest(&format!("sha256:{}", hex.to_uppercase())), Some(parsed));
        assert_eq!(parse_digest(&hex), None);
        assert_eq!(parse_digest(&format!("sha512:{hex}")), None);
        assert_eq!(parse_digest(&format!("sha256:{}", &hex[1..])), None);
        assert_eq!(parse_digest(&format!("sha256:{}g", &hex[1..])), None);
        assert_eq!(parse_digest("sha256:"), None);
    }

    #[test]
    fn release_json_finds_the_contract_bundle() {
        let hex = "ab".repeat(32);
        let json = format!(
            r#"{{"tag_name":"v0.3.2","html_url":"https://example.invalid/r","draft":false,"prerelease":false,
            "assets":[{{"name":"ultrasound-setup.exe","browser_download_url":"https://x/setup","digest":null}},
                      {{"name":"ultrasound-0.3.2-win-x64.zip","browser_download_url":"https://x/zip","digest":"sha256:{hex}"}}],
            "body":"notes"}}"#
        );
        let release = parse_release(&json).unwrap();
        assert_eq!(release.version, v("0.3.2"));
        assert_eq!(release.url, "https://example.invalid/r");
        let bundle = release.bundle.unwrap();
        assert_eq!(bundle.name, "ultrasound-0.3.2-win-x64.zip");
        assert_eq!(bundle.url, "https://x/zip");
        assert_eq!(bundle.sha256, Some([0xab; 32]));
        assert_eq!(release.installer.as_deref(), Some("https://x/setup"));
    }

    #[test]
    fn release_json_edge_cases() {
        // The v0.3.1 layout: a differently named asset is simply not a bundle.
        let old = r#"{"tag_name":"v0.3.1","assets":[{"name":"Ultrasound.v0.3.1.zip","browser_download_url":"u","digest":"sha256:00"}]}"#;
        let release = parse_release(old).unwrap();
        assert_eq!(release.bundle, None);
        assert_eq!(release.installer, None);
        assert_eq!(release.url, "https://github.com/Blakintosh/ultrasound/releases/tag/v0.3.1");

        let no_digest = r#"{"tag_name":"v1.0.0","assets":[{"name":"ultrasound-1.0.0-win-x64.zip","browser_download_url":"u"}]}"#;
        assert_eq!(parse_release(no_digest).unwrap().bundle.unwrap().sha256, None);

        assert!(parse_release(r#"{"tag_name":"v1.0.0","prerelease":true}"#).is_err());
        assert!(parse_release(r#"{"tag_name":"v1.0.0","draft":true}"#).is_err());
        assert!(parse_release(r#"{"tag_name":"nightly"}"#).is_err());
        assert!(parse_release(r#"{"message":"API rate limit exceeded"}"#).is_err());
        assert!(parse_release("<html>").is_err());
    }
}
