//! CLI executable discovery/validation helpers for provider adapters.
//!
//! Provider CLI clients use these checks to ensure configured binaries exist and
//! are executable before spawning subprocesses, preventing ambiguous runtime
//! failures from missing or non-executable command paths.

use std::path::Path;

#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;

fn is_executable_file(path: &Path) -> bool {
    let Ok(metadata) = std::fs::metadata(path) else {
        return false;
    };
    if !metadata.is_file() {
        return false;
    }
    #[cfg(unix)]
    {
        metadata.permissions().mode() & 0o111 != 0
    }
    #[cfg(not(unix))]
    {
        true
    }
}

pub fn is_executable_available(executable: &str) -> bool {
    let trimmed = executable.trim();
    if trimmed.is_empty() {
        return false;
    }

    let candidate = Path::new(trimmed);
    if candidate.is_absolute() || trimmed.contains(std::path::MAIN_SEPARATOR) {
        return is_executable_file(candidate);
    }

    let Some(path_var) = std::env::var_os("PATH") else {
        return false;
    };
    for mut path in std::env::split_paths(&path_var) {
        path.push(trimmed);
        if is_executable_file(&path) {
            return true;
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    #[cfg(unix)]
    use std::os::unix::fs::PermissionsExt;

    static PATH_ENV_LOCK: Mutex<()> = Mutex::new(());

    #[cfg(unix)]
    fn set_exec(path: &Path, mode: u32) {
        let mut perms = std::fs::metadata(path).expect("metadata").permissions();
        perms.set_mode(mode);
        std::fs::set_permissions(path, perms).expect("set perms");
    }

    #[test]
    fn unit_is_executable_available_rejects_empty() {
        assert!(!is_executable_available(""));
        assert!(!is_executable_available("   "));
    }

    #[cfg(unix)]
    #[test]
    fn integration_is_executable_available_checks_absolute_paths() {
        let temp = tempfile::tempdir().expect("tempdir");
        let path = temp.path().join("mock-bin");
        std::fs::write(&path, "#!/bin/sh\n").expect("write script");
        set_exec(&path, 0o755);
        assert!(is_executable_available(path.to_str().unwrap_or_default()));

        set_exec(&path, 0o644);
        assert!(!is_executable_available(path.to_str().unwrap_or_default()));
    }

    #[cfg(unix)]
    #[test]
    fn functional_is_executable_available_checks_path_lookup() {
        let _guard = PATH_ENV_LOCK.lock().expect("path env lock");
        let temp = tempfile::tempdir().expect("tempdir");
        let path = temp.path().join("mock-gemini");
        std::fs::write(&path, "#!/bin/sh\n").expect("write script");
        set_exec(&path, 0o755);

        let original = std::env::var_os("PATH");
        std::env::set_var("PATH", temp.path());
        let available = is_executable_available("mock-gemini");
        if let Some(value) = original {
            std::env::set_var("PATH", value);
        } else {
            std::env::remove_var("PATH");
        }

        assert!(available);
    }
}
