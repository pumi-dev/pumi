//! `pumi plugin new [DIR] [--theme default]`, a plugin to start from, and `pumi plugin dev DIR
//! [--inspect]`, which links one and reloads its worker each time a file of it changes.
use crate::client::Client;
use pumi_api::{Error, ErrorKind};
use serde_json::json;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

fn io(path: &Path, err: std::io::Error) -> Error {
    Error::new(ErrorKind::Exception, format!("{}: {err}", path.display()))
}

/// A manifest name from a directory's: lowercase letters, digits, `_` and `-`, from a letter.
pub(crate) fn name_of(dir: &Path) -> String {
    let base = dir.file_name().map(|n| n.to_string_lossy().to_lowercase()).unwrap_or_default();
    let base = base.strip_prefix("pumi-").unwrap_or(&base);
    let clean: String =
        base.chars().map(|c| if c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-' { c } else { '_' }).collect();
    let clean = clean.trim_start_matches(|c: char| !c.is_ascii_lowercase());
    if clean.is_empty() { "hello".into() } else { clean.into() }
}

/// The template (runtime/app/template), by path, with `{{name}}` where the plugin's name goes and
/// `{{owner}}` its owner's.
const TEMPLATE: &[(&str, &str)] = &[
    ("package.json", include_str!("../../../runtime/app/template/package.json")),
    ("tsconfig.json", include_str!("../../../runtime/app/template/tsconfig.json")),
    ("src/main.ts", include_str!("../../../runtime/app/template/src/main.ts")),
    ("src/ui.tsx", include_str!("../../../runtime/app/template/src/ui.tsx")),
    ("src/qr.ts", include_str!("../../../runtime/app/template/src/qr.ts")),
    ("AGENTS.md", include_str!("../../../runtime/app/template/AGENTS.md")),
    (".gitignore", include_str!("../../../runtime/app/template/.gitignore")),
];

fn files(name: &str, owner: &str) -> Vec<(&'static str, String)> {
    TEMPLATE.iter().map(|(path, text)| (*path, text.replace("{{name}}", name).replace("{{owner}}", owner))).collect()
}

/// Who a new plugin is by: the GitHub user git knows (`github.user`), else the login name, as an
/// owner is written (lowercase, `-` as `_`). Its id is `owner/name` while linked and once
/// published from that owner's GitHub, so its options and keys keep their names.
pub(crate) fn owner_guess() -> String {
    let git = std::process::Command::new("git").args(["config", "--get", "github.user"]).output().ok();
    let from_git = git.filter(|o| o.status.success()).map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string());
    let raw = from_git.filter(|s| !s.is_empty()).or_else(|| std::env::var("USER").ok()).unwrap_or_else(|| "me".into());
    let clean: String = raw
        .to_lowercase()
        .chars()
        .map(|c| if c.is_ascii_lowercase() || c.is_ascii_digit() { c } else { '_' })
        .collect();
    let clean = clean.trim_start_matches('_');
    if clean.is_empty() { "me".into() } else { clean.into() }
}

/// The theme template (runtime/app/template/theme): a plugin whose UI theme is the default's
/// colours and metrics, to change from there.
const THEME: &str = include_str!("../../../runtime/app/template/theme/theme.tsx");

/// A theme plugin's manifest: its one theme is `themes/<name>`.
fn theme_package(name: &str, owner: &str) -> String {
    let manifest = serde_json::json!({
        "name": format!("pumi-{name}"),
        "version": "0.1.0",
        "description": format!("{name}: a UI theme for pumi, from the default."),
        "type": "module",
        "scripts": { "check": "tsc --noEmit" },
        "devDependencies": { "@types/bun": "1.3.14", "@types/react": "19.2.2", "typescript": "5.9.3" },
        "pumi": { "name": name, "owner": owner, "api": pumi_api::API_LEVEL, "themes": [format!("themes/{name}")] },
    });
    serde_json::to_string_pretty(&manifest).unwrap_or_default() + "\n"
}

/// A theme plugin's files, by their path in it.
fn theme_files(name: &str, owner: &str) -> Vec<(String, String)> {
    vec![
        (format!("themes/{name}/theme.tsx"), THEME.replace("{{name}}", name)),
        ("package.json".into(), theme_package(name, owner)),
        (
            "tsconfig.json".into(),
            TEMPLATE
                .iter()
                .find(|(path, _)| *path == "tsconfig.json")
                .map(|(_, text)| text.to_string())
                .unwrap_or_default(),
        ),
        (
            ".gitignore".into(),
            TEMPLATE
                .iter()
                .find(|(path, _)| *path == ".gitignore")
                .map(|(_, text)| text.to_string())
                .unwrap_or_default(),
        ),
    ]
}

