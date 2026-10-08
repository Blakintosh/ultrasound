use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT_STAGE: AtomicU64 = AtomicU64::new(0);

pub struct StagedOutputs {
    root: PathBuf,
    preserve: bool,
}

struct PublishedFile {
    destination: PathBuf,
    backup: Option<PathBuf>,
    installed: bool,
}

impl StagedOutputs {
    pub fn new(output_root: &Path) -> Result<Self, String> {
        fs::create_dir_all(output_root).map_err(|e| format!("create {}: {}", output_root.display(), e))?;
        loop {
            let id = NEXT_STAGE.fetch_add(1, Ordering::Relaxed);
            let root = output_root.join(format!(".ultrasound-stage-{}-{}", std::process::id(), id));
            match fs::create_dir(&root) {
                Ok(()) => {
                    return Ok(Self {
                        root,
                        preserve: false,
                    });
                }
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(e) => {
                    return Err(format!(
                        "create staging directory {}: {}",
                        root.display(),
                        e
                    ));
                }
            }
        }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn publish(&mut self, output_root: &Path) -> Result<(), String> {
        let mut files = Vec::new();
        collect_files(&self.root, &mut files)?;
        files.sort();
        // Validate destinations before moving any existing output aside.
        for staged in &files {
            let destination = output_root.join(staged.strip_prefix(&self.root).unwrap());
            if destination.exists() && !destination.is_file() {
                return Err(format!("output is not a file: {}", destination.display()));
            }
            if let Some(parent) = destination.parent() {
                fs::create_dir_all(parent)
                    .map_err(|e| format!("create {}: {}", parent.display(), e))?;
            }
        }
        let mut published = Vec::new();
        for staged in files {
            let relative = staged.strip_prefix(&self.root).unwrap();
            let destination = output_root.join(relative);
            match files_equal(&staged, &destination) {
                Ok(true) => continue,
                Ok(false) => {}
                Err(error) => {
                    let rollback_errors = rollback(&published);
                    if rollback_errors.is_empty() {
                        return Err(format!(
                            "compare {}: {} (previous outputs restored)",
                            destination.display(),
                            error
                        ));
                    }
                    self.preserve = true;
                    return Err(format!(
                        "compare {}: {}; rollback errors: {:?}; staging: {}",
                        destination.display(),
                        error,
                        rollback_errors,
                        self.root.display()
                    ));
                }
            }
            let backup = self.root.join(".backup").join(relative);
            let result = (|| -> Result<(), String> {
                let existing = if destination.exists() {
                    fs::create_dir_all(backup.parent().unwrap()).map_err(|e| e.to_string())?;
                    fs::rename(&destination, &backup).map_err(|e| e.to_string())?;
                    Some(backup)
                } else {
                    None
                };
                published.push(PublishedFile {
                    destination: destination.clone(),
                    backup: existing,
                    installed: false,
                });
                fs::rename(&staged, &destination).map_err(|e| e.to_string())?;
                published.last_mut().unwrap().installed = true;
                Ok(())
            })();
            if let Err(error) = result {
                let rollback = rollback(&published);
                if !rollback.is_empty() {
                    self.preserve = true;
                    return Err(format!(
                        "publish {}: {}; rollback failed: {}. Backups retained at {}",
                        destination.display(),
                        error,
                        rollback.join("; "),
                        self.root.display()
                    ));
                }
                return Err(format!(
                    "publish {}: {} (previous outputs restored)",
                    destination.display(),
                    error
                ));
            }
        }
        Ok(())
    }
}

fn read_error(path: &Path) -> impl FnOnce(std::io::Error) -> String + '_ {
    move |e| format!("read {}: {}", path.display(), e)
}

pub fn files_equal(source_path: &Path, destination_path: &Path) -> Result<bool, String> {
    let mut source = fs::File::open(source_path).map_err(read_error(source_path))?;
    let mut destination = match fs::File::open(destination_path) {
        Ok(file) => file,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(false),
        Err(e) => return Err(read_error(destination_path)(e)),
    };
    let length = source.metadata().map_err(read_error(source_path))?.len();
    if destination.metadata().map_err(read_error(destination_path))?.len() != length {
        return Ok(false);
    }
    let mut left = [0u8; 65536];
    let mut right = [0u8; 65536];
    let mut remaining = length;
    while remaining > 0 {
        let count = remaining.min(left.len() as u64) as usize;
        source
            .read_exact(&mut left[..count])
            .map_err(read_error(source_path))?;
        destination
            .read_exact(&mut right[..count])
            .map_err(read_error(destination_path))?;
        if left[..count] != right[..count] {
            return Ok(false);
        }
        remaining -= count as u64;
    }
    Ok(true)
}

