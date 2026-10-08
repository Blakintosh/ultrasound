use std::{collections::HashMap, fs, io::Read, path::Path, sync::Mutex};

use serde::{Deserialize, Serialize};

use crate::source_asset_cache::Checksum;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
struct Stamp {
    volume: String,
    journal: u64,
    file: u64,
    usn: u64,
    size: u64,
}

#[derive(Clone, Serialize, Deserialize)]
struct Record {
    stamp: Stamp,
    checkpoint: u64,
    recipe: Vec<u8>,
    checksum: [u8; 16],
}

fn journal_contains(usn: u64, first: u64, next: u64, lowest_valid: u64) -> bool {
    usn != 0 && first <= usn && lowest_valid <= usn && usn <= next
}

struct Observation {
    stamp: Stamp,
    first: u64,
    next: u64,
    lowest_valid: u64,
}

impl Record {
    fn matches(&self, current: &Observation, recipe: &[u8], existing: Option<Checksum>) -> bool {
        self.stamp == current.stamp
            && journal_contains(
                self.checkpoint,
                current.first,
                current.next,
                current.lowest_valid,
            )
            && self.recipe == recipe
            && existing == Some(Checksum(self.checksum))
    }
}

pub struct SourceValidation {
    records: Mutex<HashMap<String, Record>>,
    enabled: bool,
    reused: std::sync::atomic::AtomicUsize,
}

impl SourceValidation {
    pub fn load(path: &Path) -> Self {
        Self {
            records: Mutex::new(
                fs::read(path)
                    .ok()
                    .and_then(|bytes| serde_json::from_slice(&bytes).ok())
                    .unwrap_or_default(),
            ),
            enabled: std::env::var_os("ULTRASOUND_FULL_HASH").is_none(),
            reused: std::sync::atomic::AtomicUsize::new(0),
        }
    }

    pub fn read(
        &self,
        path: &str,
        recipe: &[u8],
        existing: Option<Checksum>,
    ) -> Result<(Checksum, Option<Vec<u8>>), String> {
        let mut options = fs::OpenOptions::new();
        options.read(true);
        #[cfg(windows)]
        {
            use std::os::windows::fs::OpenOptionsExt;
            // Excludes existing writers and writable mappings as well as new writes/replacements.
            options.share_mode(1);
        }
        let locked = self.enabled.then(|| options.open(path)).transpose();
        let (mut file, tracked) = match locked {
            Ok(Some(file)) => (file, true),
            _ => (
                fs::File::open(path).map_err(|e| format!("read {path}: {e}"))?,
                false,
            ),
        };
        let before = if tracked { stamp(&file) } else { None };
        if let Some(ref current) = before {
            let records = self.records.lock().unwrap();
            if let Some(record) = records.get(path) {
                if record.matches(current, recipe, existing) {
                    self.reused
                        .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                    return Ok((Checksum(record.checksum), None));
                }
            }
        }
        let mut data = Vec::new();
        file.read_to_end(&mut data)
            .map_err(|e| format!("read {path}: {e}"))?;
        let checksum = Checksum::from_data_with_recipe(&data, recipe);
        let after = if tracked { stamp(&file) } else { None };
        let mut records = self.records.lock().unwrap();
        if let Some(current) = after.filter(|after| {
            before
                .as_ref()
                .is_some_and(|before| before.stamp == after.stamp)
        }) {
            records.insert(
                path.to_owned(),
                Record {
                    stamp: current.stamp,
                    checkpoint: current.next,
                    recipe: recipe.to_vec(),
                    checksum: checksum.0,
                },
            );
        } else {
            records.remove(path);
        }
        Ok((checksum, Some(data)))
    }

    /// How many sources kept their checksum without being hashed again.
    pub fn reused(&self) -> usize {
        self.reused.load(std::sync::atomic::Ordering::Relaxed)
    }

    pub fn save(&self, path: &Path) {
        if self.enabled {
            let bytes = serde_json::to_vec(&*self.records.lock().unwrap()).unwrap();
            if let Err(error) = fs::write(path, bytes) {
                eprintln!(
                    "  cannot save source validation {}: {error}",
                    path.display()
                );
            }
        }
    }
}

