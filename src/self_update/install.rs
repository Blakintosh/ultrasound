//! Downloading, verifying, extracting and swapping in a release bundle.

use sha2::{Digest, Sha256};
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

/// The bundle's files, all under `sound/`, each installed under its own name beside the exe.
pub const FILES: [&str; 2] = ["snd_convert.exe", "libFLAC.dll"];

fn sibling(path: &Path, suffix: &str) -> PathBuf {
    let mut name = path.as_os_str().to_owned();
    name.push(suffix);
    PathBuf::from(name)
}

/// Deletes `<file>.old` left by an earlier update. Failures (still in use) wait for a later run.
pub fn remove_leftovers(dir: &Path) {
    for file in FILES {
        let _ = fs::remove_file(sibling(&dir.join(file), ".old"));
    }
}

fn system_tool(name: &str) -> PathBuf {
    let root = std::env::var_os("SystemRoot").unwrap_or_else(|| r"C:\Windows".into());
    Path::new(&root).join("System32").join(name)
}

fn run_tool(command: &mut Command, limit: Duration) -> Result<(), String> {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        command.creation_flags(CREATE_NO_WINDOW);
    }
    let program = Path::new(command.get_program())
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let mut child = command
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| format!("{program}: {e}"))?;
    let deadline = Instant::now() + limit;
    loop {
        match child.try_wait() {
            Ok(Some(status)) if status.success() => return Ok(()),
            Ok(Some(status)) => return Err(format!("{program} exited with {status}")),
            Ok(None) if Instant::now() < deadline => std::thread::sleep(Duration::from_millis(50)),
            Ok(None) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(format!("{program} timed out"));
            }
            Err(e) => return Err(format!("{program}: {e}")),
        }
    }
}

/// Fetches `url` into `out` with Windows' own curl.exe.
pub fn download(url: &str, out: &Path, protocols: &str, max_secs: u64, api: bool) -> Result<(), String> {
    let mut command = Command::new(system_tool("curl.exe"));
    command
        .args(["--silent", "--fail", "--location", "--proto", protocols, "--proto-redir", protocols])
        .args(["--connect-timeout", "10", "--max-time", &max_secs.to_string()])
        .args(["--max-filesize", "268435456"])
        .args(["--user-agent", concat!("ultrasound/", env!("CARGO_PKG_VERSION"))]);
    if api {
        command.args(["--header", "Accept: application/vnd.github+json"]);
    }
    command.arg("--output").arg(out).arg(url);
    run_tool(&mut command, Duration::from_secs(max_secs + 15))
}

pub fn sha256_file(path: &Path) -> std::io::Result<[u8; 32]> {
    let mut file = fs::File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buffer = vec![0u8; 1 << 16];
    loop {
        let read = file.read(&mut buffer)?;
        if read == 0 {
            return Ok(hasher.finalize().into());
        }
        hasher.update(&buffer[..read]);
    }
}

/// Extracts the bundle with Windows' own tar.exe (bsdtar, which reads zip and refuses absolute
/// and `..` paths) and returns the staged path of each of [`FILES`].
pub fn extract(zip: &Path, into: &Path) -> Result<Vec<PathBuf>, String> {
    fs::create_dir_all(into).map_err(|e| format!("extract: {e}"))?;
    let mut command = Command::new(system_tool("tar.exe"));
    command.arg("-x").arg("-f").arg(zip).arg("-C").arg(into);
    run_tool(&mut command, Duration::from_secs(60))?;
    FILES
        .iter()
        .map(|file| {
            let path = into.join("sound").join(file);
            if path.is_file() {
                Ok(path)
            } else {
                Err(format!("bundle has no sound/{file}"))
            }
        })
        .collect()
}

enum Step {
    MovedAside(PathBuf),
    Installed(PathBuf),
}

