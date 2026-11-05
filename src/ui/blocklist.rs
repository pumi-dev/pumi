//! A page's blocklists: JSON files of WebKit's content-blocker rules, read when the page is made
//! and handed to the engine, which answers the requests they name before they leave.
//!
//! The rules are Safari's own format — what uBO Lite and AdGuard for Safari compile EasyList into —
//! so a list works as it is:
//!
//! ```json
//! [
//!   { "trigger": { "url-filter": "youtube\\.com/pagead/" }, "action": { "type": "block" } },
//!   { "trigger": { "url-filter": "^https?://www\\.youtube\\.com/" },
//!     "action": { "type": "css-display-none", "selector": ".ytp-ad-module" } }
//! ]
//! ```
//!
//! A relative path is the plugin's own (the folder its `ui.tsx` is in); an absolute one is the
//! user's, read as they could read it themselves.
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};

/// The most a list may be: a list is kilobytes, and a path that names a video is a mistake.
const MAX: u64 = 16 * 1024 * 1024;

/// The actions WebKit takes, and what each needs besides its `type`: a `url-filter`, a `selector`.
const ACTIONS: [(&str, bool, bool); 6] = [
    ("block", true, false),
    ("block-cookies", true, false),
    ("css-display-none", false, true),
    ("ignore-previous-rules", false, false),
    ("make-https", false, false),
    ("upgrade-insecure-requests", false, false),
];

/// One list, as a page is given it.
#[derive(Debug)]
pub(crate) struct List {
    /// What the engine compiles it under: the rules' own hash, so the same rules compile once
    /// however many pages and launches ask for them.
    pub(crate) identifier: String,
    /// The rules, as JSON.
    pub(crate) rules: String,
}

/// The compile identifier of `rules`: `content-` and twelve hex digits of their SHA-256.
pub(crate) fn identifier(rules: &str) -> String {
    let digest = Sha256::digest(rules.as_bytes());
    let hex: String = digest.iter().take(6).map(|byte| format!("{byte:02x}")).collect();
    format!("content-{hex}")
}

/// The lists `paths` name, read from `dir` (a plugin's folder) or from where an absolute path points.
pub(crate) fn read(paths: &[String], dir: Option<&Path>) -> Result<Vec<List>, String> {
    paths.iter().map(|path| one(path, dir)).collect()
}

/// One list: its file, its size checked, its rules checked, and the identifier they compile under.
fn one(path: &str, dir: Option<&Path>) -> Result<List, String> {
    let file = resolve(path, dir);
    let shown = file.display().to_string();
    let size = std::fs::metadata(&file).map_err(|err| format!("blocklist `{path}`: {err}"))?.len();
    if size > MAX {
        return Err(format!("blocklist `{shown}` is {size} bytes: {MAX} is the most a list may be"));
    }
    let text = std::fs::read_to_string(&file).map_err(|err| format!("blocklist `{shown}`: {err}"))?;
    let rules = check(&text, &shown)?;
    Ok(List { identifier: identifier(&rules), rules })
}

/// `path` as the file it names: an absolute path as it is, else inside the plugin's folder.
fn resolve(path: &str, dir: Option<&Path>) -> PathBuf {
    let given = Path::new(path);
    match (given.is_absolute(), dir) {
        (true, _) | (false, None) => given.to_path_buf(),
        (false, Some(dir)) => dir.join(given),
    }
}

/// The list `text` holds, as saved JSON, or what in it is not a rule: a rule without its `trigger`
/// and `action`, an action WebKit does not know, a `block` without a `url-filter`, a
/// `css-display-none` without a `selector`. A list is refused whole, never half applied.
fn check(text: &str, shown: &str) -> Result<String, String> {
    let parsed: serde_json::Value =
        serde_json::from_str(text).map_err(|err| format!("blocklist `{shown}`: not JSON: {err}"))?;
    let rules = parsed.as_array().ok_or_else(|| format!("blocklist `{shown}`: a list is an array of rules"))?;
    for (at, rule) in rules.iter().enumerate() {
        let at = at + 1;
        let (Some(trigger), Some(action)) = (rule.get("trigger"), rule.get("action")) else {
            return Err(format!("blocklist `{shown}`: rule {at} needs a `trigger` and an `action`"));
        };
        let kind = action.get("type").and_then(|kind| kind.as_str()).unwrap_or_default();
        let Some((_, needs_filter, needs_selector)) = ACTIONS.iter().find(|(name, ..)| *name == kind) else {
            return Err(format!(
                "blocklist `{shown}`: rule {at}: `{kind}` is not an action WebKit knows ({})",
                names()
            ));
        };
        if *needs_filter && trigger.get("url-filter").and_then(|filter| filter.as_str()).is_none() {
            return Err(format!("blocklist `{shown}`: rule {at}: `{kind}` needs `trigger.url-filter`"));
        }
        // WebKit's engine has no disjunctions: one `|` refuses the whole list, at compile time and
        // with nothing but a console line. It is refused here instead, where the rule is named.
        if let Some(filter) = trigger.get("url-filter").and_then(|filter| filter.as_str())
            && filter.contains('|')
        {
            return Err(format!(
                "blocklist `{shown}`: rule {at}: `url-filter` cannot use `|`: WebKit has no disjunctions (write a rule for each)"
            ));
        }
        if *needs_selector && action.get("selector").and_then(|selector| selector.as_str()).is_none() {
            return Err(format!("blocklist `{shown}`: rule {at}: `css-display-none` needs `action.selector`"));
        }
    }
    serde_json::to_string(rules).map_err(|err| format!("blocklist `{shown}`: {err}"))
}