#[cfg(not(windows))]
fn stamp(_: &fs::File) -> Option<Observation> {
    None
}

#[cfg(windows)]
fn stamp(file: &fs::File) -> Option<Observation> {
    use std::{
        ffi::c_void,
        os::windows::io::{AsRawHandle, FromRawHandle},
    };
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn DeviceIoControl(
            handle: *mut c_void,
            code: u32,
            input: *const c_void,
            input_len: u32,
            output: *mut c_void,
            output_len: u32,
            returned: *mut u32,
            overlapped: *mut c_void,
        ) -> i32;
        fn GetFinalPathNameByHandleW(
            handle: *mut c_void,
            path: *mut u16,
            size: u32,
            flags: u32,
        ) -> u32;
        fn GetVolumeInformationW(
            root: *const u16,
            name: *mut u16,
            name_len: u32,
            serial: *mut u32,
            component_len: *mut u32,
            flags: *mut u32,
            filesystem: *mut u16,
            filesystem_len: u32,
        ) -> i32;
        fn CreateFileW(
            path: *const u16,
            access: u32,
            share: u32,
            security: *mut c_void,
            disposition: u32,
            flags: u32,
            template: *mut c_void,
        ) -> *mut c_void;
    }
    fn ioctl(file: &fs::File, code: u32) -> Option<Vec<u8>> {
        let mut output = [0u64; 128];
        let mut returned = 0;
        // Aligned output storage; synchronous handles, no input buffer, and bounded returned length.
        let ok = unsafe {
            DeviceIoControl(
                file.as_raw_handle(),
                code,
                std::ptr::null(),
                0,
                output.as_mut_ptr().cast(),
                1024,
                &mut returned,
                std::ptr::null_mut(),
            )
        };
        if ok == 0 || returned > 1024 {
            return None;
        }
        Some(
            output
                .iter()
                .flat_map(|v| v.to_ne_bytes())
                .take(returned as usize)
                .collect(),
        )
    }
    fn u64_at(bytes: &[u8], offset: usize) -> Option<u64> {
        Some(u64::from_le_bytes(
            bytes.get(offset..offset + 8)?.try_into().ok()?,
        ))
    }
    let mut path = vec![0u16; 32768];
    let length = unsafe {
        GetFinalPathNameByHandleW(
            file.as_raw_handle(),
            path.as_mut_ptr(),
            path.len() as u32,
            1,
        )
    };
    if length == 0 || length as usize >= path.len() {
        return None;
    }
    let path = String::from_utf16(&path[..length as usize]).ok()?;
    let end = path.find("}\\")?;
    let volume = path[..end + 1].to_owned();
    // Only local NTFS volumes are eligible. Network/reparse destinations resolve through the handle.
    let root: Vec<u16> = format!("{volume}\\\0").encode_utf16().collect();
    let mut filesystem = [0u16; 32];
    let ok = unsafe {
        GetVolumeInformationW(
            root.as_ptr(),
            std::ptr::null_mut(),
            0,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            filesystem.as_mut_ptr(),
            filesystem.len() as u32,
        )
    };
    if ok == 0 || filesystem[..5] != [78, 84, 70, 83, 0] {
        return None;
    }
    let handle = unsafe {
        CreateFileW(
            root.as_ptr(),
            0,
            7,
            std::ptr::null_mut(),
            3,
            0x02000000,
            std::ptr::null_mut(),
        )
    };
    if handle as isize == -1 {
        return None;
    }
    let volume_file = unsafe { fs::File::from_raw_handle(handle) };
    let journal = ioctl(&volume_file, 0x000900f4)?; // FSCTL_QUERY_USN_JOURNAL
    let record = ioctl(file, 0x000900eb)?; // FSCTL_READ_FILE_USN_DATA (V2)
    if record.get(4..6)? != 2u16.to_le_bytes() {
        return None;
    }
    let usn = u64_at(&record, 24)?;
    if usn == 0 || usn >= u64_at(&journal, 16)? {
        return None;
    }
    let final_journal = ioctl(&volume_file, 0x000900f4)?;
    if u64_at(&journal, 0)? != u64_at(&final_journal, 0)? {
        return None;
    }
    Some(Observation {
        stamp: Stamp {
            volume,
            journal: u64_at(&journal, 0)?,
            file: u64_at(&record, 8)?,
            usn,
            size: file.metadata().ok()?.len(),
        },
        first: u64_at(&final_journal, 8)?,
        next: u64_at(&final_journal, 16)?,
        lowest_valid: u64_at(&final_journal, 24)?,
    })
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;
    use std::{
        io::{Seek, SeekFrom, Write},
        sync::atomic::{AtomicUsize, Ordering},
    };

    struct Fixture {
        root: std::path::PathBuf,
        source: String,
        cache: std::path::PathBuf,
    }
    impl Fixture {
        fn new() -> Self {
            static NEXT: AtomicUsize = AtomicUsize::new(0);
            let root = std::env::temp_dir().join(format!(
                "ultrasound-usn-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            fs::create_dir(&root).unwrap();
            let source = root.join("source.wav").to_str().unwrap().to_owned();
            fs::write(&source, b"original").unwrap();
            Self {
                cache: root.join("validation.json"),
                root,
                source,
            }
        }
        fn seed(&self) -> (SourceValidation, Checksum) {
            let cache = SourceValidation::load(&self.cache);
            let checksum = cache.read(&self.source, b"recipe", None).unwrap().0;
            assert!(
                cache.records.lock().unwrap().contains_key(&self.source),
                "test requires NTFS journal access"
            );
            cache.save(&self.cache);
            (SourceValidation::load(&self.cache), checksum)
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            fs::remove_dir_all(&self.root).unwrap();
        }
    }

    #[test]
    fn unavailable_or_expired_journal_history_cannot_validate_a_stamp() {
        assert!(journal_contains(100, 90, 110, 80));
        assert!(!journal_contains(100, 101, 110, 80));
        assert!(!journal_contains(100, 90, 110, 101));
        assert!(journal_contains(100, 90, 100, 80));
        assert!(!journal_contains(101, 90, 100, 80));
        assert!(!journal_contains(100, 0, 0, 0));
        assert!(!journal_contains(0, 0, 110, 0));
    }

    #[test]
    fn old_file_usn_can_be_reused_only_with_a_retained_validation_checkpoint() {
        let stamp = Stamp {
            volume: "volume".into(),
            journal: 7,
            file: 8,
            usn: 10,
            size: 8,
        };
        let record = Record {
            stamp: stamp.clone(),
            checkpoint: 100,
            recipe: b"recipe".to_vec(),
            checksum: [1; 16],
        };
        let mut current = Observation {
            stamp,
            first: 90,
            next: 110,
            lowest_valid: 0,
        };
        assert!(record.matches(&current, b"recipe", Some(Checksum([1; 16]))));
        current.first = 101;
        assert!(!record.matches(&current, b"recipe", Some(Checksum([1; 16]))));
    }

    #[test]
    fn forced_hashing_ignores_a_valid_cached_stamp() {
        let fixture = Fixture::new();
        let (mut cache, checksum) = fixture.seed();
        cache.enabled = false;
        assert!(
            cache
                .read(&fixture.source, b"recipe", Some(checksum))
                .unwrap()
                .1
                .is_some()
        );
    }

    #[test]
    fn journal_cache_reuses_unchanged_source_across_loads() {
        let fixture = Fixture::new();
        let (cache, checksum) = fixture.seed();
        // NTFS journal access is required for this test; a fallback must not masquerade as a pass.
        assert!(
            cache
                .read(&fixture.source, b"recipe", Some(checksum))
                .unwrap()
                .1
                .is_none()
        );
    }

    #[test]
    fn same_size_edit_with_restored_timestamp_is_hashed() {
        let fixture = Fixture::new();
        let (cache, checksum) = fixture.seed();
        let modified = fs::metadata(&fixture.source).unwrap().modified().unwrap();
        fs::write(&fixture.source, b"modified").unwrap();
        fs::File::options()
            .write(true)
            .open(&fixture.source)
            .unwrap()
            .set_modified(modified)
            .unwrap();
        let (changed, data) = cache
            .read(&fixture.source, b"recipe", Some(checksum))
            .unwrap();
        assert_ne!(changed, checksum);
        assert_eq!(data.unwrap(), b"modified");
    }

    #[test]
    fn replacement_and_hardlink_edits_are_hashed() {
        let fixture = Fixture::new();
        let (cache, checksum) = fixture.seed();
        fs::rename(&fixture.source, fixture.root.join("old.wav")).unwrap();
        fs::write(&fixture.source, b"replaced").unwrap();
        assert_ne!(
            cache
                .read(&fixture.source, b"recipe", Some(checksum))
                .unwrap()
                .0,
            checksum
        );
        let (cache, checksum) = fixture.seed();
        let link = fixture.root.join("link.wav");
        fs::hard_link(&fixture.source, &link).unwrap();
        fs::write(link, b"hardlink").unwrap();
        assert_ne!(
            cache
                .read(&fixture.source, b"recipe", Some(checksum))
                .unwrap()
                .0,
            checksum
        );
    }

    #[test]
    fn recipe_changes_and_missing_bank_entries_require_data() {
        let fixture = Fixture::new();
        let (cache, checksum) = fixture.seed();
        let (changed, data) = cache
            .read(&fixture.source, b"new recipe", Some(checksum))
            .unwrap();
        assert_ne!(changed, checksum);
        assert!(data.is_some());
        assert!(
            cache
                .read(&fixture.source, b"new recipe", None)
                .unwrap()
                .1
                .is_some()
        );
    }

    #[test]
    fn stale_journal_identity_and_change_number_require_hashing() {
        let fixture = Fixture::new();
        for field in ["journal", "usn", "file", "volume"] {
            let (_, checksum) = fixture.seed();
            let mut records: serde_json::Value =
                serde_json::from_slice(&fs::read(&fixture.cache).unwrap()).unwrap();
            records[&fixture.source]["stamp"][field] = if field == "volume" {
                "invalid".into()
            } else {
                0.into()
            };
            fs::write(&fixture.cache, serde_json::to_vec(&records).unwrap()).unwrap();
            let cache = SourceValidation::load(&fixture.cache);
            assert!(
                cache
                    .read(&fixture.source, b"recipe", Some(checksum))
                    .unwrap()
                    .1
                    .is_some(),
                "{field}"
            );
        }
    }

    #[test]
    fn corrupt_validation_cache_and_deleted_source_do_not_hit() {
        let fixture = Fixture::new();
        let (_, checksum) = fixture.seed();
        fs::write(&fixture.cache, b"{broken").unwrap();
        let cache = SourceValidation::load(&fixture.cache);
        assert!(
            cache
                .read(&fixture.source, b"recipe", Some(checksum))
                .unwrap()
                .1
                .is_some()
        );
        fs::remove_file(&fixture.source).unwrap();
        assert!(
            cache
                .read(&fixture.source, b"recipe", Some(checksum))
                .is_err()
        );
    }

    #[test]
    fn open_writer_forces_hashing_and_does_not_cache_a_stamp() {
        let fixture = Fixture::new();
        let (cache, checksum) = fixture.seed();
        let mut writer = fs::OpenOptions::new()
            .write(true)
            .open(&fixture.source)
            .unwrap();
        writer.seek(SeekFrom::Start(0)).unwrap();
        writer.write_all(b"modified").unwrap();
        let (changed, data) = cache
            .read(&fixture.source, b"recipe", Some(checksum))
            .unwrap();
        assert_ne!(changed, checksum);
        assert_eq!(data.unwrap(), b"modified");
        assert!(!cache.records.lock().unwrap().contains_key(&fixture.source));
    }
}
