//! What this copy offers, for the other Mod Tools products.
//!
//! `--capabilities` prints it as one JSON line ([`capabilities_json`]); Dragon (bb-linker) asks before it carries stock
//! sounds. An installed copy also leaves it in the shared components registry,
//! `%LOCALAPPDATA%\BO3ModTools\components\<install hash>\ultrasound.json`, where Apex, Blackbird and the Stock
//! Library read which components an install has. The install hash is the one every product uses: FNV-1a 64 of the
//! lower-cased canonical install path (no `\\?\` prefix, `\` separators, no trailing separator), as 16 hex digits.
//!
//! `BBL_NO_REGISTRY=1` or `ULTRASOUND_NO_REGISTRY=1` leaves the registry alone (development and test runs).

use std::path::{Path, PathBuf};

use serde_json::{Value, json};

/// What this build can do beyond the Mod Tools' converter.
pub const FEATURES: &[&str] = &["stock-sounds"];

/// The `<zone>.stocksound.json` contract versions this build reads, inclusive.
pub const STOCK_SOUND_CONTRACT: (u32, u32) =
    (crate::stock_donor::CONTRACT, crate::stock_donor::CONTRACT);

fn base() -> Value {
    json!({
        "id": "ultrasound",
        "version": env!("CARGO_PKG_VERSION"),
        "features": FEATURES,
        "stock_sound_contract": [STOCK_SOUND_CONTRACT.0, STOCK_SOUND_CONTRACT.1],
    })
}

/// `snd_convert --capabilities`: one line of JSON.
pub fn capabilities_json() -> String {
    base().to_string()
}

/// FNV-1a 64 of the lower-cased canonical install path, 16 hex digits (the Stock Library's derivation).
pub fn install_hash(canonical: &str) -> String {
    let mut h: u64 = 0xcbf29ce484222325;
    for b in canonical
        .to_lowercase()
        .replace('/', "\\")
        .trim_end_matches('\\')
        .bytes()
    {
        h ^= b as u64;
        h = h.wrapping_mul(0x100000001b3);
    }
    format!("{h:016x}")
}

