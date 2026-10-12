//! Sounds taken, as they are, from the banks the game ships.
//!
//! The Linker's stock carry (Dragon) decides which shipped sound aliases a map needs and writes
//! `zone_source\<zone>.stocksound.json` beside the zone file. The aliases themselves travel in the map's fastfile;
//! what this module supplies is their audio: each listed sound is copied byte for byte, with the entry the shipped
//! bank states for it, out of the shipped `zone\snd\<language>\<bank>.sabl|sabs` (patch bank first, then the bank,
//! then the banks it depends on) into the bank this run builds. Nothing is decoded or encoded.
//!
//! Without the file nothing here runs or prints.

use std::collections::HashSet;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

use serde::Deserialize;

use crate::bank::bank_entry::BankEntry;
use crate::string_hash;

const CONTRACT: u32 = 1;
const MAGIC: u32 = 592991538;
const HEADER: usize = 2048;
const ENTRY: usize = std::mem::size_of::<BankEntry>();

#[derive(Deserialize)]
struct Request {
    stock_sound_contract: u32,
    #[serde(default)]
    entries: Vec<RequestEntry>,
}

#[derive(Deserialize)]
struct RequestEntry {
    /// The sound's name in the bank (`wpn\...\x.LN100.pc.snd`), whose hash is the entry's key.
    file: String,
    /// `loaded` (`.sabl`) or `streamed` (`.sabs`): the bank of the map it goes to.
    storage: String,
    /// The zone whose bank holds it.
    donor: String,
    /// `all` or a language name (`english`).
    language: String,
}

/// One sound ready to go into a bank.
pub struct DonorSound {
    pub name: String,
    pub streamed: bool,
    pub entry: BankEntry,
    pub data: Vec<u8>,
}

fn rd32(b: &[u8], o: usize) -> u32 {
    u32::from_le_bytes(b[o..o + 4].try_into().unwrap())
}
fn rd64(b: &[u8], o: usize) -> i64 {
    i64::from_le_bytes(b[o..o + 8].try_into().unwrap())
}

/// A shipped bank's tables.
struct Shipped {
    path: PathBuf,
    deps: Vec<String>,
    entries: Vec<BankEntry>,
}

impl Shipped {
    fn open(path: &Path) -> Option<Shipped> {
        Self::open_one(path).or_else(|| {
            // (a bank the game ships encrypted, with a decrypted copy beside it)
            let mut alt = path.as_os_str().to_owned();
            alt.push(".wraithdec");
            Self::open_one(Path::new(&alt))
        })
    }

    fn open_one(path: &Path) -> Option<Shipped> {
        let mut f = File::open(path).ok()?;
        let len = f.metadata().ok()?.len();
        let mut h = vec![0u8; HEADER];
        f.read_exact(&mut h).ok()?;
        if rd32(&h, 0) != MAGIC || rd32(&h, 4) != 15 || rd32(&h, 8) as usize != ENTRY {
            return None;
        }
        let count = rd32(&h, 0x14) as usize;
        let at = rd64(&h, 0x28);
        if at < HEADER as i64 || at as u64 + (count * ENTRY) as u64 > len {
            return None;
        }
        f.seek(SeekFrom::Start(at as u64)).ok()?;
        let mut raw = vec![0u8; count * ENTRY];
        f.read_exact(&mut raw).ok()?;
        let entries = (0..count)
            .map(|i| unsafe { std::ptr::read_unaligned(raw[i * ENTRY..].as_ptr() as *const BankEntry) })
            .collect();
        let deps = (0..8)
            .filter_map(|i| {
                let s = &h[0x48 + 64 * i..0x48 + 64 * (i + 1)];
                let end = s.iter().position(|&b| b == 0).unwrap_or(64);
                (end > 0).then(|| String::from_utf8_lossy(&s[..end]).to_ascii_lowercase())
            })
            .collect();
        Some(Shipped { path: path.to_path_buf(), deps, entries })
    }

    fn find(&self, hash: u32) -> Option<BankEntry> {
        self.entries.iter().copied().find(|e| e.name == hash)
    }

    fn read(&self, e: &BankEntry) -> Result<Vec<u8>, String> {
        let (offset, size) = (e.offset, e.size);
        let mut f = File::open(&self.path).map_err(|x| format!("{}: {x}", self.path.display()))?;
        f.seek(SeekFrom::Start(offset)).map_err(|x| x.to_string())?;
        let mut v = vec![0u8; size as usize];
        f.read_exact(&mut v).map_err(|x| format!("{}: {x}", self.path.display()))?;
        Ok(v)
    }
}

/// Finds `hash` in the bank `donor.<dir>` and the banks it depends on.
fn locate(snd: &Path, dir: &str, donor: &str, hash: u32, streamed: bool, cache: &mut Vec<(PathBuf, Option<std::rc::Rc<Shipped>>)>) -> Option<(std::rc::Rc<Shipped>, BankEntry)> {
    let mut seen: HashSet<String> = HashSet::new();
    let mut todo: Vec<String> = vec![donor.to_ascii_lowercase()];
    let mut i = 0;
    while i < todo.len() {
        let z = todo[i].clone();
        i += 1;
        if !seen.insert(z.clone()) {
            continue;
        }
        let mut deps: Vec<String> = Vec::new();
        for s in [streamed, !streamed] {
            let ext = if s { "sabs" } else { "sabl" };
            for p in [format!("{z}.{dir}.p.{ext}"), format!("{z}.{dir}.{ext}")] {
                let path = snd.join(dir).join(p);
                let slot = match cache.iter().position(|(c, _)| *c == path) {
                    Some(k) => k,
                    None => {
                        let b = if path.is_file() { Shipped::open(&path).map(std::rc::Rc::new) } else { None };
                        cache.push((path.clone(), b));
                        cache.len() - 1
                    }
                };
                if let Some(b) = cache[slot].1.clone() {
                    if let Some(e) = b.find(hash) {
                        return Some((b, e));
                    }
                    deps.extend(b.deps.iter().cloned());
                }
            }
        }
        todo.extend(deps);
    }
    None
}

