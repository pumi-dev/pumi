//! The functions the app's windowing answers: the clipboard, a picker, opening a path. GPUI does
//! them on every OS, and this crate doesn't depend on it: the app installs a [`Desktop`] as it
//! starts ([`install_desktop`]). A process without one (`pumid`, a plugin's host) answers them
//! as unavailable.
use pumi_api::{Cx, Error, ErrorKind, Nullable, Optional, Result, api};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

/// What the app does for these functions. Called on any thread; it may wait for the app's main
/// thread, so a caller doesn't call it on one that thread waits for.
pub trait Desktop: Send + Sync {
    /// The clipboard's text, if it holds some.
    fn clipboard_text(&self) -> Result<Option<String>>;
    /// Puts `text` on the clipboard.
    fn set_clipboard_text(&self, text: String) -> Result<()>;
    /// Shows a picker and waits for it: the paths chosen, `None` when it was cancelled.
    fn pick(&self, pick: Pick) -> Result<Option<Vec<PathBuf>>>;
    /// Opens `path` in its default app.
    fn open(&self, path: &Path) -> Result<()>;
    /// Shows `path` in the system's file manager.
    fn reveal(&self, path: &Path) -> Result<()>;
}

/// The picker [`Desktop::pick`] shows.
#[derive(Clone, Debug, PartialEq)]
pub struct Pick {
    pub kind: PickKind,
    pub multiple: bool,
    /// The picker's prompt: on every OS GPUI makes it the button that chooses.
    pub prompt: Option<String>,
}

static DESKTOP: OnceLock<Box<dyn Desktop>> = OnceLock::new();

/// Makes `desktop` answer these functions in this process (once: later calls are ignored).
pub fn install_desktop(desktop: impl Desktop + 'static) {
    let _ = DESKTOP.set(Box::new(desktop));
}

fn desktop() -> Result<&'static dyn Desktop> {
    DESKTOP
        .get()
        .map(Box::as_ref)
        .ok_or_else(|| Error::new(ErrorKind::ProviderUnavailable, "only the app has a clipboard, pickers and windows"))
}

/// The clipboard's text: `null` when it holds none.
#[api(name = "clipboard.get", effect = "read", runs_on = "client", example(args = "[]", result = "\"hello\""))]
pub fn clipboard_get(_cx: &mut Cx) -> Result<Option<String>> {
    Ok(desktop()?.clipboard_text()?.filter(|text| !text.is_empty()))
}

/// Put text on the clipboard; `null` clears it.
#[api(
    name = "clipboard.set",
    runs_on = "client",
    example(args = r#"["hello"]"#, result = "null"),
    example(args = "[null]", result = "null", desc = "Cleared.")
)]
pub fn clipboard_set(_cx: &mut Cx, text: Nullable<String>) -> Result<()> {
    desktop()?.set_clipboard_text(text.0.unwrap_or_default())
}

/// What a picker picks.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum PickKind {
    /// Files to open.
    #[default]
    File,
    /// A folder.
    Directory,
}

/// What `fs.pick` takes.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PickOpts {
    /// Files, or a folder (`file`).
    #[serde(default)]
    pub kind: Optional<PickKind>,
    /// Allow choosing more than one file.
    #[serde(default)]
    pub multiple: Optional<bool>,
    /// What the picker's button says (`Open Local Folder`).
    #[serde(default)]
    pub title: Optional<String>,
    /// Where it starts: a file's folder, or a folder. Taken but not followed: the system's
    /// picker opens where it last was.
    #[serde(default)]
    pub path: Optional<String>,
}

/// What `fs.pick` answers: one path, or the ones a `multiple` picker chose.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Picked {
    One(String),
    Many(Vec<String>),
}

impl schemars::JsonSchema for Picked {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        "Picked".into()
    }
    fn json_schema(_: &mut schemars::SchemaGenerator) -> schemars::Schema {
        schemars::json_schema!({
            "oneOf": [
                { "type": "string" },
                { "type": "array", "items": { "type": "string" } }
            ]
        })
    }
}

/// Choose files to open, or a folder: a native picker. Answers the path chosen, the paths when
/// it takes more than one, and `null` when it was cancelled.
#[api(
    name = "fs.pick",
    effect = "read",
    runs_on = "client",
    example(args = r#"[{"kind": "file", "multiple": true}]"#, result = r#"["/tmp/a.log"]"#),
    example(args = r#"[{"kind": "directory", "title": "Where to clone"}]"#, result = r#""/tmp""#)
)]
pub fn pick(_cx: &mut Cx, opts: Optional<PickOpts>) -> Result<Option<Picked>> {
    let opts = opts.into_option().unwrap_or_default();
    let kind = opts.kind.0.unwrap_or_default();
    let multiple = opts.multiple.0.unwrap_or(false);
    Ok(picked(multiple, desktop()?.pick(Pick { kind, multiple, prompt: opts.title.0 })?))
}

/// What a picker chose, as `fs.pick` answers it: none chosen is cancelled.
fn picked(multiple: bool, chosen: Option<Vec<PathBuf>>) -> Option<Picked> {
    let paths: Vec<String> = chosen.into_iter().flatten().map(|path| path.to_string_lossy().into_owned()).collect();
    match (multiple, paths.len()) {
        (_, 0) => None,
        (true, _) => Some(Picked::Many(paths)),
        (false, _) => Some(Picked::One(paths.into_iter().next().expect("one path"))),
    }
}

/// What `fs.open` takes beside the path.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct OpenOpts {
    /// Show it in the file manager (the Finder, Explorer) instead of opening it.
    #[serde(default)]
    pub reveal: Optional<bool>,
    /// The app to open it with: by name on macOS (`Safari`), a program elsewhere (`code`).
    #[serde(default)]
    pub with: Optional<String>,
}

/// Open a path in its default app, or in the one `with` names; `reveal` shows it in the file
/// manager instead.
#[api(
    name = "fs.open",
    runs_on = "client",
    example(args = r#"["/tmp", {"reveal": true}]"#, result = "null"),
    example(args = r#"["/tmp/a.md", {"with": "Safari"}]"#, result = "null")
)]
pub fn open(_cx: &mut Cx, path: String, opts: Optional<OpenOpts>) -> Result<()> {
    let opts = opts.into_option().unwrap_or_default();
    if opts.reveal.0.unwrap_or(false) {
        return desktop()?.reveal(Path::new(&path));
    }
    match &opts.with.0 {
        Some(app) => open_with(app, &path),
        None => desktop()?.open(Path::new(&path)),
    }
}

/// Opens `path` with `app`: macOS's `open -a` finds an app by its name; elsewhere `app` is a
/// program, started with the path and not waited for.
fn open_with(app: &str, path: &str) -> Result<()> {
    if cfg!(target_os = "macos") {
        return crate::tool("open", &["-a", app, path]).map(|_| ());
    }
    let mut child = std::process::Command::new(app)
        .arg(path)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .map_err(|err| Error::exception(format!("{app}: {err}")))?;
    std::thread::spawn(move || child.wait());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{Picked, picked};
    use std::path::PathBuf;
    #[test]
    fn a_pick_answers_one_path_or_many_and_cancelled_is_none() {
        let chosen = || Some(vec![PathBuf::from("/tmp/a"), PathBuf::from("/tmp/b")]);
        assert_eq!(picked(false, chosen()), Some(Picked::One("/tmp/a".into())));
        assert_eq!(picked(true, chosen()), Some(Picked::Many(vec!["/tmp/a".into(), "/tmp/b".into()])));
        assert_eq!(picked(true, None), None);
        assert_eq!(picked(false, Some(Vec::new())), None);
    }
}
