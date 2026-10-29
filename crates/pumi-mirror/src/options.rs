use pumi_api::Result;
pub use pumi_model::options::{Change, Decl, Key, Options, Scope, Set, Type, check_type, reaches, valid_name};
use serde_json::{Value, json};
pub fn normalise(name: &str) -> String {
    if name.starts_with('@') { name.to_string() } else { name.replace('-', "_") }
}
pub fn coerce(options: &Options, name: &str, value: Value) -> Result<Value> {
    match options.decl(&normalise(name)) {
        Some(decl) if !value.is_null() => coerce_to(decl, value),
        _ => Ok(value),
    }
}
fn coerce_to(decl: &Decl, value: Value) -> Result<Value> {
    let value = match (decl.ty, &value) {
        (Type::Boolean, Value::String(s)) => match s.as_str() {
            "on" | "true" | "yes" | "1" => Value::Bool(true),
            "off" | "false" | "no" | "0" => Value::Bool(false),
            _ => value,
        },
        (Type::Number, Value::String(s)) => s.trim().parse::<f64>().ok().and_then(number).unwrap_or(value),
        _ => value,
    };
    check_type(decl, &value)?;
    if let Some(mut n) = value.as_f64().filter(|_| decl.ty == Type::Number) {
        if let Some(step) = decl.step.filter(|s| *s > 0.0) {
            let base = decl.min.unwrap_or(0.0);
            n = base + ((n - base) / step).round() * step;
        }
        if let Some(min) = decl.min {
            n = n.max(min);
        }
        if let Some(max) = decl.max {
            n = n.min(max);
        }
        return Ok(number((n * 1e9).round() / 1e9).unwrap_or(value));
    }
    Ok(value)
}
fn number(n: f64) -> Option<Value> {
    if n.fract() == 0.0 && n.abs() < 1e15 {
        Some(json!(n as i64))
    } else {
        serde_json::Number::from_f64(n).map(Value::Number)
    }
}
/// The Settings group of pumi's own theme's settings (`ui_font_size`, …).
pub const DEFAULT_THEME_GROUP: &str = "Default theme";
/// Its sections under the theme picker: `<DEFAULT_THEME_GROUP> · <title>`.
const DEFAULT_THEME_PANES: &str = "Default theme · Panes";
const DEFAULT_THEME_SIDEBAR: &str = "Default theme · Side bar";
const DEFAULT_THEME_MOTION: &str = "Default theme · Motion and feedback";
pub fn builtin() -> Vec<Decl> {
    use Scope::*;
    use Type::*;
    let mut decls = vec![
        Decl::new(
            "default_size",
            String,
            json!("80x24"),
            Global,
            "The size of a view no client shows, as WIDTHxHEIGHT.",
        ),
        Decl::new(
            "default_shell",
            String,
            json!(""),
            Global,
            "The program a terminal runs when none is given; empty is $SHELL, as a login shell.",
        ),
        Decl::new(
            "pane_tabs",
            Boolean,
            json!(true),
            Global,
            "A pane holds several tabs, with a strip of them on top; off, every tab is a pane of its own, as in tmux, and a tab put into a pane splits it.",
        )
        .label("Tabs in panes")
        .group(DEFAULT_THEME_PANES),
        Decl::new("base_index", Number, json!(0), Session, "The index of a session's first view."),
        Decl::new(
            "worktree",
            String,
            json!(""),
            View,
            "The git worktree a view works in, as a folder: where a terminal opened in the view without a `cwd` starts. Empty is its session's folder.",
        ),
        Decl::new(
            "worktree_dir",
            String,
            json!("~/.pumi/worktrees"),
            Global,
            "Where a new worktree is made: `<worktree_dir>/<repository>/<name>`. `~` is the home folder.",
        ),
        Decl::new(
            "history_limit",
            Number,
            json!(2000),
            Tab,
            "Lines of a terminal's scrollback kept in its saved state.",
        ),
        Decl::new("remain_on_exit", Boolean, json!(false), Tab, "Keep a terminal tab, dead, when its program exits."),
        Decl::new(
            "automatic_rename",
            Boolean,
            json!(true),
            View,
            "A view without a name shows its active tab's title.",
        ),
        Decl::new(
            "exit_empty",
            Boolean,
            json!(true),
            Global,
            "The server exits a while after its last session closed and no one is connected.",
        ),
        Decl::new(
            "viewoptions",
            String,
            json!("scrollback,history"),
            Global,
            "What a tab's saved state includes beyond its options and type data: `scrollback` (terminals), `history` (browsers), `cookies`.",
        ),
        Decl::new(
            "sessionoptions",
            String,
            json!("sessions,views,tabs,floats,options"),
            Global,
            "What `mksession` writes.",
        ),
        Decl::new(
            "renderers",
            Table,
            json!({}),
            Global,
            "Which plugin draws each core block, by block: `{ terminal = \"me/term\" }`. Without one, the bundled plugin draws it (`pumi/terminal`); a tab's `@tab_renderer` picks one for that tab.",
        ),
        Decl::new(
            "title",
            String,
            json!(""),
            Tab,
            "The tab's title: what its program said (OSC 2), its page's title, or what `select-tab -T` set.",
        ),
        Decl::new("font_size", Number, Value::Null, Tab, "The tab's font size in points; empty takes the client's."),
        Decl::new(
            "runtimepath",
            String,
            json!(""),
            Global,
            "Where plugins' files are found, comma-separated: the config directory, the plugins' site, pumi's runtime, then each enabled plugin. The host keeps it as plugins come and go.",
        ),
        Decl::new(
            "pluginindex",
            String,
            json!("https://api.pumi.dev/v1/index.json"),
            Global,
            "The store's index: a URL or a file of the plugins `pumi registry list plugin --available` finds and `pumi plugin add OWNER/NAME` installs. Installs from it are counted (once per address a day) at the same server.",
        ),
        Decl::new("buffer_limit", Number, json!(50), Global, "The most paste buffers kept; past it the oldest goes."),
        Decl::new(
            "command_alias",
            Table,
            json!(["server-info=show-messages -JT", "info=show-messages -JT"]),
            Global,
            "Command aliases, each `alias=command line`.",
        ),
        Decl::new("prefix", String, json!("C-b"), Session, "The prefix key: it enters the `prefix` key table."),
        Decl::new("prefix2", String, json!("None"), Session, "A second prefix key."),
        Decl::new("status_interval", Number, json!(15), Session, "Seconds between status line redraws."),
        Decl::new(
            "status_left",
            String,
            json!("[#{session_name}] "),
            Session,
            "The left of the default status line, a format.",
        ),
        Decl::new(
            "status_right",
            String,
            json!("\"#{=21:tab_title}\" %H:%M %d-%b-%y"),
            Session,
            "The right of the default status line, a format.",
        ),
        Decl::new(
            "main_pane_width",
            String,
            json!("80"),
            View,
            "The main pane's width in the `main-vertical` layout, in cells or a percentage (`60%`).",
        ),
        Decl::new("synchronize_tabs", Boolean, json!(false), Tab, "Typing goes to every tab of the view at once."),
        Decl::new(
            "tab_block",
            String,
            json!(""),
            Tab,
            "The block it holds: `terminal`, or a plugin's (`owner.name.id`)",
        )
        .readonly(),
        Decl::new("pane_width", Number, json!(0), Tab, "Its width in cells, as whoever shows it sized it.").readonly(),
        Decl::new("pane_height", Number, json!(0), Tab, "Its height in cells, as whoever shows it sized it.")
            .readonly(),
        Decl::new(
            "tab_current_path",
            String,
            json!(""),
            Tab,
            "Its directory: a terminal's program's (OSC 7, else the process's).",
        )
        .readonly(),
        Decl::new("tab_pid", Number, Value::Null, Tab, "A terminal's first process.").readonly(),
        Decl::new("tab_current_command", String, json!(""), Tab, "A terminal's foreground program.").readonly(),
        Decl::new(
            "tab_foreground_pgid",
            Number,
            Value::Null,
            Tab,
            "A terminal's foreground process group: what its program status reports belong to.",
        )
        .readonly(),
        Decl::new("tab_dead", Boolean, json!(false), Tab, "A terminal's program has exited.").readonly(),
        Decl::new("tab_dead_status", Number, Value::Null, Tab, "How it exited.").readonly(),
        Decl::new("client_width", Number, json!(0), Client, "The client's size in cells.").readonly(),
        Decl::new("client_height", Number, json!(0), Client, "The client's size in cells.").readonly(),
        Decl::new("key_table", String, json!("root"), Client, "The client's mode: the key table its keys go through."),
        Decl::new(
            "client_active",
            Boolean,
            Value::Null,
            Client,
            "The client's app is in front: one of its windows has the focus (`app.changed`); unset until it says.",
        ),
        Decl::new(
            "client_tab",
            Number,
            Value::Null,
            Client,
            "The tab the client's front window focuses: what its calls' current tab is, and `tab_focused`.",
        ),
        Decl::new(
            "client_visible",
            Table,
            Value::Null,
            Client,
            "The tabs the client draws, a list of ids: `tab_visible`.",
        ),
        Decl::new(
            "statusline",
            String,
            json!("#{T:status_left}#[align=right]#{T:status_right}"),
            Session,
            "The status bar, a format.",
        ),
        Decl::new(
            "tabline",
            String,
            json!(
                "#{W:#[range=window|#{window_index}]#{?window_active,#[fg=TabLineSel],#[fg=TabLine]} #{window_index}:#{window_name} #[norange default]}"
            ),
            Session,
            "The view bar, a format; each view a `range=window|N` so a click selects it.",
        ),
        Decl::new("winbar", String, json!(""), Tab, "A bar on each tab, a format; empty shows none."),
        Decl::new("titlestring", String, json!("#{session_name}"), Session, "The window's title, a format."),
    ];
    decls.extend([
        Decl::new(
            "ui_theme",
            String,
            json!("default"),
            Global,
            "The theme: the whole window — its layout, its parts and its look, for the app and every plugin. pumi's own `default`, or a plugin's: a `theme.ts` in one of the directories its manifest lists under `themes`.",
        )
        .label("Theme")
        .group("Appearance")
        .control("ui_theme"),
        // pumi's own theme's settings: Settings shows them under the theme picker while it is
        // the one picked (`DEFAULT_THEME_GROUP`); a plugin's theme has settings of its own.
        Decl::new(
            "ui_font_size",
            Number,
            json!(13),
            Global,
            "The size of the window's text: views, the side bar, pane titles, menus. Terminals' text is the Font's size.",
        )
        .label("Text size")
        .unit("pt")
        .range(10.0, 16.0, Some(1.0))
        .group(DEFAULT_THEME_GROUP),
        Decl::new(
            "ui_inactive_pane_fade",
            Number,
            json!(25),
            Global,
            "How much a terminal fades while another pane has the focus; 0 keeps every pane as bright as the focused one.",
        )
        .label("Inactive pane fade")
        .unit("%")
        .range(0.0, 80.0, Some(5.0))
        .group(DEFAULT_THEME_GROUP)
        .control("slider"),
        Decl::new(
            "ui_pane_gap",
            Number,
            json!(0),
            Global,
            "Room between a view's panes and round them; 0 butts them together with a line between.",
        )
        .label("Space between panes")
        .unit("px")
        .range(0.0, 16.0, Some(2.0))
        .group(DEFAULT_THEME_GROUP),
        Decl::new(
            "ui_radius",
            Number,
            json!(6),
            Global,
            "How round the corners of menus, dialogs, buttons and rows are; 0 is square.",
        )
        .label("Corner radius")
        .unit("px")
        .range(0.0, 12.0, Some(1.0))
        .group(DEFAULT_THEME_GROUP),
        Decl::new(
            "ui_accent",
            String,
            json!(""),
            Global,
            "The colour of the focus, selections and primary buttons, `#rrggbb`; empty keeps the theme's blue.",
        )
        .label("Accent color")
        .placeholder("#3474f0")
        .group(DEFAULT_THEME_GROUP)
        .control("color"),
        Decl::new(
            "ui_background_opacity",
            Number,
            json!(95),
            Global,
            "How much the window's background hides what is behind it: the side bar, the title bar and the terminals' background. 100 is opaque (no transparency); lower lets the desktop show through.",
        )
        .label("Background opacity")
        .unit("%")
        .range(20.0, 100.0, Some(5.0))
        .group(DEFAULT_THEME_GROUP)
        .control("slider"),
        Decl::new(
            "ui_background_blur",
            Number,
            json!(40),
            Global,
            "How much what shows through a transparent background is blurred; 0 leaves it sharp. Only seen while the background opacity is under 100.",
        )
        .label("Background blur")
        .unit("px")
        .range(0.0, 60.0, Some(2.0))
        .group(DEFAULT_THEME_GROUP)
        .control("slider"),
        // How it lays out and behaves: a plugin's theme that wants the same declares its own
        // (`pane_titles`, … unprefixed in its manifest; the theme kit's `usePaneTitles` reads it).
        Decl::new(
            "ui_pane_titles",
            Boolean,
            json!(true),
            Global,
            "A bar over each pane with its name and buttons, to drag it by.",
        )
        .label("Show pane title bars")
        .group(DEFAULT_THEME_PANES),
        Decl::new(
            "ui_pane_outline",
            Boolean,
            json!(false),
            Global,
            "A line round the pane with the focus while its view is split.",
        )
        .label("Outline the focused pane")
        .group(DEFAULT_THEME_PANES),
        Decl::new(
            "ui_pane_equalize",
            Boolean,
            json!(true),
            Global,
            "Every pane in a view gets an equal share as one opens or closes.",
        )
        .label("Even out splits automatically")
        .group(DEFAULT_THEME_PANES),
        Decl::new(
            "ui_focus_follows_mouse",
            Boolean,
            json!(false),
            Global,
            "A pane takes the focus as the mouse moves onto it, without a click.",
        )
        .label("Focus the pane under the mouse")
        .group(DEFAULT_THEME_PANES),
        Decl::new(
            "ui_sidebar_swipe",
            Boolean,
            json!(true),
            Global,
            "Two fingers swept left over the side bar hide it; ⌘B brings it back.",
        )
        .label("Hide the side bar with a swipe")
        .group(DEFAULT_THEME_SIDEBAR),
        Decl::new(
            "ui_view_icons",
            Number,
            json!(3),
            Global,
            "How many of a view's tabs show as icons beside its name; past that, a count.",
        )
        .label("Tab icons per view")
        .range(0.0, 3.0, Some(1.0))
        .group(DEFAULT_THEME_SIDEBAR)
        .control("slider"),
        Decl::new(
            "animate",
            Number,
            json!(1),
            Global,
            "Panes glide into place as they open, close, move and zoom, and the start screen fades in.",
        )
        .label("Animate layout changes")
        .range(0.0, 1.0, Some(1.0))
        .group(DEFAULT_THEME_MOTION)
        .control("toggle"),
        Decl::new(
            "haptics",
            Boolean,
            json!(true),
            Global,
            "A tap on a Force Touch trackpad as a switch turns or a slider steps. Nothing on a mouse.",
        )
        .label("Haptic feedback")
        .group(DEFAULT_THEME_MOTION),
        Decl::new(
            "terminal_colorscheme",
            String,
            json!(""),
            Global,
            "Give terminals a colour scheme of their own (one of iTerm2-Color-Schemes'), or leave empty to match the theme. pumi's own theme takes its colours from the scheme too, dark or light as the scheme's background is (Ghostty's `theme`).",
        )
        .label("Color scheme")
        .group("Appearance")
        .control("terminal_theme"),
        Decl::new(
            "terminal_palette_generate",
            Boolean,
            json!(false),
            Global,
            "Make the 256-colour palette's colours 16–255 from the terminals' sixteen, background and foreground, so programs that use them match the colour scheme (Ghostty's `palette-generate`). Off keeps xterm's, which older programs assume. Colours a theme pins stay.",
        )
        .label("Generate 256 colors from the scheme")
        .group("Terminal"),
        Decl::new(
            "terminal_palette_harmonious",
            Boolean,
            json!(false),
            Global,
            "On a light background, the generated colours run from the background to the foreground, as on a dark one, instead of from dark to light (Ghostty's `palette-harmonious`).",
        )
        .label("Shade light schemes from the background")
        .group("Terminal"),
        Decl::new(
            "guifont",
            String,
            json!(""),
            Global,
            "The typeface for code: terminals, and any editor that follows it (the SDK's `useFont`; Ghostty's `font-family`).",
        )
        .label("Font")
        .group("Font")
        .control("font"),
        Decl::new(
            "linespace",
            Number,
            json!(0),
            Global,
            "How much taller than the font's own line each line of code is, in percent (Ghostty's `adjust-cell-height`): terminals, and any editor that follows it. 0 is the font's own.",
        )
        .label("Line height")
        .unit("%")
        .range(-50.0, 100.0, Some(5.0))
        .group("Font"),
        Decl::new(
            "terminal_font_fallbacks",
            String,
            json!(""),
            Global,
            "Fonts the terminals use, in order, for characters the font doesn't have; separated by commas (Ghostty's repeated `font-family`).",
        )
        .label("Fallback fonts")
        .placeholder("Symbols Nerd Font Mono, Apple Color Emoji")
        .group("Font"),
        Decl::new(
            "terminal_font_style",
            String,
            json!("regular"),
            Global,
            "The weight of the terminals' regular text; bold text is drawn heavier still (Ghostty's `font-style`).",
        )
        .label("Font weight")
        .labelled(&[
            ("thin", "Thin"),
            ("extralight", "Extra light"),
            ("light", "Light"),
            ("regular", "Regular"),
            ("medium", "Medium"),
            ("semibold", "Semibold"),
            ("bold", "Bold"),
            ("extrabold", "Extra bold"),
            ("black", "Black"),
        ])
        .group("Font"),
        Decl::new(
            "terminal_font_smoothing",
            Boolean,
            json!(false),
            Global,
            "Thicken the terminals' text as macOS's font smoothing does (Ghostty's `font-thicken`). Off draws thinner, crisper glyphs.",
        )
        .label("Thicken text")
        .group("Font"),
        Decl::new(
            "terminal_ligatures",
            Boolean,
            json!(true),
            Global,
            "Join characters the font draws as one, as `->` or `!=` in a coding font. Off draws each as typed (Ghostty's `font-feature = -calt`).",
        )
        .label("Ligatures")
        .group("Font"),
        Decl::new(
            "terminal_minimum_contrast",
            Number,
            json!(1),
            Global,
            "Text whose contrast with what is under it is below this ratio (1 to 21, WCAG's) is lightened or darkened until it reads. 1 leaves every colour as the program gave it (Ghostty's `minimum-contrast`).",
        )
        .label("Minimum text contrast")
        .range(1.0, 21.0, Some(0.5))
        .group("Terminal"),
        Decl::new(
            "terminal_padding_x",
            Number,
            json!(4),
            Global,
            "Space between a terminal's text and its pane's left and right edges (Ghostty's `window-padding-x`).",
        )
        .label("Padding left and right")
        .unit("px")
        .range(0.0, 40.0, Some(1.0))
        .group("Terminal"),
        Decl::new(
            "terminal_padding_y",
            Number,
            json!(2),
            Global,
            "Space between a terminal's text and its pane's top and bottom edges (Ghostty's `window-padding-y`).",
        )
        .label("Padding top and bottom")
        .unit("px")
        .range(0.0, 40.0, Some(1.0))
        .group("Terminal"),
        Decl::new(
            "terminal_padding_color",
            String,
            json!("extend"),
            Global,
            "What fills the padding: the theme's background, or the colour of the cells at the screen's edge, so a program that paints its whole screen is painted to the pane's edges (Ghostty's `window-padding-color`).",
        )
        .label("Padding colour")
        .labelled(&[("extend", "The screen's edge"), ("background", "Theme background")])
        .group("Terminal"),
        Decl::new(
            "terminal_cursor_style",
            String,
            json!("block"),
            Global,
            "The cursor's shape while the program hasn't asked for another (Ghostty's `cursor-style`).",
        )
        .label("Cursor shape")
        .labelled(&[("block", "Block"), ("bar", "Bar"), ("underline", "Underline"), ("block_hollow", "Hollow block")])
        .group("Cursor"),
        Decl::new(
            "terminal_cursor_blink",
            String,
            json!("off"),
            Global,
            "Whether the cursor blinks (Ghostty's `cursor-style-blink`).",
        )
        .label("Blinking cursor")
        .labelled(&[("off", "Off"), ("on", "On")])
        .group("Cursor"),
        Decl::new(
            "terminal_copy_on_select",
            Boolean,
            json!(false),
            Global,
            "Copy text to the clipboard as soon as it is selected (Ghostty's `copy-on-select`).",
        )
        .label("Copy text when selected")
        .group("Mouse"),
        Decl::new(
            "macos_option_as_alt",
            Boolean,
            json!(true),
            Global,
            "Use Option for terminal shortcuts like alt+b instead of typing characters like å (Ghostty's `macos-option-as-alt`).",
        )
        .label("Use Option as Alt")
        .group("Keys"),
        Decl::new(
            "autoupdate",
            Boolean,
            json!(false),
            Global,
            "Check every few hours and download new versions in the background.",
        )
        .label("Update automatically")
        .group("Updates"),
        Decl::new(
            "pluginautocheck",
            Boolean,
            json!(false),
            Global,
            "Check every few hours whether an installed plugin has a new version, and say so.",
        )
        .label("Check for plugin updates automatically")
        .group("Plugins"),
        Decl::new(
            "keymap_preset",
            String,
            json!("default"),
            Global,
            "Start from the shortcuts of an editor you already know, or from none.",
        )
        .label("Base keymap")
        .labelled(&[
            ("default", "pumi"),
            ("vscode", "VS Code"),
            ("jetbrains", "JetBrains"),
            ("sublime_text", "Sublime Text"),
            ("atom", "Atom"),
            ("textmate", "TextMate"),
            ("emacs", "Emacs"),
            ("cursor", "Cursor"),
            ("none", "None"),
        ])
        .group("Keys"),
        Decl::new("keymap_bindings", Table, json!({}), Global, "Your shortcuts that differ from the base keymap.")
            .label("Key bindings")
            .group("Keys")
            .control("keymap"),
        // Every plugin's pages are the app's webview: its inspector is the app's to turn on.
        Decl::new(
            "webview_devtools",
            Boolean,
            json!(true),
            Global,
            "Inspect and debug the web pages plugins show with alt+cmd+i.",
        )
        .label("Web Inspector")
        .group("Plugins"),
    ]);
    decls.extend(
        [
            ("terminal_background", "The terminals' background"),
            ("terminal_foreground", "The terminals' text"),
            ("terminal_cursor_color", "The terminals' cursor"),
            ("terminal_selection_background", "The terminals' selection"),
        ]
        .map(|(name, what)| {
            Decl::new(
                name,
                String,
                json!(""),
                Global,
                &format!("{what} colour, `#rrggbb`; empty keeps the colour scheme's."),
            )
        }),
    );
    decls.extend((0..16).map(|i| {
        Decl::new(
            &format!("terminal_color_{i}"),
            String,
            json!(""),
            Global,
            "An ANSI colour of the terminals, `#rrggbb`; empty keeps the colour scheme's.",
        )
    }));
    decls
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn tmux_spellings_and_values_reach_the_option() {
        let options = Options::new(builtin());
        assert_eq!(normalise("remain-on-exit"), "remain_on_exit");
        assert_eq!(normalise("@my-var"), "@my-var", "a user option is taken as written");
        assert_eq!(coerce(&options, "remain-on-exit", json!("on")).unwrap(), json!(true));
        assert_eq!(coerce(&options, "history_limit", json!("500")).unwrap(), json!(500));
        assert!(coerce(&options, "history_limit", json!("lots")).is_err());
        assert_eq!(coerce(&options, "linespace", json!(13)).unwrap(), json!(15), "snapped to the step");
        assert_eq!(coerce(&options, "linespace", json!(900)).unwrap(), json!(100), "clamped to the range");
        assert_eq!(coerce(&options, "@x", json!("on")).unwrap(), json!("on"), "nobody declared it");
        assert_eq!(coerce(&options, "linespace", Value::Null).unwrap(), Value::Null, "an unset");
    }
}
