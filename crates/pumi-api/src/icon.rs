//! An icon a tab or a block names: what its `@icon` option holds, and a block's `icon` in a
//! manifest. One of the app's icons by name (`terminal`), an image inline (`data:image/svg+xml,…`),
//! or a file in the plugin (`plugin:icons/board.svg`). In `@icon` it may also be
//! `{"src": …, "tint": false}`: drawn in its own colours, not the theme's.
use serde_json::Value;

/// The most an icon's source may be, in bytes: options are kept and sent to every window.
pub const MAX_ICON: usize = 16 * 1024;

/// The image types an icon may be.
const IMAGE_TYPES: &[&str] = &["svg+xml", "png", "jpeg", "webp", "gif"];
const IMAGE_EXTS: &[&str] = &["svg", "png", "jpg", "jpeg", "webp", "gif"];

/// Why `src` is not an icon's source, if it is not one.
pub fn check_icon_src(src: &str) -> Result<(), String> {
    if src.len() > MAX_ICON {
        return Err(format!("an icon is at most {MAX_ICON} bytes, not {}", src.len()));
    }
    if let Some(rest) = src.strip_prefix("data:") {
        let kind = rest.split([';', ',']).next().unwrap_or_default().to_ascii_lowercase();
        let ok = kind.strip_prefix("image/").is_some_and(|t| IMAGE_TYPES.contains(&t)) && rest.contains(',');
        return if ok { Ok(()) } else { Err("a `data:` icon is an SVG, PNG, JPEG, WebP or GIF image".into()) };
    }
    if let Some(path) = src.strip_prefix("plugin:") {
        let ext = path.rsplit_once('.').map(|(_, e)| e.to_ascii_lowercase()).unwrap_or_default();
        let inside = !path.is_empty()
            && !path.starts_with('/')
            && !path.contains('\\')
            && !path.contains(':')
            && path.split('/').all(|part| !part.is_empty() && part != "." && part != "..");
        return if inside && IMAGE_EXTS.contains(&ext.as_str()) {
            Ok(())
        } else {
            Err(format!("`{src}` is not an SVG, PNG, JPEG, WebP or GIF file in the plugin (`plugin:icons/x.svg`)"))
        };
    }
    let name = !src.is_empty()
        && src.len() <= 64
        && src.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_');
    if name {
        Ok(())
    } else {
        Err(format!("`{src}` is an icon's name (`terminal`), a `data:image/…` URI or `plugin:<path>`"))
    }
}

/// Why `value` is not what `@icon` holds, if it is not: a source, `{"src", "tint"}`, or null.
pub fn check_icon(value: &Value) -> Result<(), String> {
    match value {
        Value::Null => Ok(()),
        Value::String(src) => check_icon_src(src),
        Value::Object(fields) => {
            if let Some(key) = fields.keys().find(|k| !matches!(k.as_str(), "src" | "tint")) {
                return Err(format!("an icon has a `src` and a `tint`, not `{key}`"));
            }
            if fields.get("tint").is_some_and(|t| !t.is_boolean()) {
                return Err("an icon's `tint` is true or false".into());
            }
            match fields.get("src") {
                Some(Value::String(src)) => check_icon_src(src),
                _ => Err("an icon's `src` is a string".into()),
            }
        }
        other => Err(format!("an icon is a string or `{{\"src\", \"tint\"}}`, not {other}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn an_icon_is_a_name_an_image_or_a_plugin_file() {
        for ok in [
            "terminal",
            "git_branch",
            "agent_claude",
            "data:image/svg+xml,<svg/>",
            "data:image/png;base64,AAAA",
            "plugin:icons/board.svg",
        ] {
            assert_eq!(check_icon_src(ok), Ok(()), "{ok}");
        }
        for bad in [
            "",
            "Terminal",
            "https://example.com/favicon.ico",
            "file:///etc/x.svg",
            "data:text/html,<b>",
            "data:image/png",
            "plugin:",
            "plugin:/abs.svg",
            "plugin:../up.svg",
            "plugin:a/../../b.svg",
            "plugin:icon.txt",
            "plugin:C:\\x.svg",
        ] {
            assert!(check_icon_src(bad).is_err(), "{bad}");
        }
        assert!(
            check_icon_src(&format!("data:image/svg+xml,{}", "x".repeat(MAX_ICON))).unwrap_err().contains("at most")
        );
    }

    #[test]
    fn an_icon_may_say_it_keeps_its_colours() {
        assert_eq!(check_icon(&Value::Null), Ok(()));
        assert_eq!(check_icon(&json!({ "src": "data:image/svg+xml,<svg/>", "tint": false })), Ok(()));
        assert_eq!(check_icon(&json!({ "src": "terminal" })), Ok(()));
        assert!(check_icon(&json!({ "src": "terminal", "size": 3 })).is_err());
        assert!(check_icon(&json!({ "src": "terminal", "tint": "no" })).is_err());
        assert!(check_icon(&json!({ "tint": true })).is_err());
        assert!(check_icon(&json!(3)).is_err());
    }
}
