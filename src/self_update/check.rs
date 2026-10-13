//! `snd_convert.exe --check-update [--json] [--refresh]`: ask the release feed whether a newer Ultrasound exists,
//! for a program with a window (Blackbird) that offers the update itself. The answer has the shape Dragon and the
//! Stock Library give (`docs/release-contract.md` of gscode-installer): `current`, `latest`, `update_available`,
//! `url`, `digest`, `notes`, `bundle_url`, `installer_url`, `checked_at`, `cached`, `error`.
//!
//! It is its own command: no sound build runs it, it never installs anything, it always exits 0 (trouble is part
//! of the answer) and a slow or dead network costs at most 6 s. The answer is cached for a day (an hour after a
//! failure) in `%LOCALAPPDATA%\BO3ModTools\updates\ultrasound.json`, keyed by the feed URL. The last good release
//! survives a failed check, and the answer is judged against the running version every time.

use super::release::{Release, Version, parse_release};
use super::{FEED_OVERRIDE, LATEST_URL, install};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::fs;
use std::path::{Path, PathBuf};

const OK_SECS: u64 = 24 * 60 * 60;
const FAILED_SECS: u64 = 60 * 60;

/// What the cache file holds: the last good release and how the last attempt went.
#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct Cache {
    #[serde(default)]
    feed: String,
    #[serde(default)]
    checked_at: u64,
    #[serde(default)]
    error: Option<String>,
    #[serde(default)]
    latest: Option<String>,
    #[serde(default)]
    url: Option<String>,
    #[serde(default)]
    digest: Option<String>,
    #[serde(default)]
    notes: String,
    #[serde(default)]
    bundle_url: Option<String>,
    #[serde(default)]
    installer_url: Option<String>,
}

fn cache_path(dir: &Path) -> PathBuf {
    dir.join("ultrasound.json")
}

fn load(dir: &Path) -> Option<Cache> {
    serde_json::from_str(&fs::read_to_string(cache_path(dir)).ok()?).ok()
}

fn save(dir: &Path, cache: &Cache) {
    if fs::create_dir_all(dir).is_err() {
        return;
    }
    if let Ok(text) = serde_json::to_string(cache) {
        let tmp = cache_path(dir).with_extension("json.tmp");
        if fs::write(&tmp, text).is_ok() {
            let _ = fs::rename(&tmp, cache_path(dir));
        }
    }
}

fn digest_text(sha: &[u8; 32]) -> String {
    let hex: String = sha.iter().map(|b| format!("{b:02x}")).collect();
    format!("sha256:{hex}")
}

fn with_release(mut cache: Cache, release: Release) -> Cache {
    cache.error = None;
    cache.latest = Some(release.version.to_string());
    cache.url = Some(release.url);
    cache.notes = release.notes;
    cache.installer_url = release.installer;
    match release.bundle {
        Some(bundle) => {
            cache.digest = bundle.sha256.as_ref().map(digest_text);
            cache.bundle_url = Some(bundle.url);
        }
        None => {
            cache.digest = None;
            cache.bundle_url = None;
        }
    }
    cache
}

/// The cache to answer from: the stored one when it is fresh, else the result of `fetch` (the feed's JSON text).
/// Returns it with whether it came from the cache.
fn resolve(
    dir: &Path,
    feed: &str,
    now: u64,
    refresh: bool,
    fetch: impl FnOnce() -> Result<String, String>,
) -> (Cache, bool) {
    let stored = load(dir).filter(|c| c.feed == feed);
    if let (Some(c), false) = (&stored, refresh) {
        // A check time in the future means the clock moved back: stale.
        let limit = if c.error.is_some() {
            FAILED_SECS
        } else {
            OK_SECS
        };
        if matches!(now.checked_sub(c.checked_at), Some(age) if age < limit) {
            return (c.clone(), true);
        }
    }
    let mut base = stored.unwrap_or_default();
    base.feed = feed.to_string();
    base.checked_at = now;
    let fresh = match fetch().and_then(|text| parse_release(&text)) {
        Ok(release) => with_release(base, release),
        Err(e) => {
            base.error = Some(e);
            base
        }
    };
    save(dir, &fresh);
    (fresh, false)
}

