use pumi_sys::paths::{exe, is_executable};
use std::path::{Path, PathBuf};
fn candidate_dirs() -> Vec<PathBuf> {
    let home = crate::home_dir();
    vec![
        home.join(".local/bin"),
        home.join(".cargo/bin"),
        PathBuf::from("/opt/homebrew/bin"),
        PathBuf::from("/usr/local/bin"),
    ]
}
fn find(name: &str) -> Option<PathBuf> {
    let paths = std::env::var_os("PATH")?;
    std::env::split_paths(&paths).map(|dir| dir.join(name)).find(|path| is_executable(path))
}
pub fn pumid_bin() -> PathBuf {
    if let Some(configured) = std::env::var_os("PUMID_BIN") {
        return PathBuf::from(configured);
    }
    let name = exe("pumid");
    let beside = std::env::current_exe().ok().and_then(|app| app.parent().map(|dir| dir.join(&name)));
    beside.filter(|path| is_executable(path)).or_else(|| find(&name)).unwrap_or_else(|| PathBuf::from(name))
}
/// The name an installed bundle runs as (`Contents/Resources/appname`, which scripts/install.sh
/// writes: `pumi`, or `pumi-dev` for pumi Dev). It wins over an inherited `$PUMI_APPNAME`: `open`
/// from a tab of the other pumi passes that tab's, and pumi Dev would use pumi's server and files.
pub fn adopt_bundled_appname() {
    let Some(file) =
        std::env::current_exe().ok().and_then(|exe| Some(exe.parent()?.parent()?.join("Resources/appname")))
    else {
        return;
    };
    if let Ok(name) = std::fs::read_to_string(file)
        && !name.trim().is_empty()
    {
        unsafe { std::env::set_var(pumi_proto::paths::APPNAME_ENV, name.trim()) };
    }
}
/// Puts the directories programs are installed to on the app's `PATH`. An app opened from the
/// Dock or Finder starts with the system's (`/usr/bin:/bin:/usr/sbin:/sbin`); then the login
/// shell's `PATH`, as the server caches it for terminals (`login_env`, `login-env.json`), goes
/// first, so plugins and the programs they start find what a terminal finds (a `node` nvm put on
/// it). An app started from a shell keeps the shell's.
pub fn hydrate_path(login_env: &Path) {
    let inherited = std::env::var_os("PATH").unwrap_or_default();
    let inherited: Vec<PathBuf> = std::env::split_paths(&inherited).collect();
    // `PUMI_LOGIN_ENV=0` keeps the server from reading the login shell: the app does not either.
    let wanted = std::env::var("PUMI_LOGIN_ENV").as_deref() != Ok("0");
    let login = if wanted && is_short(&inherited, &crate::home_dir()) { login_path(login_env) } else { Vec::new() };
    let candidates: Vec<PathBuf> = candidate_dirs().into_iter().filter(|dir| dir.is_dir()).collect();
    if let Ok(joined) = std::env::join_paths(merged([login, candidates, inherited])) {
        unsafe { std::env::set_var("PATH", joined) };
    }
}
/// Whether `path` is the one an app opened from the desktop gets: none of its directories is in
/// the user's home. A shell's has the user's own directories in it.
fn is_short(path: &[PathBuf], home: &Path) -> bool {
    !path.iter().any(|dir| dir.starts_with(home))
}
/// The `PATH` in the login environment the server cached (`{"PATH": …, …}`), or none.
fn login_path(cache: &Path) -> Vec<PathBuf> {
    let Some(env) = std::fs::read(cache)
        .ok()
        .and_then(|bytes| serde_json::from_slice::<std::collections::BTreeMap<String, String>>(&bytes).ok())
    else {
        return Vec::new();
    };
    env.get("PATH").map(|path| std::env::split_paths(path).collect()).unwrap_or_default()
}
/// The lists' directories in order, each once.
fn merged<const N: usize>(lists: [Vec<PathBuf>; N]) -> Vec<PathBuf> {
    let mut dirs: Vec<PathBuf> = Vec::new();
    for dir in lists.into_iter().flatten() {
        if !dir.as_os_str().is_empty() && !dirs.contains(&dir) {
            dirs.push(dir);
        }
    }
    dirs
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn path_hydration_keeps_what_was_there() {
        let before = std::env::var_os("PATH").unwrap_or_default();
        hydrate_path(Path::new("/nonexistent/login-env.json"));
        let after = std::env::var_os("PATH").expect("hydrate_path always sets PATH");
        let after: Vec<PathBuf> = std::env::split_paths(&after).collect();
        for dir in std::env::split_paths(&before) {
            assert!(dir.as_os_str().is_empty() || after.contains(&dir), "{dir:?} was dropped from PATH");
        }
        let shell = exe(if cfg!(windows) { "cmd" } else { "sh" });
        assert!(find(&shell).is_some(), "{shell} is on PATH but was not found");
    }
    #[test]
    fn a_desktop_path_is_short_and_a_shells_is_not() {
        let home = Path::new("/home/someone");
        let system: Vec<PathBuf> = ["/usr/bin", "/bin", "/usr/sbin", "/sbin"].map(PathBuf::from).into();
        assert!(is_short(&system, home));
        let shell = [vec![home.join(".nvm/versions/node/v24/bin")], system].concat();
        assert!(!is_short(&shell, home));
    }
    #[test]
    fn the_login_path_goes_first_each_directory_once() {
        let dir = std::env::temp_dir().join(format!("pumi-login-env-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let cache = dir.join("login-env.json");
        let login = std::env::join_paths(["/home/me/.nvm/bin", "/opt/homebrew/bin", "/usr/bin"]).unwrap();
        std::fs::write(&cache, serde_json::json!({ "PATH": login.to_str().unwrap(), "HOME": "/home/me" }).to_string())
            .unwrap();
        let login = login_path(&cache);
        assert_eq!(login, ["/home/me/.nvm/bin", "/opt/homebrew/bin", "/usr/bin"].map(PathBuf::from));
        let candidates = vec![PathBuf::from("/home/me/.local/bin"), PathBuf::from("/opt/homebrew/bin")];
        let inherited = vec![PathBuf::from("/usr/bin"), PathBuf::from("/bin")];
        assert_eq!(
            merged([login, candidates, inherited]),
            ["/home/me/.nvm/bin", "/opt/homebrew/bin", "/usr/bin", "/home/me/.local/bin", "/bin"].map(PathBuf::from)
        );
        assert!(login_path(&dir.join("missing.json")).is_empty());
        std::fs::write(&cache, "not json").unwrap();
        assert!(login_path(&cache).is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