fn rollback(files: &[PublishedFile]) -> Vec<String> {
    let mut errors = Vec::new();
    for file in files.iter().rev() {
        if file.installed {
            if let Err(e) = fs::remove_file(&file.destination) {
                errors.push(format!("remove {}: {}", file.destination.display(), e));
                continue;
            }
        }
        if let Some(backup) = &file.backup {
            if let Err(e) = fs::rename(backup, &file.destination) {
                errors.push(format!("restore {}: {}", file.destination.display(), e));
            }
        }
    }
    errors
}

fn collect_files(dir: &Path, files: &mut Vec<PathBuf>) -> Result<(), String> {
    let list_error = |e: std::io::Error| format!("list {}: {}", dir.display(), e);
    for entry in fs::read_dir(dir).map_err(list_error)? {
        let entry = entry.map_err(list_error)?;
        if entry.file_type().map_err(list_error)?.is_dir() {
            collect_files(&entry.path(), files)?;
        } else {
            files.push(entry.path());
        }
    }
    Ok(())
}

impl Drop for StagedOutputs {
    fn drop(&mut self) {
        if !self.preserve {
            let _ = fs::remove_dir_all(&self.root);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn publishes_complete_files_and_cleans_staging() {
        let outer = StagedOutputs::new(&std::env::temp_dir()).unwrap();
        let output = outer.root().join("output");
        fs::create_dir_all(&output).unwrap();
        fs::write(output.join("a"), b"old").unwrap();
        let stage_path;
        {
            let mut stage = StagedOutputs::new(&output).unwrap();
            stage_path = stage.root().to_path_buf();
            fs::write(stage.root().join("a"), b"new").unwrap();
            fs::write(stage.root().join("b"), b"added").unwrap();
            stage.publish(&output).unwrap();
            assert_eq!(fs::read(output.join("a")).unwrap(), b"new");
            assert_eq!(fs::read(output.join("b")).unwrap(), b"added");
        }
        assert!(!stage_path.exists());
    }

    #[test]
    fn invalid_destination_leaves_existing_files_untouched() {
        let outer = StagedOutputs::new(&std::env::temp_dir()).unwrap();
        let output = outer.root().join("output");
        fs::create_dir_all(output.join("b")).unwrap();
        fs::write(output.join("a"), b"old").unwrap();
        let mut stage = StagedOutputs::new(&output).unwrap();
        fs::write(stage.root().join("a"), b"new").unwrap();
        fs::write(stage.root().join("b"), b"added").unwrap();
        assert!(stage.publish(&output).is_err());
        assert_eq!(fs::read(output.join("a")).unwrap(), b"old");
    }

    #[test]
    fn rollback_restores_replaced_files_and_removes_new_files() {
        let outer = StagedOutputs::new(&std::env::temp_dir()).unwrap();
        let replaced = outer.root().join("replaced");
        let backup = outer.root().join("backup");
        let added = outer.root().join("added");
        fs::write(&replaced, b"new").unwrap();
        fs::write(&backup, b"old").unwrap();
        fs::write(&added, b"new").unwrap();
        assert!(
            rollback(&[
                PublishedFile {
                    destination: replaced.clone(),
                    backup: Some(backup),
                    installed: true
                },
                PublishedFile {
                    destination: added.clone(),
                    backup: None,
                    installed: true
                },
            ])
            .is_empty()
        );
        assert_eq!(fs::read(replaced).unwrap(), b"old");
        assert!(!added.exists());
    }

    #[test]
    #[cfg(windows)]
    fn sharing_violation_rolls_back_an_earlier_replacement() {
        use std::os::windows::fs::OpenOptionsExt;
        let outer = StagedOutputs::new(&std::env::temp_dir()).unwrap();
        let output = outer.root().join("output");
        fs::create_dir_all(&output).unwrap();
        fs::write(output.join("a"), b"old a").unwrap();
        fs::write(output.join("z"), b"old z").unwrap();
        let _locked = fs::OpenOptions::new()
            .read(true)
            .share_mode(0)
            .open(output.join("z"))
            .unwrap();
        let mut stage = StagedOutputs::new(&output).unwrap();
        fs::write(stage.root().join("a"), b"new a").unwrap();
        fs::write(stage.root().join("z"), b"new z").unwrap();
        let error = stage.publish(&output).unwrap_err();
        assert!(error.contains("previous outputs restored"), "{error}");
        assert_eq!(fs::read(output.join("a")).unwrap(), b"old a");
    }
}