fn answer(cache: &Cache, current: Version, cached: bool) -> Value {
    let latest = cache.latest.as_deref().and_then(Version::parse);
    let newer = latest.is_some_and(|l| l > current);
    json!({
        "current": current.to_string(),
        "latest": latest.map(|l| l.to_string()),
        "update_available": newer,
        "url": cache.url,
        "digest": cache.digest,
        "notes": cache.notes,
        "bundle_url": cache.bundle_url,
        "installer_url": cache.installer_url,
        "checked_at": cache.checked_at,
        "cached": cached,
        "error": cache.error,
    })
}

fn sentence(a: &Value) -> String {
    let current = a["current"].as_str().unwrap_or("");
    if let Some(latest) = a["latest"].as_str()
        && a["update_available"] == true
    {
        return format!("Ultrasound {latest} is available (you have {current}).");
    }
    match a["error"].as_str() {
        Some(e) => format!("Couldn't check for an Ultrasound update: {e}"),
        None => format!("Ultrasound {current} is up to date."),
    }
}

fn updates_dir() -> Option<PathBuf> {
    let local = std::env::var_os("LOCALAPPDATA").filter(|v| !v.is_empty())?;
    Some(Path::new(&local).join("BO3ModTools").join("updates"))
}

fn fetch_feed(feed: &str, protocols: &str) -> Result<String, String> {
    let tmp = std::env::temp_dir().join(format!("ultrasound-check-{}.json", std::process::id()));
    let result = install::download_limited(feed, &tmp, protocols, 3, 6, 1 << 20, true)
        .and_then(|()| fs::read_to_string(&tmp).map_err(|e| e.to_string()));
    let _ = fs::remove_file(&tmp);
    result.map_err(|e| format!("the release feed didn't answer ({e})"))
}