/// `pumi plugin new [DIR] [--theme default]`: a plugin in DIR (here by default), which must not
/// have one already. A theme plugin draws the window; any other provides functions.
pub fn new(dir: Option<&str>, theme: bool) -> Result<i32, Error> {
    let dir = std::path::absolute(dir.unwrap_or(".")).map_err(|e| io(Path::new("."), e))?;
    if dir.join("package.json").exists() {
        return Err(Error::new(ErrorKind::InvalidParams, format!("{} has a package.json already", dir.display())));
    }
    let name = name_of(&dir);
    let owner = owner_guess();
    let made: Vec<(String, String)> = if theme {
        theme_files(&name, &owner)
    } else {
        files(&name, &owner).into_iter().map(|(path, text)| (path.to_string(), text)).collect()
    };
    for (rel, text) in made {
        let path = dir.join(rel);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| io(parent, e))?;
        }
        std::fs::write(&path, text).map_err(|e| io(&path, e))?;
    }
    println!("made the {} {owner}/{name} in {}", if theme { "theme" } else { "plugin" }, dir.display());
    if theme {
        println!(
            "next: `pumi plugin dev {}` links it, and Settings → Appearance draws the window with it",
            dir.display()
        );
    } else {
        println!("next: `pumi plugin dev {}` runs it in pumi, reloading it when you save", dir.display());
    }
    Ok(0)
}

/// The newest change under `dir` (its `node_modules` and `.git` aside), and how many files.
fn stamp(dir: &Path) -> (SystemTime, usize) {
    let mut newest = SystemTime::UNIX_EPOCH;
    let mut count = 0;
    let mut todo = vec![dir.to_path_buf()];
    while let Some(at) = todo.pop() {
        for entry in std::fs::read_dir(&at).into_iter().flatten().flatten() {
            let name = entry.file_name();
            if name == "node_modules" || name == ".git" {
                continue;
            }
            let Ok(meta) = entry.metadata() else { continue };
            if meta.is_dir() {
                todo.push(entry.path());
            } else {
                count += 1;
                newest = newest.max(meta.modified().unwrap_or(SystemTime::UNIX_EPOCH));
            }
        }
    }
    (newest, count)
}

/// `pumi plugin dev DIR [--inspect]`: links DIR, and asks the app to reload it whenever a file in it
/// changes, printing its log as it grows. `--inspect` runs it under Bun's inspector.
pub fn dev(client: &mut Client, dir: &str, inspect: bool) -> Result<i32, Error> {
    let dir: PathBuf = std::path::absolute(dir).map_err(|e| io(Path::new(dir), e))?;
    let src = dir.display().to_string();
    let plugin = pumi_api::manifest::read(&dir, Some(&src)).map_err(|e| Error::new(ErrorKind::InvalidParams, e))?;
    client.call("plugin.add", json!([src, { "linked": true }]))?;
    let reload = |client: &mut Client| {
        let data = json!({ "name": plugin.id, "inspect": inspect });
        client.call("event.emit", json!(["plugin.reloadRequested", { "data": data }]))
    };
    reload(client)?;
    let log = crate::plugin::log_path(&plugin.id);
    let mut read = std::fs::metadata(&log).map(|m| m.len()).unwrap_or(0);
    println!("{} is linked; it reloads when a file in {} changes (Ctrl-C ends this)", plugin.id, dir.display());
    let mut last = stamp(&dir);
    loop {
        std::thread::sleep(Duration::from_millis(250));
        let now = stamp(&dir);
        if now != last {
            last = now;
            reload(client)?;
            println!("reloaded {}", plugin.id);
        }
        if let Ok(text) = std::fs::read(&log) {
            let text = if (text.len() as u64) < read { &text[..] } else { &text[read as usize..] };
            print!("{}", String::from_utf8_lossy(text));
            read = std::fs::metadata(&log).map(|m| m.len()).unwrap_or(0);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn a_new_package_s_manifest_is_one_pumi_reads() {
        assert_eq!(name_of(Path::new("/x/pumi-My Queue")), "my_queue");
        assert_eq!(name_of(Path::new("/x/9lives")), "lives");
        let dir = std::env::temp_dir().join(format!("pumi-plugin-new-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        for (rel, text) in files("queue", "me") {
            let path = dir.join(rel);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, text).unwrap();
        }
        let plugin = pumi_api::manifest::read(&dir, None).unwrap();
        assert_eq!((plugin.id.as_str(), plugin.main.as_deref()), ("me/queue", Some("src/main.ts")));
        assert_eq!(plugin.manifest.api, pumi_api::API_LEVEL);
        assert_eq!(plugin.manifest.contributes.commands[0].id, "hello");
        assert!(plugin.manifest.contributes.slots.contains_key("panel"));
        assert_eq!(plugin.manifest.contributes.settings.sections[0].component, "Settings");
        assert!(plugin.manifest.contributes.options.contains_key("position"));
        assert!(files("queue", "me").iter().all(|(_, text)| !text.contains("{{name}}") && !text.contains("{{owner}}")));
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
