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

/// Resolve `file_spec` (a path relative to the asset root) to a list of .wav
/// source files, each with a deterministic target filename.
///
/// v1 limitations:
/// - no wildcards; a filespec either resolves to a single file or to every
///   `*.wav` inside a directory (the "dir enumeration" case).
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
        let target_name = build_target_name(env, alias, platform, language, &src, looping_override.as_ref())?;
        out.push(ResolvedFile {
            source_path: src,
            target_name,
        });
    }
    Ok(out)
}

fn is_supported_audio_ext(ext: Option<&str>) -> bool {
    matches!(ext, Some("wav") | Some("flac"))
}

/// If `full` is a supported audio file, return [full]. If it's a directory,
/// return every supported audio file inside it (non-recursive). Otherwise, empty.
fn enumerate_sources(full: &Path) -> Result<Vec<PathBuf>, String> {
    if full.is_file() {
        if is_supported_audio_ext(full.extension().and_then(|s| s.to_str())) {
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
        if is_supported_audio_ext(path.extension().and_then(|s| s.to_str())) {
            if path.to_string_lossy().contains(' ') {
                return Err(format!(
                    "filename contains a space: {}",
                    path.display()
                ));
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
        .ok_or_else(|| format!("alias '{}' missing Storage", alias.name))?;
    let looping = looping_override.unwrap_or_else(|| {
        alias.looping.as_ref().unwrap()
    });
    let compression = alias
        .compression
        .ok_or_else(|| format!("alias '{}' missing Compression", alias.name))?;

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
    let rel = strip_asset_root_prefix(env, source, &language.name);
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
        return Err(format!("source path has no extension: {}", source.display()));
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
    let localized_root = env.get_source_asset_dir(true).to_string_lossy().replace('\\', "/");
    let shared_root = env.get_source_asset_dir(false).to_string_lossy().replace('\\', "/");

    let mut s = s_forward.as_str();
    if s.to_ascii_lowercase().starts_with(&localized_root.to_ascii_lowercase()) {
        s = &s[localized_root.len()..];
    }
    if s.to_ascii_lowercase().starts_with(&shared_root.to_ascii_lowercase()) {
        s = &s[shared_root.len()..];
    }
    // Strip leading separators.
    while s.starts_with('/') || s.starts_with('\\') {
        s = &s[1..];
    }
    // Strip leading language segment.
    let lang_prefix = format!("{}/", language);
    if s.to_ascii_lowercase().starts_with(&lang_prefix.to_ascii_lowercase()) {
        s = &s[lang_prefix.len()..];
    }
    // Strip leading separators again for good measure.
    while s.starts_with('/') || s.starts_with('\\') {
        s = &s[1..];
    }
    PathBuf::from(s)
}

