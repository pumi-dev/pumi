//! `pumi tab list` as a table: which tabs there are, where, what they hold and their status
//! (`@status`). With `--json` (or `--closed`) it is the server's `tab.list`, ids as scripts read
//! them. Made of the server's functions (`tab.list`, `tab.get`), so a script can do the same.
use crate::client::{Address, Client};
use crate::usage;
use pumi_api::{Error, ErrorKind};
use serde_json::{Value, json};

/// Whether `words` are `tab list` as a table; runs it if they are.
pub fn run(address: &Address, words: &[String], json: bool) -> Option<Result<i32, Error>> {
    let (noun, verb) = (words.first()?.as_str(), words.get(1).map(String::as_str));
    let rest = words.get(2..).unwrap_or_default();
    let json = json || rest.iter().any(|word| word == "--json");
    let asks_help = rest.iter().any(|word| word == "--help" || word == "-h");
    match (noun, verb) {
        ("tab", Some("list")) if !json && !asks_help && !rest.iter().any(|w| w == "--closed") => {
            Some(tab_list(address, rest))
        }
        _ => None,
    }
}

/// The words of a command line: its positionals, and its `--name value` flags.
struct Words {
    positionals: Vec<String>,
    flags: Vec<(String, String)>,
}

fn words(rest: &[String]) -> Result<Words, Error> {
    let mut out = Words { positionals: Vec::new(), flags: Vec::new() };
    let mut more = rest.iter();
    while let Some(word) = more.next() {
        if word == "--" {
            out.positionals.extend(more.by_ref().cloned());
            break;
        }
        match word.strip_prefix("--") {
            Some(long) => {
                let (name, inline) = long.split_once('=').map_or((long, None), |(n, v)| (n, Some(v.to_string())));
                let value =
                    inline.or_else(|| more.next().cloned()).ok_or_else(|| usage(format!("--{name} needs a value")))?;
                out.flags.push((name.to_string(), value));
            }
            None => out.positionals.push(word.clone()),
        }
    }
    Ok(out)
}
impl Words {
    fn flag(&self, name: &str) -> Option<&str> {
        self.flags.iter().find(|(n, _)| n == name).map(|(_, v)| v.as_str())
    }
    fn only(&self, allowed: &[&str], form: &str) -> Result<(), Error> {
        match self.flags.iter().find(|(n, _)| !allowed.contains(&n.as_str())) {
            Some((name, _)) => Err(usage(format!("`--{name}` is not one of its flags\n{form}"))),
            None => Ok(()),
        }
    }
}

/// The caller's own tab: `$PUMI_TAB`.
fn own_tab() -> Option<String> {
    std::env::var("PUMI_TAB").ok().filter(|tab| tab.starts_with('%'))
}

fn connect(address: &Address) -> Result<Client, Error> {
    let mut client = Client::open(address, true)?;
    client.introduce("cli")?;
    Ok(client)
}

/// `tab.list`'s filters from the command line: `--session`, `--view`, `--block`, `--trait`.
fn filters(words: &Words) -> Value {
    let mut opts = serde_json::Map::new();
    for name in ["session", "view", "block", "trait"] {
        if let Some(value) = words.flag(name) {
            opts.insert(name.into(), json!(value));
        }
    }
    Value::Object(opts)
}

/// Each tab `opts` lists, as `tab.get` has it; one that closed meanwhile is left out.
fn tabs(client: &mut Client, opts: Value) -> Result<Vec<Value>, Error> {
    let ids = client.call("tab.list", json!([opts]))?;
    let mut out = Vec::new();
    for id in ids.as_array().into_iter().flatten() {
        match client.call("tab.get", json!([id])) {
            Ok(tab) => out.push(tab),
            Err(err) if err.kind == ErrorKind::NotFound => {}
            Err(err) => return Err(err),
        }
    }
    Ok(out)
}

