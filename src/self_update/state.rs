//! Per-install update state, kept outside the game folder.

use super::release::Version;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;

pub const CHECK_INTERVAL_SECS: u64 = 24 * 60 * 60;

#[derive(Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct State {
    /// The install folder this state belongs to, for anyone reading the file.
    #[serde(default)]
    pub install: String,
    /// Unix time a background check was last started.
    #[serde(default)]
    pub last_check: u64,
    /// What the last background check did, for diagnosis only.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_result: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub notice: Option<Notice>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Notice {
    Updated { version: String, previous: String, url: String },
    Available { version: String, url: String },
}

impl State {
    /// A missing or unreadable file is a fresh state.
    pub fn load(path: &Path) -> State {
        fs::read_to_string(path)
            .ok()
            .and_then(|text| serde_json::from_str(&text).ok())
            .unwrap_or_default()
    }

    pub fn save(&self, path: &Path) -> std::io::Result<()> {
        let tmp = path.with_extension("json.tmp");
        let text = serde_json::to_string_pretty(self).map_err(std::io::Error::other)?;
        fs::write(&tmp, text)?;
        fs::rename(&tmp, path)
    }

    pub fn check_due(&self, now: u64) -> bool {
        // A check time in the future means the clock moved; check rather than wait it out.
        now < self.last_check || now - self.last_check >= CHECK_INTERVAL_SECS
    }

    /// The line to print on this run, if any. "Updated" shows once; "available" repeats until the
    /// running version catches up. Returns whether the state changed.
    pub fn take_notice(&mut self, current: Version) -> (Option<String>, bool) {
        match self.notice.clone() {
            None => (None, false),
            Some(Notice::Updated { version, previous, url }) => {
                self.notice = None;
                (Some(format!("Ultrasound updated to {version} (was {previous}): {url}")), true)
            }
            Some(Notice::Available { version, url }) => {
                if Version::parse(&version).is_some_and(|v| v > current) {
                    (Some(format!("Ultrasound {version} is available: {url}")), false)
                } else {
                    self.notice = None;
                    (None, true)
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("ultrasound-state-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn state_round_trips_and_tolerates_bad_files() {
        let dir = temp_dir("roundtrip");
        let path = dir.join("state.json");
        assert_eq!(State::load(&path), State::default());

        let state = State {
            install: r"C:\Game\sound".into(),
            last_check: 1_700_000_000,
            last_result: Some("installed 0.3.2".into()),
            notice: Some(Notice::Updated {
                version: "0.3.2".into(),
                previous: "0.3.1".into(),
                url: "https://example.invalid".into(),
            }),
        };
        state.save(&path).unwrap();
        assert_eq!(State::load(&path), state);
        assert!(!dir.join("state.json.tmp").exists());

        let available = State {
            notice: Some(Notice::Available { version: "1.0.0".into(), url: "u".into() }),
            ..State::default()
        };
        available.save(&path).unwrap();
        assert_eq!(State::load(&path), available);

        fs::write(&path, "{not json").unwrap();
        assert_eq!(State::load(&path), State::default());
        fs::write(&path, r#"{"last_check":5,"future_field":true}"#).unwrap();
        assert_eq!(State::load(&path).last_check, 5);
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn checks_at_most_daily() {
        let state = State { last_check: 1_000_000, ..State::default() };
        assert!(!state.check_due(1_000_000));
        assert!(!state.check_due(1_000_000 + CHECK_INTERVAL_SECS - 1));
        assert!(state.check_due(1_000_000 + CHECK_INTERVAL_SECS));
        assert!(state.check_due(999_999));
        assert!(State::default().check_due(1_700_000_000));
    }

    #[test]
    fn notices_print_once_or_until_resolved() {
        let current = Version::parse("0.3.1").unwrap();
        let mut state = State {
            notice: Some(Notice::Updated { version: "0.3.1".into(), previous: "0.3.0".into(), url: "u".into() }),
            ..State::default()
        };
        assert_eq!(
            state.take_notice(current),
            (Some("Ultrasound updated to 0.3.1 (was 0.3.0): u".into()), true)
        );
        assert_eq!(state.take_notice(current), (None, false));

        state.notice = Some(Notice::Available { version: "0.3.2".into(), url: "u".into() });
        for _ in 0..2 {
            assert_eq!(state.take_notice(current), (Some("Ultrasound 0.3.2 is available: u".into()), false));
        }
        let newer = Version::parse("0.3.2").unwrap();
        assert_eq!(state.take_notice(newer), (None, true));
        assert_eq!(state.notice, None);
    }
}
