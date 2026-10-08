use std::path::{Path, PathBuf};

use crate::env::Env;
use crate::tables::alias_enums::{AliasLooping, AliasStorage};
use crate::tables::row_alias::RowAlias;
use crate::tables::row_locale::RowLocale;
use crate::tables::row_platform::RowPlatform;

/// A single source file resolved from a `FileSpec` plus its deterministic target
/// name inside the sound asset bank. Corresponds to one asset going into the bank.
#[derive(Debug, Clone)]
pub struct ResolvedFile {
    pub source_path: PathBuf,
    pub target_name: String,
}

/// Resolve `file_spec` (a path relative to the asset root) to a list of WAV,
/// FLAC or OGG source files, each with a deterministic target filename.
///
/// There are no wildcards: a filespec resolves either to a single file or to
/// every supported file directly inside a directory.
pub fn expand(
    env: &Env,
    alias: &RowAlias,
    platform: &RowPlatform,
    language: &RowLocale,
    file_spec: &str,
    looping_override: Option<AliasLooping>,
) -> Result<Vec<ResolvedFile>, String> {
    if file_spec.is_empty() {
        return Ok(Vec::new());
    }

    // A filespec is partitioned strictly by locale. For a non-shared locale
    // (e.g. `en`), look only under `soundloc_assets/<search_name>/<spec>`;
    // shared assets live under `sound_assets/<spec>` and belong to the `all`
    // bank. No fallback between trees — a shared weapon foley is *supposed*
    // to be absent from the English bank so the runtime loads it from `all`
    // at playback time.
    let candidate = if language.is_shared {
        env.get_source_asset_dir(false).join(file_spec)
    } else {
        let localized = format!("{}/{}", language.search_name, file_spec);
        env.get_source_asset_dir(true).join(&localized)
    };
    let sources = enumerate_sources(&candidate)?;
    if sources.is_empty() {
        // Missing files are fatal only for the shared bank. A localized bank
        // legitimately contains nothing for most aliases, so silently drop
        // the filespec.
        if language.is_shared {
            return Err(format!(
                "no files matched filespec '{}' (looked in {})",
                file_spec,
                candidate.display()
            ));
        }
        return Ok(Vec::new());
    }

    let mut out = Vec::with_capacity(sources.len());
    for src in sources {
        let target_name = build_target_name(
            env,
            alias,
            platform,
            language,
            &src,
            looping_override.as_ref(),
        )?;
        out.push(ResolvedFile {
            source_path: src,
            target_name,
        });
    }
    Ok(out)
}

fn is_supported_audio(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| ["wav", "flac", "ogg"].iter().any(|s| e.eq_ignore_ascii_case(s)))
}

/// If `full` is a supported audio file, return [full]. If it's a directory,
/// return every supported audio file inside it (non-recursive). Otherwise, empty.
fn enumerate_sources(full: &Path) -> Result<Vec<PathBuf>, String> {
    if full.is_file() {
        if is_supported_audio(full) {
            return Ok(vec![full.to_path_buf()]);
        }
        return Ok(Vec::new());
    }
    if !full.is_dir() {
        return Ok(Vec::new());
    }
    let mut files = Vec::new();
    for entry in std::fs::read_dir(full)
        .map_err(|e| format!("Failed to read dir {}: {}", full.display(), e))?
    {
        let entry = entry.map_err(|e| format!("read_dir entry error: {}", e))?;
        let path = entry.path();
        if is_supported_audio(&path) {
            if entry.file_name().to_string_lossy().contains(' ') {
                return Err(format!("filename contains a space: {}", path.display()));
            }
            files.push(path);
        }
    }
    files.sort();
    Ok(files)
}

