//! Background self-update.
//!
//! After a successful run, at most once a day, the installed `snd_convert.exe` starts a detached copy
//! of itself with `--update` and exits. That process checks the latest release, verifies and swaps in
//! the bundle, and records the outcome in a state file; the next run prints it. Nothing here may fail
//! or slow down a sound build, so every error is swallowed.

mod detach;
mod install;
mod release;
mod state;

use release::{Version, bundle_name, parse_release};
use sha2::{Digest, Sha256};
use state::{Notice, State};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

const LATEST_URL: &str = "https://api.github.com/repos/Blakintosh/ultrasound/releases/latest";
const FEED_OVERRIDE: &str = "GSCODE_UPDATE_FEED";
const UPDATE_FLAG: &str = "--update";

/// An installed copy: `snd_convert.exe` in the game's `sound` folder. Other names (a development
/// build in `target/`) never update themselves.
struct Install {
    exe: PathBuf,
    dir: PathBuf,
    data: PathBuf,
}

impl Install {
    fn locate() -> Option<Install> {
        let exe = std::env::current_exe().ok()?;
        let name = exe.file_name()?.to_str()?;
        if !name.eq_ignore_ascii_case(install::FILES[0]) {
            return None;
        }
        let dir = exe.parent()?.to_path_buf();
        let local = std::env::var_os("LOCALAPPDATA").filter(|v| !v.is_empty())?;
        let key = dir.to_string_lossy().to_lowercase();
        let id: String = Sha256::digest(key.as_bytes())[..8].iter().map(|b| format!("{b:02x}")).collect();
        let data = Path::new(&local).join("gscode").join("ultrasound").join(id);
        Some(Install { exe, dir, data })
    }

    fn state_path(&self) -> PathBuf {
        self.data.join("state.json")
    }

    fn load(&self) -> State {
        let mut state = State::load(&self.state_path());
        state.install = self.dir.display().to_string();
        state
    }

    fn save(&self, state: &State) {
        if fs::create_dir_all(&self.data).is_ok() {
            let _ = state.save(&self.state_path());
        }
    }
}

fn now() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

/// On every start: remove files left by the last swap and report its outcome.
pub fn on_start() {
    if let Some(dir) = std::env::current_exe().ok().as_deref().and_then(Path::parent) {
        install::remove_leftovers(dir);
    }
    let Some(install) = Install::locate() else { return };
    let mut state = install.load();
    let (line, changed) = state.take_notice(Version::current());
    if let Some(line) = line {
        println!("{line}");
    }
    if changed {
        install.save(&state);
    }
}

/// After a successful run: start the background check if one is due. Never waits for it.
pub fn after_success() {
    let Some(install) = Install::locate() else { return };
    let mut state = install.load();
    let now = now();
    if !state.check_due(now) {
        return;
    }
    state.last_check = now;
    install.save(&state);
    let _ = detach::spawn_detached(&install.exe, UPDATE_FLAG, &install.dir);
}

enum Outcome {
    /// Offline, rate-limited, not found, or nothing usable published: say nothing.
    Silent(String),
    UpToDate,
    Installed { version: Version, url: String },
    /// A newer release exists but could not be installed here.
    Unavailable { version: Version, url: String, reason: String },
}

/// The `--update` process. Writes nothing to stdout/stderr; the outcome goes to the state file.
pub fn run_update() {
    let Some(install) = Install::locate() else { return };
    if fs::create_dir_all(&install.data).is_err() {
        return;
    }
    let Some(_lock) = lock(&install.data.join("update.lock")) else { return };
    install::remove_leftovers(&install.dir);
    let work = install.data.join("download");
    let _ = fs::remove_dir_all(&work);
    let outcome = match fs::create_dir_all(&work) {
        Ok(()) => check_and_install(&install, &work),
        Err(e) => Outcome::Silent(format!("cannot create {}: {e}", work.display())),
    };
    let _ = fs::remove_dir_all(&work);

    let current = Version::current();
    let mut state = install.load();
    match outcome {
        Outcome::Silent(reason) => state.last_result = Some(reason),
        Outcome::UpToDate => {
            state.last_result = Some(format!("up to date ({current})"));
            if matches!(state.notice, Some(Notice::Available { .. })) {
                state.notice = None;
            }
        }
        Outcome::Installed { version, url } => {
            state.last_result = Some(format!("installed {version}"));
            state.notice = Some(Notice::Updated {
                version: version.to_string(),
                previous: current.to_string(),
                url,
            });
        }
        Outcome::Unavailable { version, url, reason } => {
            state.last_result = Some(format!("{version} not installed: {reason}"));
            state.notice = Some(Notice::Available { version: version.to_string(), url });
        }
    }
    install.save(&state);
}

/// An exclusive lock held until the handle drops (or the process ends).
fn lock(path: &Path) -> Option<fs::File> {
    let mut options = fs::OpenOptions::new();
    options.write(true).create(true).truncate(false);
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        options.share_mode(0);
    }
    options.open(path).ok()
}

fn check_and_install(install: &Install, work: &Path) -> Outcome {
    let (feed, protocols) = match std::env::var(FEED_OVERRIDE) {
        Ok(feed) if !feed.is_empty() => (feed, "=https,http,file"),
        _ => (LATEST_URL.to_string(), "=https"),
    };
    let latest = work.join("latest.json");
    if let Err(e) = install::download(&feed, &latest, protocols, 30, true) {
        return Outcome::Silent(format!("release check failed: {e}"));
    }
    let release = match fs::read_to_string(&latest).map_err(|e| e.to_string()).and_then(|t| parse_release(&t)) {
        Ok(release) => release,
        Err(e) => return Outcome::Silent(format!("release check failed: {e}")),
    };
    if release.version <= Version::current() {
        return Outcome::UpToDate;
    }
    let unavailable = |reason: String| Outcome::Unavailable {
        version: release.version,
        url: release.url.clone(),
        reason,
    };
    let Some(bundle) = &release.bundle else {
        return unavailable(format!("no {} asset", bundle_name(release.version)));
    };
    let Some(expected) = bundle.sha256 else {
        return unavailable("bundle has no sha256 digest".into());
    };
    let zip = work.join(&bundle.name);
    if let Err(e) = install::download(&bundle.url, &zip, protocols, 300, false) {
        return Outcome::Silent(format!("download failed: {e}"));
    }
    match install::sha256_file(&zip) {
        Ok(actual) if actual == expected => {}
        Ok(_) => return unavailable("bundle digest mismatch".into()),
        Err(e) => return unavailable(format!("cannot read bundle: {e}")),
    }
    let staged = match install::extract(&zip, &work.join("bundle")) {
        Ok(staged) => staged,
        Err(e) => return unavailable(e),
    };
    match install::swap(&install.dir, &staged) {
        Ok(()) => Outcome::Installed { version: release.version, url: release.url.clone() },
        Err(e) => unavailable(e),
    }
}
