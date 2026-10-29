use pumi_sys::paths::Dir;
use std::path::{Path, PathBuf};
pub const DATA_DIR_ENV: &str = "PUMI_DATA_DIR";
pub const APPNAME_ENV: &str = "PUMI_APPNAME";
pub fn appname() -> String {
    match std::env::var(APPNAME_ENV) {
        Ok(name) if !name.is_empty() && !name.contains(['/', '\\']) && name != "." && name != ".." => name,
        _ if cfg!(debug_assertions) => "pumi-dev".into(),
        _ => "pumi".into(),
    }
}
pub fn export_appname() {
    if std::env::var_os(APPNAME_ENV).is_none_or(|v| v.is_empty()) {
        unsafe { std::env::set_var(APPNAME_ENV, appname()) };
    }
}
/// The name people see (the menu bar's "About", "Hide", "Quit"): `pumi Dev` for `pumi-dev`, so it
/// and pumi can run side by side, as Zed Dev does beside Zed.
pub fn display_name() -> String {
    match appname().as_str() {
        "pumi-dev" => "pumi Dev".into(),
        name => name.into(),
    }
}
pub fn config_dir() -> PathBuf {
    pumi_sys::paths::dir(Dir::Config, &appname())
}
/// Neovim's `stdpath()`: pumi's `config`, `data`, `state` and `cache` directories, each under its
/// XDG base (where it is set, else the OS's place for it) and named for `$PUMI_APPNAME` (`pumi`,
/// `pumi-dev` in a debug build).
pub fn stdpath(kind: &str) -> Option<PathBuf> {
    let kind = match kind {
        "config" => Dir::Config,
        "data" => Dir::Data,
        "state" => Dir::State,
        "cache" => Dir::Cache,
        _ => return None,
    };
    Some(pumi_sys::paths::dir(kind, &appname()))
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum File {
    NativeSocket,
    Lock,
    Session,
    History,
    Log,
    LoginEnv,
    Handoff,
    PumiSocket,
    GridSocket,
    ServerLock,
}
/// Each file's name in the data directory. A `.sock` is a socket, at [`pumi_sys::ipc::address`].
const FILES: &[(File, &str)] = &[
    (File::NativeSocket, "pumid.sock"),
    (File::Lock, "pumid.lock"),
    (File::Session, "session.json"),
    (File::History, "history"),
    (File::Log, "pumid.log"),
    (File::LoginEnv, "login-env.json"),
    (File::Handoff, "handoff"),
    (File::PumiSocket, "pumi.sock"),
    (File::GridSocket, "grid.sock"),
    (File::ServerLock, "server.lock"),
];
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Paths {
    pub data: PathBuf,
}
impl Paths {
    pub fn resolve() -> Self {
        let data = std::env::var_os(DATA_DIR_ENV)
            .filter(|dir| !dir.is_empty())
            .map(PathBuf::from)
            .unwrap_or_else(default_data_dir);
        Self { data }
    }
    pub fn at(data: impl Into<PathBuf>) -> Self {
        Self { data: data.into() }
    }
    pub fn file(&self, file: File) -> PathBuf {
        let name = FILES.iter().find(|(f, _)| *f == file).map(|(_, name)| *name).unwrap_or("unknown");
        match name.ends_with(".sock") {
            true => pumi_sys::ipc::address(&self.data, name),
            false => self.data.join(name),
        }
    }
    pub fn native_socket(&self) -> PathBuf {
        self.file(File::NativeSocket)
    }
    pub fn history(&self, terminal_id: &str) -> PathBuf {
        self.file(File::History).join(format!("{}.ansi", sanitize(terminal_id)))
    }
    pub fn ensure(&self) -> std::io::Result<()> {
        std::fs::create_dir_all(&self.data)?;
        restrict(&self.data, 0o700)
    }
}
fn sanitize(name: &str) -> String {
    name.chars().map(|c| if c.is_ascii_alphanumeric() || c == '_' || c == '-' { c } else { '_' }).collect()
}
pub fn home_dir() -> PathBuf {
    pumi_sys::paths::home()
}
fn default_data_dir() -> PathBuf {
    pumi_sys::paths::dir(Dir::Server, &appname())
}
/// Lets only this user in (`0o700`, `0o600` on Unix).
pub fn restrict(path: &Path, mode: u32) -> std::io::Result<()> {
    pumi_sys::paths::restrict(path, mode)
}
pub fn display(path: &Path) -> String {
    match path.strip_prefix(home_dir()) {
        Ok(rest) => format!("~{}{}", std::path::MAIN_SEPARATOR, rest.display()),
        Err(_) => path.display().to_string(),
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn every_file_has_a_name() {
        let paths = Paths::at("/d");
        for (file, name) in FILES {
            let want = match name.ends_with(".sock") {
                true => pumi_sys::ipc::address(Path::new("/d"), name),
                false => Path::new("/d").join(name),
            };
            assert_eq!(paths.file(*file), want);
        }
        assert_eq!(paths.history("term_1:x"), Path::new("/d/history/term_1_x.ansi"));
    }
}
