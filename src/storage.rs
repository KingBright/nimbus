use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
#[cfg(unix)]
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT_FILE_ID: AtomicU64 = AtomicU64::new(0);

fn suffixed_path(path: &Path, suffix: &str) -> PathBuf {
    let mut name = path.as_os_str().to_os_string();
    name.push(suffix);
    PathBuf::from(name)
}

fn create_private_file(path: &Path) -> io::Result<File> {
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    options.mode(0o600);
    options.open(path)
}

/// Preserve unreadable data before replacing a file, then publish the complete
/// replacement with a same-directory rename. Existing backups are never replaced.
pub(crate) fn atomic_write(
    path: &Path,
    contents: &[u8],
    validate_existing: impl Fn(&str) -> bool,
) -> io::Result<()> {
    match fs::read(path) {
        Ok(existing) => {
            let valid = std::str::from_utf8(&existing)
                .map(validate_existing)
                .unwrap_or(false);
            if !valid {
                let mut index = 0u64;
                loop {
                    let suffix = if index == 0 {
                        ".invalid".to_string()
                    } else {
                        format!(".invalid.{index}")
                    };
                    let backup = suffixed_path(path, &suffix);
                    match create_private_file(&backup) {
                        Ok(mut file) => {
                            if let Err(error) =
                                file.write_all(&existing).and_then(|_| file.sync_all())
                            {
                                let _ = fs::remove_file(&backup);
                                return Err(error);
                            }
                            eprintln!("[Nimbus] 已保留损坏数据备份：{}", backup.display());
                            break;
                        }
                        Err(error) if error.kind() == io::ErrorKind::AlreadyExists => index += 1,
                        Err(error) => return Err(error),
                    }
                }
            }
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Err(error) => return Err(error),
    }

    let (temp_path, mut file) = loop {
        let id = NEXT_FILE_ID.fetch_add(1, Ordering::Relaxed);
        let candidate = suffixed_path(path, &format!(".{}.{}.tmp", std::process::id(), id));
        match create_private_file(&candidate) {
            Ok(file) => break (candidate, file),
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(error),
        }
    };
    let result = file
        .write_all(contents)
        .and_then(|_| file.sync_all())
        .and_then(|_| fs::rename(&temp_path, path));
    if result.is_err() {
        let _ = fs::remove_file(&temp_path);
    }
    result
}

#[cfg(test)]
pub(crate) mod test_support {
    use super::*;

    pub(crate) struct TempDir(pub PathBuf);

    impl TempDir {
        pub(crate) fn new() -> Self {
            loop {
                let id = NEXT_FILE_ID.fetch_add(1, Ordering::Relaxed);
                let path = std::env::temp_dir()
                    .join(format!("nimbus-storage-test-{}-{id}", std::process::id()));
                match fs::create_dir(&path) {
                    Ok(()) => return Self(path),
                    Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
                    Err(error) => panic!("create temporary test directory: {error}"),
                }
            }
        }

        pub(crate) fn path(&self, filename: &str) -> PathBuf {
            self.0.join(filename)
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use test_support::TempDir;

    #[test]
    fn replaces_complete_file_without_leaving_temporary_files() {
        let dir = TempDir::new();
        let path = dir.path("data");
        fs::write(&path, b"old").unwrap();
        atomic_write(&path, b"new contents", |_| true).unwrap();
        assert_eq!(fs::read(&path).unwrap(), b"new contents");
        assert_eq!(fs::read_dir(&dir.0).unwrap().count(), 1);
    }

    #[test]
    fn preserves_invalid_bytes_without_replacing_previous_backup() {
        let dir = TempDir::new();
        let path = dir.path("data");
        fs::write(&path, [0xff, 0xfe]).unwrap();
        fs::write(dir.path("data.invalid"), b"previous corruption").unwrap();
        atomic_write(&path, b"valid", |_| false).unwrap();
        assert_eq!(
            fs::read(dir.path("data.invalid")).unwrap(),
            b"previous corruption"
        );
        assert_eq!(fs::read(dir.path("data.invalid.1")).unwrap(), [0xff, 0xfe]);
        assert_eq!(fs::read(&path).unwrap(), b"valid");
    }

    #[test]
    fn rejects_unreadable_target_without_replacing_it() {
        let dir = TempDir::new();
        let path = dir.path("data");
        fs::create_dir(&path).unwrap();
        assert!(atomic_write(&path, b"new", |_| true).is_err());
        assert!(path.is_dir());
        assert_eq!(fs::read_dir(&dir.0).unwrap().count(), 1);
    }
}