/// `pumi tab list [--session ID] [--view ID] [--block NAME] [--trait NAME]`: a table of the tabs.
fn tab_list(address: &Address, rest: &[String]) -> Result<i32, Error> {
    let words = words(rest)?;
    let form = "pumi tab list [--session ID] [--view ID] [--block NAME] [--trait NAME] [--closed] [--json]";
    words.only(&["session", "view", "block", "trait"], form)?;
    if !words.positionals.is_empty() {
        return Err(usage(format!("it takes no arguments\n{form}")));
    }
    let mut client = connect(address)?;
    let tabs = tabs(&mut client, filters(&words))?;
    let columns = ["TAB", "VIEW", "PANE", "BLOCK", "STATUS", "TITLE", "CWD"];
    let rows: Vec<Vec<String>> = tabs
        .iter()
        .map(|tab| {
            vec![
                id_cell(tab),
                text(&tab["view"]),
                place_cell(&tab["place"]),
                text(&tab["block"]),
                text(&tab["options"]["@status"]),
                text(&tab["title"]),
                home(&text(&tab["cwd"])),
            ]
        })
        .collect();
    print!("{}", table(&columns, &rows));
    Ok(0)
}

fn text(value: &Value) -> String {
    match value {
        Value::Null => String::new(),
        Value::String(text) => text.clone(),
        other => other.to_string(),
    }
}

/// A tab's id, `*` after the caller's own.
fn id_cell(tab: &Value) -> String {
    let id = text(&tab["id"]);
    if own_tab().as_deref() == Some(id.as_str()) { format!("{id}*") } else { id }
}

/// Where a tab is: its pane, or its kind of place (`float`, `hidden`).
fn place_cell(place: &Value) -> String {
    match place["pane"].as_str() {
        Some(pane) => pane.to_string(),
        None => text(&place["kind"]),
    }
}

/// A path with the home directory as `~`.
fn home(path: &str) -> String {
    let home = pumi_proto::paths::home_dir().display().to_string();
    match path.strip_prefix(&home) {
        Some(rest) if !home.is_empty() && (rest.is_empty() || rest.starts_with('/')) => format!("~{rest}"),
        _ => path.to_string(),
    }
}

/// Rows as columns, each as wide as its widest cell (the last as it is), cut at 48 chars.
pub fn table(columns: &[&str], rows: &[Vec<String>]) -> String {
    const MOST: usize = 48;
    let cut = |cell: &str| -> String {
        let first = cell.lines().next().unwrap_or("");
        if first.chars().count() <= MOST {
            return first.to_string();
        }
        let mut out: String = first.chars().take(MOST - 1).collect();
        out.push('…');
        out
    };
    let rows: Vec<Vec<String>> = rows.iter().map(|row| row.iter().map(|cell| cut(cell)).collect()).collect();
    // Columns no row has anything in are left out.
    let shown: Vec<usize> = match rows.is_empty() {
        true => (0..columns.len()).collect(),
        false => (0..columns.len()).filter(|i| rows.iter().any(|row| !row[*i].is_empty())).collect(),
    };
    let width = |i: usize| rows.iter().map(|row| row[i].chars().count()).chain([columns[i].len()]).max().unwrap_or(0);
    let widths: Vec<usize> = (0..columns.len()).map(width).collect();
    let line = |cells: Vec<&str>| -> String {
        let last = shown.len().saturating_sub(1);
        let mut out = String::new();
        for (n, i) in shown.iter().enumerate() {
            let cell = cells[*i];
            if n == last {
                out.push_str(cell);
            } else {
                out.push_str(cell);
                out.push_str(&" ".repeat(widths[*i] - cell.chars().count() + 2));
            }
        }
        out.trim_end().to_string() + "\n"
    };
    let mut out = line(columns.to_vec());
    for row in &rows {
        out.push_str(&line(row.iter().map(String::as_str).collect()));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn a_table_lines_its_columns_up_and_leaves_empty_ones_out() {
        let rows = vec![
            vec!["%3".to_string(), String::new(), "working".to_string()],
            vec!["%12".to_string(), String::new(), "idle".to_string()],
        ];
        assert_eq!(table(&["TAB", "VIEW", "STATUS"], &rows), "TAB  STATUS\n%3   working\n%12  idle\n");
    }
    #[test]
    fn words_are_positionals_and_flags() {
        let given: Vec<String> = ["--view", "@2", "--block=terminal", "--", "--literal"].map(String::from).to_vec();
        let words = words(&given).unwrap();
        assert_eq!(words.positionals, vec!["--literal"]);
        assert_eq!((words.flag("view"), words.flag("block")), (Some("@2"), Some("terminal")));
        assert!(words.only(&["view"], "form").is_err(), "--block is not allowed there");
        assert!(super::words(&["--session".to_string()]).is_err(), "a flag needs its value");
    }
}
