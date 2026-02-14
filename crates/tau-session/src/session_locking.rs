//! Session file lock lifecycle helpers for cross-process coordination.
use super::*;

pub(super) struct LockGuard {
    path: PathBuf,
}

impl Drop for LockGuard {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}

pub(super) fn acquire_lock(
    path: &Path,
    timeout: Duration,
    stale_after: Duration,
) -> Result<LockGuard> {
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent)
                .with_context(|| format!("failed to create lock directory {}", parent.display()))?;
        }
    }

    let start = SystemTime::now();

    loop {
        match OpenOptions::new().create_new(true).write(true).open(path) {
            Ok(mut file) => {
                let pid = std::process::id();
                let _ = writeln!(file, "{pid}");
                return Ok(LockGuard {
                    path: path.to_path_buf(),
                });
            }
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                if stale_after > Duration::ZERO && reclaim_stale_lock(path, stale_after) {
                    continue;
                }
                let elapsed = SystemTime::now().duration_since(start).unwrap_or_default();
                if elapsed >= timeout {
                    bail!("timed out acquiring lock {}", path.display());
                }
                thread::sleep(Duration::from_millis(50));
            }
            Err(error) => {
                return Err(anyhow!(
                    "failed to acquire lock {}: {error}",
                    path.display()
                ));
            }
        }
    }
}

fn reclaim_stale_lock(path: &Path, stale_after: Duration) -> bool {
    let metadata = match fs::metadata(path) {
        Ok(metadata) => metadata,
        Err(_) => return false,
    };
    let modified = match metadata.modified() {
        Ok(modified) => modified,
        Err(_) => return false,
    };
    let age = match SystemTime::now().duration_since(modified) {
        Ok(age) => age,
        Err(_) => Duration::ZERO,
    };
    if age < stale_after {
        return false;
    }

    fs::remove_file(path).is_ok()
}
