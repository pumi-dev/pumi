//! The menu bar: the application menu here, the rest from the app's commands. Only macOS shows
//! one; elsewhere GPUI keeps the bar and its bindings unseen.
//!
//! GPUI installs a bar of its own (`pumi_gpuix::app_menu`: the app menu and the Window menu), but
//! it does not know the app's commands — those live in JS (`runtime/app/app/commands.ts`), with
//! their labels and their chords. `engine.setMenus(json)` hands them over in the shape
//! `[{ name, items: [{ command, title, chord }, { separator: true }, { name, items: […] }] }]`,
//! and a click posts a `menu` event to JS (`on("menu", …)`), which runs the command in the window
//! in front.
//!
//! There is deliberately no Edit menu: a menu key equivalent is consumed by AppKit before the
//! window ever sees the keystroke, and the renderer's text elements do their own clipboard
//! handling, so an Edit menu would take ⌘C and ⌘V away from them.
use gpui::{App, KeyBinding, Menu, MenuItem, SharedString, SystemMenuType};
use napi::bindgen_prelude::*;
use serde_json::Value;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};

#[cfg(target_os = "macos")]
#[path = "macos.rs"]
mod os;
#[cfg(not(target_os = "macos"))]
#[path = "other.rs"]
mod os;

pub use os::about;

/// An item that runs one of the app's commands: JS decides what the command does.
#[derive(Clone, PartialEq, Debug, gpui::Action)]
#[action(namespace = pumi, no_json)]
pub struct RunCommand(pub SharedString);

gpui::actions!(
    pumi,
    [
        /// Show the About panel.
        AboutPumi,
    ]
);

/// The bar's key context. GPUI reads a menu item's key equivalent out of a binding for its
/// action; nothing is ever focused in this context, so these bindings only label the menus —
/// a key AppKit does not take as an equivalent still reaches the window, and JS runs it.
const MENU_CONTEXT: &str = "PumiMenuBar";

static BAR: Mutex<Option<Value>> = Mutex::new(None);
/// The chords the web page with the keyboard keeps (`<WebView keepKeys>`), as `pumi_gpuix::chord`
/// spells them: no item has them as its key equivalent meanwhile, or AppKit would take the key
/// before the page sees it (Monaco's `cmd+d` would split the tab).
static KEPT: Mutex<Vec<String>> = Mutex::new(Vec::new());
static LISTENED: AtomicBool = AtomicBool::new(false);

/// Remembers the bar JS sent, and puts it up when the app is already running.
pub fn set(json: &str) -> Result<()> {
    let bar: Value = serde_json::from_str(json).map_err(|err| Error::from_reason(format!("bad menus: {err}")))?;
    *BAR.lock().unwrap() = Some(bar);
    if pumi_gpuix::app::running() {
        pumi_gpuix::app::update(install)?;
    }
    Ok(())
}

/// A page took the keyboard, keeping `chords`, or gave it back (none): the bar again without them.
pub fn keep(chords: &[String], cx: &mut App) {
    {
        let mut kept = KEPT.lock().unwrap();
        if kept.as_slice() == chords {
            return;
        }
        *kept = chords.to_vec();
    }
    install(cx);
}

/// Puts the bar up: the application menu, then what JS last sent.
pub fn install(cx: &mut App) {
    if !LISTENED.swap(true, Ordering::Relaxed) {
        cx.on_action(|run: &RunCommand, _cx: &mut App| {
            crate::engine::emit("menu", serde_json::json!({ "command": run.0.as_ref() }));
        });
        cx.on_action(|_: &AboutPumi, cx: &mut App| about(cx));
    }
    let Some(bar) = BAR.lock().unwrap().clone() else { return };
    let mut bindings = Vec::new();
    let mut menus = vec![app_menu(&mut bindings)];
    menus.extend(menus_of(&bar, &mut bindings));
    // The bar's own bindings replace the ones it had: an item's key equivalent is the first binding
    // of its action, so one left from before would outlive a change of keys (or a kept chord).
    let others: Vec<KeyBinding> = cx
        .key_bindings()
        .borrow()
        .bindings()
        .filter(|binding| binding.action().as_any().downcast_ref::<RunCommand>().is_none())
        .cloned()
        .collect();
    cx.clear_key_bindings();
    cx.bind_keys(others.into_iter().chain(bindings));
    cx.set_menus(menus);
}