/// The actions, as a list for a message.
fn names() -> String {
    ACTIONS.iter().map(|(name, ..)| *name).collect::<Vec<_>>().join(", ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    const GOOD: &str = r#"[
        { "trigger": { "url-filter": "youtube\\.com/pagead/" }, "action": { "type": "block" } },
        { "trigger": { "url-filter": "^https?://www\\.youtube\\.com/" },
          "action": { "type": "css-display-none", "selector": ".ytp-ad-module" } }
    ]"#;

    #[test]
    fn a_good_list_is_a_list() {
        let rules = check(GOOD, "t.json").unwrap();
        assert!(rules.contains("css-display-none"), "{rules}");
        assert_eq!(serde_json::from_str::<serde_json::Value>(&rules).unwrap().as_array().unwrap().len(), 2);
    }

    #[test]
    fn the_same_rules_compile_under_the_same_name() {
        let one = check(GOOD, "a.json").unwrap();
        let two = check(GOOD, "b.json").unwrap();
        assert_eq!(identifier(&one), identifier(&two));
        assert!(identifier(&one).starts_with("content-"));
        assert_eq!(identifier(&one).len(), "content-".len() + 12);
        assert_ne!(identifier(&one), identifier(&one.replace("pagead", "pagead2")));
    }

    #[test]
    fn a_rule_without_its_parts_is_refused() {
        let missing = r#"[{ "trigger": { "url-filter": "a" } }]"#;
        assert!(check(missing, "t").unwrap_err().contains("needs a `trigger` and an `action`"));
        let unknown = r#"[{ "trigger": {}, "action": { "type": "hide-everything" } }]"#;
        assert!(check(unknown, "t").unwrap_err().contains("not an action WebKit knows"));
        let no_filter = r#"[{ "trigger": {}, "action": { "type": "block" } }]"#;
        assert!(check(no_filter, "t").unwrap_err().contains("needs `trigger.url-filter`"));
        let no_selector = r#"[{ "trigger": {}, "action": { "type": "css-display-none" } }]"#;
        assert!(check(no_selector, "t").unwrap_err().contains("needs `action.selector`"));
    }

    #[test]
    fn a_disjunction_is_refused_by_name() {
        let either = r#"[{ "trigger": { "url-filter": "^(www|m)\\.youtube\\.com/" }, "action": { "type": "block" } }]"#;
        let err = check(either, "t").unwrap_err();
        assert!(err.contains("no disjunctions"), "{err}");
        assert!(err.contains("rule 1"), "{err}");
    }

    #[test]
    fn what_is_not_a_list_is_refused() {
        assert!(check("{}", "t").unwrap_err().contains("an array of rules"));
        assert!(check("not json", "t").unwrap_err().contains("not JSON"));
    }

    #[test]
    fn a_list_is_read_from_the_plugin_it_names_or_from_an_absolute_path() {
        let dir = std::env::temp_dir().join(format!("pumi-blocklist-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("youtube.json"), GOOD).unwrap();
        let lists = read(&["youtube.json".into()], Some(&dir)).unwrap();
        assert_eq!(lists.len(), 1);
        assert!(lists[0].rules.contains("ytp-ad-module"));
        let absolute = dir.join("youtube.json").display().to_string();
        assert_eq!(read(&[absolute], None).unwrap().len(), 1);
        assert!(read(&["missing.json".into()], Some(&dir)).unwrap_err().contains("missing.json"));
        fs::remove_dir_all(&dir).unwrap();
    }
}