/// The command: prints the answer and never fails.
pub fn run(json: bool, refresh: bool) {
    let (feed, protocols) = match std::env::var(FEED_OVERRIDE) {
        Ok(feed) if !feed.is_empty() => (feed.replace("{id}", "ultrasound"), "=https,http,file"),
        _ => (LATEST_URL.to_string(), "=https"),
    };
    let now = super::now();
    let a = match updates_dir() {
        Some(dir) => {
            let (cache, cached) =
                resolve(&dir, &feed, now, refresh, || fetch_feed(&feed, protocols));
            answer(&cache, Version::current(), cached)
        }
        None => {
            let cache = Cache {
                checked_at: now,
                error: Some("no profile folder to keep the cache in".into()),
                ..Cache::default()
            };
            answer(&cache, Version::current(), false)
        }
    };
    if json {
        println!("{a}");
    } else {
        println!("{}", sentence(&a));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("us-check-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        dir
    }

    fn feed_json(tag: &str) -> String {
        let hex = "cd".repeat(32);
        format!(
            r#"{{"tag_name":"{tag}","html_url":"https://example.invalid/r","body":"what is new",
            "assets":[{{"name":"ultrasound-setup.exe","browser_download_url":"https://x/setup"}},
            {{"name":"ultrasound-{v}-win-x64.zip","browser_download_url":"https://x/zip","digest":"sha256:{hex}"}}]}}"#,
            v = tag.trim_start_matches('v')
        )
    }

    #[test]
    fn newer_release_answers_in_the_contract_shape() {
        let dir = temp("shape");
        let (cache, cached) = resolve(&dir, "f", 1000, false, || Ok(feed_json("v9.0.0")));
        assert!(!cached);
        let a = answer(&cache, Version::current(), cached);
        assert_eq!(a["current"], env!("CARGO_PKG_VERSION"));
        assert_eq!(a["latest"], "9.0.0");
        assert_eq!(a["update_available"], true);
        assert_eq!(a["url"], "https://example.invalid/r");
        assert_eq!(a["digest"], format!("sha256:{}", "cd".repeat(32)));
        assert_eq!(a["notes"], "what is new");
        assert_eq!(a["bundle_url"], "https://x/zip");
        assert_eq!(a["installer_url"], "https://x/setup");
        assert_eq!(a["checked_at"], 1000);
        assert_eq!(a["cached"], false);
        assert!(a["error"].is_null());
        assert!(sentence(&a).contains("9.0.0 is available"));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn same_or_older_is_not_an_update() {
        let dir = temp("same");
        let tag = format!("v{}", env!("CARGO_PKG_VERSION"));
        let (cache, c) = resolve(&dir, "f", 1, false, || Ok(feed_json(&tag)));
        let a = answer(&cache, Version::current(), c);
        assert_eq!(a["update_available"], false);
        assert!(sentence(&a).contains("up to date"));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn answers_from_the_cache_for_a_day_unless_refreshed() {
        let dir = temp("cache");
        resolve(&dir, "f", 1000, false, || Ok(feed_json("v9.0.0")));
        let boom = || -> Result<String, String> { panic!("must not ask the feed") };
        let (c, cached) = resolve(&dir, "f", 1000 + OK_SECS - 1, false, boom);
        assert!(cached);
        assert_eq!(c.latest.as_deref(), Some("9.0.0"));
        let (c, cached) = resolve(&dir, "f", 1000 + OK_SECS, false, || Ok(feed_json("v9.1.0")));
        assert!(!cached);
        assert_eq!(c.latest.as_deref(), Some("9.1.0"));
        let (c, cached) = resolve(&dir, "f", 1001 + OK_SECS, true, || Ok(feed_json("v9.2.0")));
        assert!(!cached);
        assert_eq!(c.latest.as_deref(), Some("9.2.0"));
        let (c, cached) = resolve(&dir, "other", 1002 + OK_SECS, false, || {
            Ok(feed_json("v9.3.0"))
        });
        assert!(!cached);
        assert_eq!(c.latest.as_deref(), Some("9.3.0"));
        let (_, cached) = resolve(&dir, "other", 5, false, || Ok(feed_json("v9.4.0")));
        assert!(!cached, "a clock set back is stale");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_failure_keeps_the_last_good_release_and_is_retried_after_an_hour() {
        let dir = temp("fail");
        resolve(&dir, "f", 1000, false, || Ok(feed_json("v9.0.0")));
        let t = 1000 + OK_SECS;
        let (c, cached) = resolve(&dir, "f", t, false, || Err("offline".into()));
        assert!(!cached);
        let a = answer(&c, Version::current(), cached);
        assert_eq!(a["latest"], "9.0.0");
        assert_eq!(a["update_available"], true);
        assert_eq!(a["error"], "offline");
        let boom = || -> Result<String, String> { panic!("too soon") };
        let (_, cached) = resolve(&dir, "f", t + FAILED_SECS - 1, false, boom);
        assert!(cached);
        let (c, cached) = resolve(&dir, "f", t + FAILED_SECS, false, || {
            Ok(feed_json("v9.0.0"))
        });
        assert!(!cached);
        assert!(c.error.is_none());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_dead_feed_with_nothing_cached_answers_with_an_error() {
        let dir = temp("dead");
        let (c, cached) = resolve(&dir, "f", 10, false, || Err("down".into()));
        let a = answer(&c, Version::current(), cached);
        assert!(a["latest"].is_null());
        assert_eq!(a["update_available"], false);
        assert_eq!(a["error"], "down");
        assert!(sentence(&a).starts_with("Couldn't check"));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn an_unusable_feed_is_an_error_not_a_crash() {
        let dir = temp("junk");
        let (c, _) = resolve(&dir, "f", 10, false, || Ok("<html>".into()));
        assert!(c.error.is_some());
        let _ = fs::remove_dir_all(&dir);
    }
}