/// The application menu: About, Settings, the keybindings, the services, hiding and quitting.
/// Its actions are the renderer's own (`pumi_gpuix::app_menu`), which already has handlers.
fn app_menu(bindings: &mut Vec<KeyBinding>) -> Menu {
    use pumi_gpuix::app_menu::{Hide, HideOthers, Quit, ShowAll};
    let name = pumi_gpuix::app::app_name();
    let items = vec![
        MenuItem::action(format!("About {name}"), AboutPumi),
        MenuItem::separator(),
        command("settings", "Settings…", Some("cmd+,"), bindings),
        command("help", "Keybindings…", Some("cmd+/"), bindings),
        MenuItem::separator(),
        MenuItem::os_submenu("Services", SystemMenuType::Services),
        MenuItem::separator(),
        MenuItem::action(format!("Hide {name}"), Hide),
        MenuItem::action("Hide Others", HideOthers),
        MenuItem::action("Show All", ShowAll),
        MenuItem::separator(),
        MenuItem::action(format!("Quit {name}"), Quit),
    ];
    Menu { name: name.into(), items: tidy(items), disabled: false }
}

fn menus_of(bar: &Value, bindings: &mut Vec<KeyBinding>) -> Vec<Menu> {
    bar.as_array().into_iter().flatten().filter_map(|menu| submenu(menu, bindings)).collect()
}

fn submenu(value: &Value, bindings: &mut Vec<KeyBinding>) -> Option<Menu> {
    let name = value["name"].as_str()?;
    let items = value["items"].as_array().into_iter().flatten().filter_map(|item| item_of(item, bindings)).collect();
    let items = tidy(items);
    if items.is_empty() {
        return None;
    }
    Some(Menu { name: name.into(), items, disabled: false })
}

fn item_of(value: &Value, bindings: &mut Vec<KeyBinding>) -> Option<MenuItem> {
    if value["separator"].as_bool() == Some(true) {
        return Some(MenuItem::separator());
    }
    if value.get("items").is_some() {
        return submenu(value, bindings).map(MenuItem::submenu);
    }
    let name = value["command"].as_str()?;
    let title = value["title"].as_str().unwrap_or(name);
    Some(command(name, title, value["chord"].as_str(), bindings))
}

fn command(name: &str, title: &str, chord: Option<&str>, bindings: &mut Vec<KeyBinding>) -> MenuItem {
    let action = RunCommand(name.into());
    // A binding is only here to give the item its key equivalent, but `KeyBinding::new` panics on
    // a chord GPUI cannot parse (a keymap's own spelling of one, say), so check.
    if let Some(spelled) = chord.and_then(spell)
        && !kept(&spelled)
    {
        bindings.push(KeyBinding::new(&spelled, action.clone(), Some(MENU_CONTEXT)));
    }
    MenuItem::action(title, action)
}

/// Whether a chord in GPUI's spelling is one the focused page keeps.
fn kept(spelled: &str) -> bool {
    is_kept(&KEPT.lock().unwrap(), spelled)
}

/// Whether `spelled` (GPUI's spelling) is one of `kept` (the page's: `alt+cmd+up`, `cmd+enter`), as
/// one keystroke: both spelled the way GPUI reads them, and compared by modifiers and key.
fn is_kept(kept: &[String], spelled: &str) -> bool {
    let Ok(stroke) = gpui::Keystroke::parse(spelled) else { return false };
    kept.iter()
        .filter_map(|chord| spell(chord).and_then(|s| gpui::Keystroke::parse(&s).ok()))
        .any(|k| k.modifiers == stroke.modifiers && k.key == stroke.key)
}

/// A chord in the app's spelling (`cmd+shift+n`) in GPUI's (`cmd-shift-n`).
///
/// The app's keymap joins with `+` and names its punctuation (`minus`, `slash`); GPUI splits on `-`
/// and reads a key by its own name. `-` is spelled `cmd--`, which GPUI reads as the minus key.
fn spell(chord: &str) -> Option<String> {
    let mut parts: Vec<&str> = chord.split('+').collect();
    let key = match parts.pop()?.trim() {
        "minus" => "-",
        "equal" => "=",
        "plus" => "+",
        "slash" => "/",
        "enter" | "return" => "\r",
        "tab" => "\t",
        other => other,
    };
    let mut spelled = parts.join("-");
    if !spelled.is_empty() {
        spelled.push('-');
    }
    spelled.push_str(key);
    gpui::Keystroke::parse(&spelled).ok().map(|_| spelled)
}

