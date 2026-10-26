use std::io;
use std::path::{Path, PathBuf};
pub fn default_bin_dir() -> Option<PathBuf> {
    let exe = exe().ok()?;
    let dir = exe.parent()?;
    let shims = dir.join("shims");
    let cli = dir.join(pumi_sys::paths::exe("pumi-cli"));
    if cli.exists() {
        let _ = std::fs::create_dir_all(&shims);
        for name in SHIM_NAMES {
            let link = shims.join(pumi_sys::paths::exe(name));
            // Missing, or left naming where the app was before it moved.
            if !link.exists() {
                let _ = std::fs::remove_file(&link);
                let _ = pumi_sys::paths::link(&cli, &link);
            }
        }
    }
    let pumi = pumi_sys::paths::exe("pumi");
    [dir.join("../Resources/bin"), dir.to_path_buf(), shims]
        .into_iter()
        .find(|candidate| candidate.join(&pumi).exists())
        .map(|dir| std::fs::canonicalize(&dir).unwrap_or(dir))
}
const SHIM_NAMES: &[&str] = &["pumi"];
pub fn host_binary() -> Option<PathBuf> {
    if let Some(path) = std::env::var_os("PUMI_HOST").filter(|p| !p.is_empty()) {
        return Some(PathBuf::from(path));
    }
    let path = exe().ok()?.parent()?.join(pumi_sys::paths::exe("pumi-host"));
    path.is_file().then_some(path)
}
pub fn exe() -> io::Result<PathBuf> {
    static EXE: std::sync::OnceLock<Option<PathBuf>> = std::sync::OnceLock::new();
    EXE.get_or_init(|| {
        let exe = std::env::current_exe().ok()?;
        let text = exe.to_str()?;
        Some(PathBuf::from(text.strip_suffix(" (deleted)").unwrap_or(text)))
    })
    .clone()
    .ok_or_else(|| io::Error::other("this process's binary is unknown"))
}
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Image {
    pub path: PathBuf,
    /// When it was created (seconds, nanoseconds; 0 where the file system does not say), its
    /// size, when it was modified (seconds, nanoseconds). The tuple an older daemon wrote, so
    /// its handoff still reads.
    id: (u64, u64, u64, i64, i64),
}
impl Image {
    pub fn of(path: &Path) -> Option<Image> {
        let m = std::fs::metadata(path).ok()?;
        let since = |time: std::io::Result<std::time::SystemTime>| {
            time.ok().and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok()).unwrap_or_default()
        };
        let (created, modified) = (since(m.created()), since(m.modified()));
        let id = (
            created.as_secs(),
            created.subsec_nanos().into(),
            m.len(),
            modified.as_secs() as i64,
            modified.subsec_nanos().into(),
        );
        Some(Image { path: path.to_path_buf(), id })
    }
    pub fn outdated(&self) -> bool {
        Image::of(&self.path).is_some_and(|now| now != *self)
    }
}
#[derive(Clone, Debug, Default)]
pub struct Running {
    pub core: Option<Image>,
    pub host: Option<Image>,
}
