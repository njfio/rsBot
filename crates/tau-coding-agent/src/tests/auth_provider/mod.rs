use std::path::Path;

mod auth_and_provider;
mod commands_and_packages;
mod runtime_and_startup;
mod session_and_doctor;

fn make_script_executable(path: &Path) {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut permissions = std::fs::metadata(path).expect("metadata").permissions();
        permissions.set_mode(0o755);
        std::fs::set_permissions(path, permissions).expect("set executable permissions");
    }
}

fn write_route_table_fixture(path: &Path, body: &str) {
    std::fs::write(path, body).expect("write route table fixture");
}