/// The install path as hashed: canonical, without the `\\?\` prefix.
pub fn canonical_install(install: &Path) -> Option<String> {
    let c = std::fs::canonicalize(install).ok()?;
    let s = c.to_string_lossy();
    Some(s.strip_prefix(r"\\?\").unwrap_or(&s).to_string())
}

/// The install an installed copy belongs to: `<install>\sound\snd_convert.exe`, beside `BlackOps3.exe` or `share`.
fn install_of(exe: &Path) -> Option<PathBuf> {
    let dir = exe.parent()?;
    let root = dir.parent()?;
    let in_game = dir.file_name()?.eq_ignore_ascii_case("sound")
        && (root.join("BlackOps3.exe").is_file() || root.join("share").is_dir());
    in_game.then(|| root.to_path_buf())
}

fn flag(name: &str) -> bool {
    std::env::var(name).is_ok_and(|v| !v.is_empty() && v != "0")
}

/// The entry's text for `exe` (pretty JSON and a newline, as the Stock Library writes its own).
fn entry_text(exe: &Path, meta: &std::fs::Metadata) -> String {
    let mtime = meta
        .modified()
        .ok()
        .and_then(|m| m.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let mut v = base();
    v["path"] = json!(exe.to_string_lossy());
    v["exe_size"] = json!(meta.len());
    v["exe_mtime"] = json!(mtime);
    // (the Stock Library and Blackbird read `capabilities` from every entry)
    v["capabilities"] = json!(FEATURES);
    format!("{}\n", serde_json::to_string_pretty(&v).unwrap_or_default())
}

/// Writes `ultrasound.json` into `root\<install hash>\` (temp file and rename; untouched when equal).
fn write_entry(root: &Path, install: &Path, exe: &Path) -> Option<PathBuf> {
    let canon = canonical_install(install)?;
    let dir = root.join(install_hash(&canon));
    let meta = std::fs::metadata(exe).ok()?;
    let text = entry_text(exe, &meta);
    let path = dir.join("ultrasound.json");
    if std::fs::read_to_string(&path).is_ok_and(|t| t == text) {
        return Some(path);
    }
    std::fs::create_dir_all(&dir).ok()?;
    let tmp = dir.join(format!("ultrasound.json.{}.tmp", std::process::id()));
    std::fs::write(&tmp, text).ok()?;
    if std::fs::rename(&tmp, &path).is_err() {
        let _ = std::fs::remove_file(&tmp);
        return None;
    }
    Some(path)
}

/// Records this copy in the registry when it is installed in a game folder. Never fails a run.
pub fn record_self() {
    if flag("BBL_NO_REGISTRY") || flag("ULTRASOUND_NO_REGISTRY") {
        return;
    }
    let Some(local) = std::env::var_os("LOCALAPPDATA").filter(|v| !v.is_empty()) else {
        return;
    };
    let Ok(exe) = std::env::current_exe() else {
        return;
    };
    let Some(install) = install_of(&exe) else {
        return;
    };
    let root = Path::new(&local).join("BO3ModTools").join("components");
    let _ = write_entry(&root, &install, &exe);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hash_matches_the_other_products() {
        // (the same vector is pinned in Dragon's and the Stock Library's tests)
        assert_eq!(
            install_hash(r"I:\SteamLibrary\steamapps\common\Call of Duty Black Ops III"),
            install_hash(r"i:/steamlibrary/steamapps/common/call of duty black ops iii\")
        );
        assert_eq!(install_hash(r"C:\BO3"), "b0d554d56a3c7164");
    }

    #[test]
    fn capabilities_name_stock_sounds_and_the_contract() {
        let v: Value = serde_json::from_str(&capabilities_json()).unwrap();
        assert_eq!(v["id"], "ultrasound");
        assert_eq!(v["version"], env!("CARGO_PKG_VERSION"));
        assert!(
            v["features"]
                .as_array()
                .unwrap()
                .iter()
                .any(|f| f == "stock-sounds")
        );
        assert_eq!(v["stock_sound_contract"], json!([1, 1]));
        assert!(!capabilities_json().contains('\n'));
    }

    #[test]
    fn entry_goes_under_the_canonical_install_hash() {
        let tmp = std::env::temp_dir().join(format!("ultrasound-registry-{}", std::process::id()));
        let install = tmp.join("Call of Duty Black Ops III");
        let sound = install.join("sound");
        std::fs::create_dir_all(&sound).unwrap();
        std::fs::create_dir_all(install.join("share")).unwrap();
        let exe = sound.join("snd_convert.exe");
        std::fs::write(&exe, b"MZ").unwrap();
        assert_eq!(install_of(&exe).as_deref(), Some(install.as_path()));
        assert_eq!(install_of(&tmp.join("snd_convert.exe")), None);

        let root = tmp.join("components");
        // (a path spelled with a trailing separator and `..` resolves to the same folder)
        let spelled = sound.join("..").join("");
        let path = write_entry(&root, &spelled, &exe).unwrap();
        let canon = canonical_install(&install).unwrap();
        assert_eq!(
            path,
            root.join(install_hash(&canon)).join("ultrasound.json")
        );
        let v: Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(v["id"], "ultrasound");
        assert_eq!(v["exe_size"], 2);
        assert_eq!(v["path"], exe.to_string_lossy().as_ref());
        assert!(
            v["capabilities"]
                .as_array()
                .unwrap()
                .iter()
                .any(|f| f == "stock-sounds")
        );
        // (a second run with nothing changed leaves the file as it is)
        let before = std::fs::metadata(&path).unwrap().modified().unwrap();
        write_entry(&root, &install, &exe).unwrap();
        assert_eq!(
            std::fs::metadata(&path).unwrap().modified().unwrap(),
            before
        );
        let _ = std::fs::remove_dir_all(&tmp);
    }
}
