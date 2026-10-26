//! The app's native half: a napi addon the app's Bun process loads. It holds the renderer
//! (pumi-gpuix, re-exported), the connection to pumid, the terminals and the OS's services; the
//! window itself is React (runtime/app).
pub use pumi_gpuix::*;

mod assets;
mod bins;
mod diagnostics;
mod engine;
mod fonts;
mod haptics;
mod menus;
mod runtime;
mod settings;
mod system;
mod term;
mod theme;
mod ui;
mod updater;

#[napi_derive::module_init]
fn init() {
    ui::elements::register();
    pumi_gpuix::app::set_assets(assets::Assets);
    theme::chrome::install();
    pumi_gpuix::app::on_reopen(|| engine::emit("app", serde_json::json!({ "reopen": true })));
    pumi_gpuix::app::on_open_urls(engine::opened);
    // The app's menu bar replaces the renderer's minimal one, which `app::ensure` installs first.
    pumi_gpuix::app::on_start(menus::install);
}

static SAFE: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
/// Safe mode (`--safe`): the default theme, and no plugin runs. The app's JS decides it.
pub fn safe_mode() -> bool {
    SAFE.load(std::sync::atomic::Ordering::Relaxed)
}
pub(crate) fn set_safe_mode(on: bool) {
    SAFE.store(on, std::sync::atomic::Ordering::Relaxed);
}

pub fn home_dir() -> std::path::PathBuf {
    pumi_sys::paths::home()
}
pub fn config_dir() -> std::path::PathBuf {
    pumi_proto::paths::config_dir()
}
/// `path` as the user reads it: under their home, `~/` and the rest (`~\` on Windows).
pub fn display_path(path: &std::path::Path) -> String {
    match path.strip_prefix(home_dir()) {
        Ok(rest) => format!("~{}{}", std::path::MAIN_SEPARATOR, rest.display()),
        Err(_) => path.display().to_string(),
    }
}
pub(crate) fn log_panics(data_dir: Option<std::path::PathBuf>) {
    let dir = data_dir.map_or_else(|| pumi_proto::paths::Paths::resolve().data, Into::into);
    let path = dir.join("panic.log");
    let default = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        use std::io::Write;
        let backtrace = std::backtrace::Backtrace::force_capture();
        let thread = std::thread::current();
        let when = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs());
        if let Ok(mut file) = std::fs::OpenOptions::new().create(true).append(true).open(&path) {
            let _ = writeln!(
                file,
                "--- {} {when} thread {}: {info}\n{backtrace}\n",
                env!("CARGO_PKG_VERSION"),
                thread.name().unwrap_or("?"),
            );
        }
        default(info);
    }));
}