/// No separator first, last, or beside another.
fn tidy(items: Vec<MenuItem>) -> Vec<MenuItem> {
    let mut out: Vec<MenuItem> = Vec::with_capacity(items.len());
    for item in items {
        if matches!(item, MenuItem::Separator) && out.last().is_none_or(|last| matches!(last, MenuItem::Separator)) {
            continue;
        }
        out.push(item);
    }
    if out.last().is_some_and(|last| matches!(last, MenuItem::Separator)) {
        out.pop();
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn names(items: &[MenuItem]) -> Vec<String> {
        items
            .iter()
            .map(|item| match item {
                MenuItem::Separator => "-".to_string(),
                MenuItem::Submenu(menu) => menu.name.to_string(),
                MenuItem::SystemMenu(menu) => menu.name.to_string(),
                MenuItem::Action { name, .. } => name.to_string(),
            })
            .collect()
    }

    fn chord(bindings: &[KeyBinding], command: &str) -> Option<String> {
        let wanted = RunCommand(command.into());
        bindings
            .iter()
            .find(|binding| binding.action().partial_eq(&wanted))
            .map(|binding| binding.keystrokes().iter().map(|k| k.inner().unparse()).collect::<Vec<_>>().join(" "))
    }

    #[test]
    fn the_app_menu_has_about_settings_and_quit() {
        let mut bindings = Vec::new();
        let menu = app_menu(&mut bindings);
        assert_eq!(names(&menu.items).first().map(String::as_str), Some("About pumi"));
        assert!(names(&menu.items).iter().any(|item| item == "Settings…"));
        assert!(names(&menu.items).iter().any(|item| item == "Quit pumi"));
        assert_eq!(chord(&bindings, "settings").as_deref(), Some("cmd-,"));
        assert_eq!(spell("cmd+/").as_deref(), Some("cmd-/"));
    }

    #[test]
    fn a_command_keeps_the_chord_it_came_with() {
        let mut bindings = Vec::new();
        let menu = submenu(&json!({ "name": "Pane", "items": [{ "command": "split_vertical", "title": "Split Right", "chord": "cmd+d" }] }), &mut bindings).unwrap();
        assert_eq!(names(&menu.items), ["Split Right"]);
        assert_eq!(chord(&bindings, "split_vertical").as_deref(), Some("cmd-d"));
    }

    #[test]
    fn a_chord_the_focused_page_keeps_is_no_items_key_equivalent() {
        let kept = ["cmd+d", "alt+cmd+up", "cmd+enter", "cmd+slash", "cmd+["].map(String::from);
        for chord in ["cmd+d", "cmd+alt+up", "cmd+enter", "cmd+/", "cmd+["] {
            assert!(is_kept(&kept, &spell(chord).unwrap()), "{chord}");
        }
        for chord in ["cmd+shift+d", "cmd+alt+down", "cmd+t", "shift+enter"] {
            assert!(!is_kept(&kept, &spell(chord).unwrap()), "{chord}");
        }
        assert!(!is_kept(&[], "cmd-d"));
    }

    #[test]
    fn the_apps_chords_are_spelled_the_way_gpui_reads_them() {
        assert_eq!(spell("cmd+t").as_deref(), Some("cmd-t"));
        assert_eq!(spell("cmd+shift+n").as_deref(), Some("cmd-shift-n"));
        assert_eq!(spell("cmd+alt+left").as_deref(), Some("cmd-alt-left"));
        assert_eq!(spell("cmd+,").as_deref(), Some("cmd-,"));
        assert_eq!(spell("cmd+slash").as_deref(), Some("cmd-/"));
        assert_eq!(spell("cmd+minus").as_deref(), Some("cmd--"));
        assert_eq!(spell("cmd+enter").as_deref(), Some("cmd-\r"));
        assert_eq!(spell("shift+escape").as_deref(), Some("shift-escape"));
        // A sequence is not one keystroke: no key equivalent, and no panic either.
        assert_eq!(spell("cmd+k cmd+c"), None);
    }

    #[test]
    fn a_spelled_chord_keeps_its_modifiers_and_its_key() {
        let stroke = gpui::Keystroke::parse(&spell("cmd+shift+n").unwrap()).unwrap();
        assert_eq!(stroke.key, "n");
        assert!(stroke.modifiers.platform && stroke.modifiers.shift && !stroke.modifiers.alt);
    }

    #[test]
    fn an_item_without_a_command_or_children_is_dropped() {
        let mut bindings = Vec::new();
        let bar = json!([{ "name": "Empty", "items": [] }, { "name": "File", "items": [{ "title": "nothing" }] }]);
        assert!(menus_of(&bar, &mut bindings).is_empty());
    }

    #[test]
    fn separators_never_lead_trail_or_double() {
        let mut bindings = Vec::new();
        let menu = submenu(
            &json!({ "name": "View", "items": [
                { "separator": true },
                { "command": "a", "title": "A" },
                { "separator": true },
                { "separator": true },
                { "command": "b", "title": "B" },
                { "separator": true }
            ]}),
            &mut bindings,
        )
        .unwrap();
        assert_eq!(names(&menu.items), ["A", "-", "B"]);
    }
}
