pub use pumi_model::options::{Change, Decl, Key, Options, Scope, Set, Type, check_type, reaches, valid_name};
use serde_json::{Value, json};
pub fn builtin() -> Vec<Decl> {
    use Scope::*;
    use Type::*;
    vec![
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
            "own_permissions",
            Boolean,
            json!(true),
            Global,
            "macOS: a new terminal's program asks for privacy permissions (files, screen recording, …) as itself, so the prompt names it, not pumi; off, pumi asks and what pumi was granted covers every program it runs.",
        ),
        Decl::new("base_index", Number, json!(0), Session, "The index of a session's first view."),
        Decl::new(
            "worktree",
            String,
            json!(""),
            View,
            "The git worktree a view works in, as a folder: where a terminal opened in the view without a `cwd` starts. Empty is its session's folder.",
        ),
        Decl::new(
            "history_bytes",
            Number,
            json!(4 << 20),
            Tab,
            "Bytes of a terminal's output the core keeps: what a client attaching is sent, and what a restart restores.",
        ),
        Decl::new("remain_on_exit", Boolean, json!(false), Tab, "Keep a terminal tab, dead, when its program exits."),
        Decl::new(
            "restore_command",
            String,
            json!(""),
            Tab,
            "A line a restored terminal types into its new shell, as the program running in it set it (an agent's resume command). Dropped when the shell is in the foreground again.",
        ),
        Decl::new(
            "environment",
            Table,
            json!({}),
            Session,
            "Variables a new terminal gets, by name; a `false` value removes one.",
        ),
        Decl::new(
            "exit_empty",
            Boolean,
            json!(true),
            Global,
            "The core exits a while after its last session closed and no one is connected.",
        ),
        Decl::new(
            "viewoptions",
            String,
            json!("scrollback,history"),
            Global,
            "What a tab's saved state includes beyond its options and type data: `scrollback` (terminals), `history` (browsers), `cookies`.",
        ),
        Decl::new(
            "title",
            String,
            json!(""),
            Tab,
            "The tab's title: what its program said (OSC 2), its page's title, or what `select-tab -T` set.",
        ),
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
        Decl::new("tab_tty", String, json!(""), Tab, "A terminal's tty.").readonly(),
        Decl::new(
            "tab_start_command",
            String,
            json!(""),
            Tab,
            "What a terminal was started with, as a shell reads it.",
        )
        .readonly(),
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
        Decl::new(
            "keys",
            Table,
            json!({}),
            Global,
            "The server's key tables, by table and key: each is `{call, args}`, called at the tab the key was pressed in (`root`, `prefix`, a block's own table).",
        ),
        Decl::new("client_width", Number, json!(0), Client, "The client's size in cells.").readonly(),
        Decl::new("client_height", Number, json!(0), Client, "The client's size in cells.").readonly(),
    ]
}