/// Swaps each staged file in beside the running exe: delete `<file>.old`, rename the current file
/// to `<file>.old`, move the new one in. Any failure puts every file back.
pub fn swap(dir: &Path, staged: &[PathBuf]) -> Result<(), String> {
    let targets: Vec<PathBuf> = FILES.iter().map(|f| dir.join(f)).collect();
    let news: Vec<PathBuf> = targets.iter().map(|t| sibling(t, ".new")).collect();
    let clean_news = || {
        for new in &news {
            let _ = fs::remove_file(new);
        }
    };

    // Copy first: the staging folder may be on another volume, and this proves the folder is writable
    // before anything installed is touched.
    for (source, new) in staged.iter().zip(&news) {
        if let Err(e) = fs::copy(source, new) {
            clean_news();
            return Err(format!("cannot write {}: {e}", new.display()));
        }
    }
    for target in &targets {
        let old = sibling(target, ".old");
        if let Err(e) = fs::remove_file(&old)
            && e.kind() != std::io::ErrorKind::NotFound
        {
            clean_news();
            return Err(format!("cannot remove {}: {e}", old.display()));
        }
    }

    let mut steps = Vec::new();
    let mut failure = None;
    for (target, new) in targets.iter().zip(&news) {
        if target.exists() {
            if let Err(e) = fs::rename(target, sibling(target, ".old")) {
                failure = Some(format!("cannot move {} aside: {e}", target.display()));
                break;
            }
            steps.push(Step::MovedAside(target.clone()));
        }
        if let Err(e) = fs::rename(new, target) {
            failure = Some(format!("cannot move in {}: {e}", target.display()));
            break;
        }
        steps.push(Step::Installed(target.clone()));
    }
    let Some(failure) = failure else {
        return Ok(());
    };

    let mut unrestored = Vec::new();
    for step in steps.iter().rev() {
        let result = match step {
            Step::Installed(target) => fs::rename(target, sibling(target, ".new")),
            Step::MovedAside(target) => fs::rename(sibling(target, ".old"), target),
        };
        if let Err(e) = result {
            let (Step::Installed(target) | Step::MovedAside(target)) = step;
            unrestored.push(format!("{}: {e}", target.display()));
        }
    }
    clean_news();
    if unrestored.is_empty() {
        Err(format!("{failure}; previous files restored"))
    } else {
        Err(format!("{failure}; could not restore {}", unrestored.join(", ")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture(name: &str) -> (PathBuf, Vec<PathBuf>) {
        let root = std::env::temp_dir().join(format!("ultrasound-swap-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        let dir = root.join("sound");
        let staged = root.join("staged");
        fs::create_dir_all(&dir).unwrap();
        fs::create_dir_all(&staged).unwrap();
        let mut sources = Vec::new();
        for file in FILES {
            fs::write(dir.join(file), format!("old {file}")).unwrap();
            fs::write(staged.join(file), format!("new {file}")).unwrap();
            sources.push(staged.join(file));
        }
        (dir, sources)
    }

    fn contents(dir: &Path) -> Vec<(String, String)> {
        let mut names: Vec<_> = fs::read_dir(dir)
            .unwrap()
            .map(|e| {
                let e = e.unwrap();
                (e.file_name().to_string_lossy().into_owned(), fs::read_to_string(e.path()).unwrap())
            })
            .collect();
        names.sort();
        names
    }

    #[test]
    fn swap_keeps_old_files_beside_new_ones() {
        let (dir, staged) = fixture("ok");
        fs::write(dir.join("libFLAC.dll.old"), "stale").unwrap();
        swap(&dir, &staged).unwrap();
        assert_eq!(
            contents(&dir),
            [
                ("libFLAC.dll", "new libFLAC.dll"),
                ("libFLAC.dll.old", "old libFLAC.dll"),
                ("snd_convert.exe", "new snd_convert.exe"),
                ("snd_convert.exe.old", "old snd_convert.exe"),
            ]
            .map(|(a, b)| (a.to_string(), b.to_string()))
        );
        remove_leftovers(&dir);
        assert_eq!(contents(&dir).len(), 2);
        fs::remove_dir_all(dir.parent().unwrap()).unwrap();
    }

    #[test]
    #[cfg(windows)]
    fn locked_file_rolls_back_every_file() {
        use std::os::windows::fs::OpenOptionsExt;
        let (dir, staged) = fixture("locked");
        let before = contents(&dir);
        let locked = fs::OpenOptions::new()
            .read(true)
            .share_mode(1) // FILE_SHARE_READ only: no rename
            .open(dir.join("libFLAC.dll"))
            .unwrap();
        let error = swap(&dir, &staged).unwrap_err();
        assert!(error.contains("previous files restored"), "{error}");
        drop(locked);
        assert_eq!(contents(&dir), before);
        fs::remove_dir_all(dir.parent().unwrap()).unwrap();
    }

    #[test]
    fn missing_staged_file_touches_nothing() {
        let (dir, mut staged) = fixture("missing");
        let before = contents(&dir);
        staged[1] = staged[1].with_file_name("absent.dll");
        assert!(swap(&dir, &staged).unwrap_err().starts_with("cannot write"));
        assert_eq!(contents(&dir), before);
        fs::remove_dir_all(dir.parent().unwrap()).unwrap();
    }

    #[test]
    fn sha256_matches_known_vector() {
        let dir = std::env::temp_dir().join(format!("ultrasound-sha-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("abc");
        fs::write(&path, b"abc").unwrap();
        let expected = super::super::release::parse_digest(
            "sha256:ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad",
        )
        .unwrap();
        assert_eq!(sha256_file(&path).unwrap(), expected);
        fs::remove_dir_all(dir).unwrap();
    }
}
