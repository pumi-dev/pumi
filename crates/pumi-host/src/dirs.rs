pub use pumi_proto::paths::stdpath;
use std::path::{Path, PathBuf};
pub fn config() -> PathBuf {
    stdpath("config").expect("config is a stdpath")
}
pub fn runtime_dir() -> Option<PathBuf> {
    if let Some(dir) = std::env::var_os("PUMI_RUNTIME").filter(|v| !v.is_empty()) {
        return Some(PathBuf::from(dir));
    }
    let is_runtime = |dir: &Path| dir.join("app").is_dir();
    if let Ok(exe) = std::env::current_exe() {
        let exe = exe.canonicalize().unwrap_or(exe);
        for up in exe.ancestors().skip(1).take(3) {
            for candidate in [up.join("Resources/runtime"), up.join("runtime"), up.join("share/pumi/runtime")] {
                if is_runtime(&candidate) {
                    return Some(candidate);
                }
            }
        }
    }
    let checkout = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../runtime");
    is_runtime(&checkout).then(|| checkout.canonicalize().unwrap_or(checkout))
}
pub fn expand(dir: &str) -> PathBuf {
    match dir.strip_prefix("~/") {
        Some(rest) => pumi_proto::paths::home_dir().join(rest),
        None => PathBuf::from(dir),
    }
}
pub fn runtimepath(k: &crate::kernel::Kernel) -> Vec<PathBuf> {
    k.global_option("runtimepath")
        .as_str()
        .unwrap_or("")
        .split(',')
        .map(str::trim)
        .filter(|d| !d.is_empty())
        .map(expand)
        .collect()
}
pub fn runtime_files(k: &crate::kernel::Kernel, rel: &str) -> Vec<PathBuf> {
    runtimepath(k).into_iter().map(|d| d.join(rel)).filter(|p| p.is_file()).collect()
}
pub fn write_atomic(path: &Path, text: &str) -> std::io::Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let tmp = path.with_extension(format!("{}.tmp", path.extension().and_then(|e| e.to_str()).unwrap_or_default()));
    std::fs::write(&tmp, text)?;
    std::fs::rename(&tmp, path)
}