/// The sounds to put into the `language` banks (folder `dir`) of `zone`: empty (and silent) unless the request file is there.
pub fn load(game_dir: &Path, source_dir: &Path, zone: &str, language: &str, dir: &str) -> Result<Vec<DonorSound>, String> {
    let path = source_dir.join("zone_source").join(format!("{zone}.stocksound.json"));
    let Ok(text) = std::fs::read_to_string(&path) else { return Ok(Vec::new()) };
    let req: Request = serde_json::from_str(&text).map_err(|e| format!("{}: {e}", path.display()))?;
    if req.stock_sound_contract != CONTRACT {
        return Err(format!("{}: stock sound contract {} is not {CONTRACT}; update Ultrasound", path.display(), req.stock_sound_contract));
    }
    let snd = game_dir.join("zone").join("snd");
    let mut cache = Vec::new();
    let mut out: Vec<DonorSound> = Vec::new();
    let mut seen: HashSet<(u32, bool)> = HashSet::new();
    let (mut missing, mut banks) = (0usize, HashSet::new());
    for r in req.entries.iter().filter(|r| r.language.eq_ignore_ascii_case(language)) {
        let streamed = r.storage.eq_ignore_ascii_case("streamed");
        let hash = string_hash::hash(&r.file);
        if !seen.insert((hash, streamed)) {
            continue;
        }
        match locate(&snd, dir, &r.donor, hash, streamed, &mut cache) {
            Some((bank, entry)) if entry.format != 0 && entry.format != 8 => {
                let f = entry.format;
                missing += 1;
                println!("  stock sounds: '{}' is stored as format {f}, which a bank of this build does not hold", r.file);
                let _ = bank;
            }
            Some((bank, entry)) => {
                banks.insert(bank.path.clone());
                let data = bank.read(&entry)?;
                out.push(DonorSound { name: r.file.clone(), streamed, entry, data });
            }
            None => {
                missing += 1;
                println!("  stock sounds: '{}' is in no bank of {} (its alias will be silent)", r.file, r.donor);
            }
        }
    }
    if !out.is_empty() || missing > 0 {
        println!("  stock sounds [{language}]: {} from {} shipped bank(s), {} not found", out.len(), banks.len(), missing);
    }
    Ok(out)
}

const RATES: [i32; 9] = [8000, 12000, 16000, 24000, 32000, 44100, 48000, 96000, 192000];

/// The sound as the bank writer takes a converted one: the data untouched, the entry's fields as the shipped bank
/// states them.
pub fn as_converted(d: &DonorSound, source_checksum: crate::checksum::Checksum) -> (crate::converter::SoundAssetBankConvertedAsset, Vec<u8>) {
    use crate::converter::{AliasLooping, AssetFormat, SoundAssetBankConvertedAsset};
    let e = d.entry;
    let (rate_index, channels, looping, format) = (e.frame_rate_index, e.channel_count, e.looping, e.format);
    (
        SoundAssetBankConvertedAsset {
            converted_checksum: crate::checksum::Checksum::from_data(&d.data),
            source_checksum,
            format: if format == 0 { AssetFormat::SndAssetFormatPcms16 } else { AssetFormat::SndAssetFormatFlac },
            looping: if looping == 1 { AliasLooping::Looping } else { AliasLooping::NonLooping },
            frame_count: e.frame_count as i64,
            frame_rate: RATES.get(rate_index as usize).copied().unwrap_or(48000),
            channel_count: channels as i32,
            envelope_loudness0: e.envelope_loudness0,
            envelope_loudness1: e.envelope_loudness1,
            envelope_loudness2: e.envelope_loudness2,
            envelope_loudness3: e.envelope_loudness3,
            envelope_time1: e.envelope_time1,
            envelope_time2: e.envelope_time2,
        },
        d.data.clone(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nothing_without_a_request() {
        let dir = std::env::temp_dir().join(format!("us_donor_none_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let got = load(&dir, &dir, "zm_none", "all", "all").unwrap();
        assert!(got.is_empty());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_newer_contract_is_refused() {
        let dir = std::env::temp_dir().join(format!("us_donor_contract_{}", std::process::id()));
        std::fs::create_dir_all(dir.join("zone_source")).unwrap();
        std::fs::write(dir.join("zone_source").join("zm_x.stocksound.json"), r#"{"stock_sound_contract": 2, "entries": []}"#).unwrap();
        let err = load(&dir, &dir, "zm_x", "all", "all").err().expect("refused");
        assert!(err.contains("contract 2"), "{err}");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_missing_sound_is_reported_not_fatal() {
        let dir = std::env::temp_dir().join(format!("us_donor_missing_{}", std::process::id()));
        std::fs::create_dir_all(dir.join("zone_source")).unwrap();
        std::fs::write(
            dir.join("zone_source").join("zm_y.stocksound.json"),
            r#"{"stock_sound_contract": 1, "entries": [{"file": "a\b.LN100.pc.snd", "storage": "loaded", "donor": "zm_nowhere", "language": "all"}]}"#,
        )
        .unwrap();
        let got = load(&dir, &dir, "zm_y", "all", "all").unwrap();
        assert!(got.is_empty());
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