/// Build `{StorageChar}{LoopingChar}{Compression}.{Platform}.snd` appended to
/// the source base name, with any leading language prefix stripped.
fn build_target_name(
    env: &Env,
    alias: &RowAlias,
    platform: &RowPlatform,
    language: &RowLocale,
    source: &Path,
    looping_override: Option<&AliasLooping>,
) -> Result<String, String> {
    let storage = alias
        .storage
        .as_ref()
        .ok_or_else(|| "missing Storage".to_string())?;
    let looping = match looping_override {
        Some(looping) => looping,
        None => alias
            .looping
            .as_ref()
            .ok_or_else(|| "missing Looping".to_string())?,
    };
    let compression = alias
        .compression
        .ok_or_else(|| "missing Compression".to_string())?;

    let scaled = language.scale_compression(platform.scale_compression(compression));

    let storage_char = match storage {
        AliasStorage::Loaded => 'L',
        AliasStorage::Streamed => 'S',
        AliasStorage::Primed => 'P',
    };
    let looping_char = match looping {
        AliasLooping::Looping => 'L',
        AliasLooping::Nonlooping => 'N',
    };

    // Strip (in order) the localized asset root, the shared asset root, a
    // leading language-name segment, and any leading separators. What's left
    // is the path relative to the asset root. Pure string manipulation — no
    // PathBuf::join, which produces mixed separators on Windows when the base
    // path already contains forward slashes.
    let rel = strip_asset_root_prefix(env, source, &language.search_name);
    // Canonicalize separators to '\' (the on-disk bank format uses backslashes).
    let mut rel_str = rel.to_string_lossy().replace('/', "\\");
    // Replace the trailing `.wav` (or whatever extension) with the suffix.
    let last_dot = rel_str
        .rfind('.')
        .ok_or_else(|| format!("source path has no extension: {}", source.display()))?;
    // Guard against matching a `.` inside a directory name (e.g. "foo.bar/baz").
    // If the last `.` appears before the last separator, there's no extension.
    let last_sep = rel_str.rfind('\\');
    if last_sep.map(|s| s > last_dot).unwrap_or(false) {
        return Err(format!(
            "source path has no extension: {}",
            source.display()
        ));
    }
    rel_str.truncate(last_dot);
    let suffix = format!(
        ".{}{}{}.{}.snd",
        storage_char, looping_char, scaled, platform.platform
    );
    rel_str.push_str(&suffix);
    Ok(rel_str)
}

/// Strip asset-root and language-prefix components from an absolute source
/// path, leaving only the portion relative to the asset root.
fn strip_asset_root_prefix(env: &Env, path: &Path, language: &str) -> PathBuf {
    let s_forward = path.to_string_lossy().replace('\\', "/");
    let localized_root = env
        .get_source_asset_dir(true)
        .to_string_lossy()
        .replace('\\', "/");
    let shared_root = env
        .get_source_asset_dir(false)
        .to_string_lossy()
        .replace('\\', "/");

    let mut s = s_forward.as_str();
    if s.to_ascii_lowercase()
        .starts_with(&localized_root.to_ascii_lowercase())
    {
        s = &s[localized_root.len()..];
    }
    if s.to_ascii_lowercase()
        .starts_with(&shared_root.to_ascii_lowercase())
    {
        s = &s[shared_root.len()..];
    }
    // Strip leading separators.
    while s.starts_with('/') || s.starts_with('\\') {
        s = &s[1..];
    }
    // Strip leading language segment.
    let lang_prefix = format!("{}/", language);
    if s.to_ascii_lowercase()
        .starts_with(&lang_prefix.to_ascii_lowercase())
    {
        s = &s[lang_prefix.len()..];
    }
    // Strip leading separators again for good measure.
    while s.starts_with('/') || s.starts_with('\\') {
        s = &s[1..];
    }
    PathBuf::from(s)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn enumeration_ignores_case_and_spaces_above_the_folder() {
        let root = std::env::temp_dir().join(format!("ultrasound filespec {}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let dir = root.join("Call of Duty Black Ops III/sound_assets/zm");
        std::fs::create_dir_all(&dir).unwrap();
        for name in ["b.WAV", "a.flac", "c.Ogg", "notes.txt"] {
            std::fs::write(dir.join(name), b"").unwrap();
        }

        let names = |paths: Vec<PathBuf>| -> Vec<String> {
            paths.iter().map(|p| p.file_name().unwrap().to_string_lossy().into_owned()).collect()
        };
        assert_eq!(names(enumerate_sources(&dir).unwrap()), ["a.flac", "b.WAV", "c.Ogg"]);
        assert_eq!(names(enumerate_sources(&dir.join("b.WAV")).unwrap()), ["b.WAV"]);
        assert!(enumerate_sources(&dir.join("notes.txt")).unwrap().is_empty());

        std::fs::write(dir.join("has space.wav"), b"").unwrap();
        let err = enumerate_sources(&dir).unwrap_err();
        assert!(err.contains("has space.wav"), "{err}");
        std::fs::remove_dir_all(root).unwrap();
    }
}
