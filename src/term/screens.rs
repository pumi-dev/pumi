use super::grid::{Cell, Grid};
use serde_json::{Value, json};
use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};
pub const FUNCTION: &str = "tab.cells";
const KEEP: usize = 64;
fn screens() -> &'static Mutex<HashMap<String, Arc<Grid>>> {
    static SCREENS: OnceLock<Mutex<HashMap<String, Arc<Grid>>>> = OnceLock::new();
    SCREENS.get_or_init(Default::default)
}
fn lock() -> std::sync::MutexGuard<'static, HashMap<String, Arc<Grid>>> {
    screens().lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}
pub fn publish(tab: &str, grid: Arc<Grid>) {
    let mut screens = lock();
    if screens.len() >= KEEP
        && !screens.contains_key(tab)
        && let Some(oldest) = screens.keys().next().cloned()
    {
        screens.remove(&oldest);
    }
    screens.insert(tab.to_string(), grid);
}
#[cfg(test)]
pub fn forget(tab: &str) {
    lock().remove(tab);
}
/// The screen the app last had of a tab, for the app's own drawing (a theme's previews).
pub fn grid(tab: &str) -> Option<Arc<Grid>> {
    lock().get(tab).cloned()
}
pub fn info() -> Value {
    json!({
        "name": FUNCTION,
        "since": 1,
        "provider": "app",
        "runs_on": "client",
        "effect": "read",
        "desc": "A tab's screen as the app last drew it: its last rows, in runs of cells with the \
                 colours the terminal gave them. The app's own screen — the one it parsed from the \
                 tab's bytes — so nothing is resized to take it, and a tab it never drew has none.",
        // One argument, `opts`: a function's `params_schema` names its arguments, in order, as
        // the server's `fn.set` reads it, so the object's own fields are one level down.
        "parameters": [["Dict", "opts"]],
        "params_schema": {
            "type": "object",
            "properties": {
                "opts": {
                    "type": "object",
                    "properties": {
                        "tab": { "type": ["integer", "string"], "description": "The tab (`%3`)." },
                        "lines": { "type": "integer", "description": "How many rows of it, from the last that is not blank. 20 by default." },
                        "trim": { "type": "boolean", "description": "Drop trailing blank cells (the default), so a row is only as wide as its text." }
                    },
                    "required": ["tab"],
                    "additionalProperties": false
                }
            },
            "required": ["opts"],
            "additionalProperties": false
        },
        "returns": "Object",
        "returns_schema": {
            "type": "object",
            "properties": {
                "tab": { "type": "string" },
                "cols": { "type": "integer" },
                "rows": { "type": "integer" },
                "cursor": { "type": ["object", "null"] },
                "default_fg": { "type": "string" },
                "default_bg": { "type": "string" },
                "lines": { "type": "array" }
            }
        },
        "examples": [{ "args": [{ "tab": "%3", "lines": 12 }] }],
        "notes": [
            "The screen as the app last drew it: a tab in a view that is not showing keeps the screen it had when it was.",
            "A tab the app has not drawn since it started has no screen: `provider_unavailable`.",
        ],
        "source": "app"
    })
}
pub fn answer(name: &str, args: &Value) -> Option<Result<Value, pumi_api::Error>> {
    match name {
        FUNCTION => Some(screen(&opts_of(args))),
        _ => None,
    }
}
/// `tab.cells`'s arguments as its one object: `[{tab, lines}]`, or, from a caller that gave the
/// tab itself, `["%3", 30]` (`tab.cells("%3")`).
fn opts_of(args: &Value) -> Value {
    let list = match args {
        Value::Array(list) => list.as_slice(),
        one => std::slice::from_ref(one),
    };
    match list.first() {
        Some(Value::Object(opts)) => Value::Object(opts.clone()),
        Some(tab @ (Value::String(_) | Value::Number(_))) => {
            let mut opts = serde_json::Map::new();
            opts.insert("tab".into(), tab.clone());
            if let Some(lines) = list.get(1).filter(|lines| !lines.is_null()) {
                opts.insert("lines".into(), lines.clone());
            }
            if let Some(trim) = list.get(2).filter(|trim| !trim.is_null()) {
                opts.insert("trim".into(), trim.clone());
            }
            Value::Object(opts)
        }
        _ => json!({}),
    }
}
fn unavailable(tab: &str) -> pumi_api::Error {
    pumi_api::Error::new(
        pumi_api::ErrorKind::ProviderUnavailable,
        format!("the app has no screen of {tab}: it is not a terminal tab it has drawn"),
    )
}
fn screen(args: &Value) -> Result<Value, pumi_api::Error> {
    let id = match args.get("tab") {
        Some(Value::Number(n)) => format!("%{n}"),
        Some(Value::String(text)) if !text.is_empty() => text.clone(),
        _ => {
            return Err(pumi_api::Error::invalid_params(
                "tab.cells takes `{tab}`: `pumi tab cells %3 --lines 20`, or `tab.cells({tab: \"%3\"})`",
            ));
        }
    };
    let lines = args.get("lines").and_then(Value::as_u64).unwrap_or(20).clamp(1, 500) as usize;
    let trim = args.get("trim").and_then(Value::as_bool).unwrap_or(true);
    let grid = lock().get(&id).cloned().ok_or_else(|| unavailable(&id))?;
    Ok(describe(&id, &grid, lines, trim))
}
fn describe(tab: &str, grid: &Grid, lines: usize, trim: bool) -> Value {
    let last = grid.rows.iter().rposition(|row| !row.iter().all(empty)).map_or(0, |last| last + 1);
    let from = last.saturating_sub(lines);
    json!({
        "tab": tab,
        "cols": grid.cols,
        "rows": grid.rows.len(),
        "from": from,
        "cursor": grid.cursor.map(|cursor| json!({
            "x": cursor.x,
            "y": cursor.y,
            "shape": match cursor.shape {
                super::CursorShape::Block => "block",
                super::CursorShape::Bar => "bar",
                super::CursorShape::Underline => "underline",
                super::CursorShape::Hollow => "hollow",
            },
        })),
        "default_fg": hex(grid.default_fg),
        "default_bg": hex(grid.default_bg),
        "lines": grid.rows[from..last].iter().map(|row| runs(row, trim)).collect::<Vec<Value>>(),
    })
}
fn empty(cell: &Cell) -> bool {
    cell.bg.is_none() && cell.text.as_str().trim().is_empty()
}
fn runs(row: &[Cell], trim: bool) -> Value {
    let cells: Vec<&Cell> = match trim {
        true => {
            let end = row.iter().rposition(|cell| !empty(cell)).map_or(0, |end| end + 1);
            row[..end].iter().collect()
        }
        false => row.iter().collect(),
    };
    let mut runs: Vec<Value> = Vec::new();
    let mut current: Option<(Value, String)> = None;
    for cell in cells {
        let style = json!({
            "fg": hex(cell.fg),
            "bg": cell.bg.map(hex),
            "bold": cell.bold,
            "italic": cell.italic,
            "underline": cell.underline.is_some(),
            "strike": cell.strike,
            "link": cell.link,
        });
        let text = cell.text.as_str();
        match current.take() {
            Some((same, mut run)) if same == style => {
                run.push_str(text);
                current = Some((same, run));
            }
            Some((same, run)) => {
                runs.push(run_of(&same, &run));
                current = Some((style, text.to_string()));
            }
            None => current = Some((style, text.to_string())),
        }
    }
    if let Some((style, run)) = current {
        runs.push(run_of(&style, &run));
    }
    Value::Array(runs)
}
fn run_of(style: &Value, text: &str) -> Value {
    let mut run = style.clone();
    run["text"] = Value::String(text.to_string());
    run
}
fn hex(reference: u32) -> String {
    format!("#{:06x}", crate::theme::terminal::resolve(reference))
}
#[cfg(test)]
mod tests {
    use super::*;
    fn row(text: &str) -> Vec<Cell> {
        text.chars().map(Cell::plain).collect()
    }
    fn grid(lines: &[&str]) -> Grid {
        let mut grid = Grid::default();
        grid.cols = lines.iter().map(|line| line.chars().count()).max().unwrap_or(0) as u16;
        grid.rows = lines.iter().map(|line| Arc::new(row(line))).collect();
        grid
    }
    #[test]
    fn the_last_lines_that_hold_something_and_no_further() {
        let grid = grid(&["one", "two", "", "", ""]);
        let described = describe("%1", &grid, 2, true);
        assert_eq!(described["from"], 0, "the last rows that hold something are the first two");
        assert_eq!(described["lines"].as_array().map(Vec::len), Some(2));
    }
    #[test]
    fn a_row_is_runs_of_cells_that_share_a_style() {
        let described = describe("%1", &grid(&["$ ls -la"]), 5, true);
        let first = &described["lines"][0];
        assert_eq!(first.as_array().map(Vec::len), Some(1), "one style, one run: {first}");
        assert_eq!(first[0]["text"], "$ ls -la");
        assert_eq!(first[0]["bold"], false);
    }
    #[test]
    fn trailing_blanks_go_and_kept_ones_stay() {
        let grid = grid(&["a   "]);
        let trimmed = describe("%1", &grid, 5, true);
        assert_eq!(trimmed["lines"][0][0]["text"], "a", "the blanks after the text go");
        let kept = describe("%1", &grid, 5, false);
        assert_eq!(kept["lines"][0][0]["text"], "a   ", "unless asked to keep them");
    }
    #[test]
    fn a_cell_with_a_background_keeps_the_blanks_after_it() {
        let mut grid = grid(&["a   "]);
        Arc::get_mut(&mut grid.rows[0]).map(|cells| cells[1].bg = Some(0x00ff00));
        let described = describe("%1", &grid, 5, true);
        let row = &described["lines"][0];
        assert_eq!(row[0]["text"], "a", "the text before the background");
        assert_eq!(row[1]["text"], " ", "and the cell the background keeps: {row}");
    }
    #[test]
    fn the_function_info_is_one_the_registry_takes() {
        let info: Result<pumi_api::FunctionInfo, _> = serde_json::from_value(info());
        let info = match info {
            Ok(info) => info,
            Err(err) => panic!("the registry reads this: {err}"),
        };
        assert_eq!(info.name, FUNCTION);
        assert_eq!((info.runs_on, info.effect), (pumi_api::RunsOn::Client, pumi_api::Effect::Read));
        assert_eq!(info.provider, "app");
        assert!(info.validate().is_ok(), "a function without a description, schema or example is refused");
    }
    #[test]
    fn a_tab_the_app_never_drew_has_no_screen() {
        let err = screen(&json!({ "tab": 9999 })).unwrap_err();
        assert_eq!(err.kind, pumi_api::ErrorKind::ProviderUnavailable);
    }
    #[test]
    fn a_published_screen_is_answered_and_goes_when_the_tab_does() {
        publish("%7001", Arc::new(grid(&["$ echo hi", "hi"])));
        let answered = match answer(FUNCTION, &json!([{ "tab": 7001, "lines": 5 }])) {
            Some(Ok(answer)) => answer,
            other => panic!("a screen for a tab that has one: {other:?}"),
        };
        assert_eq!(answered["tab"], "%7001");
        assert_eq!(answered["lines"][1][0]["text"], "hi");
        assert_eq!(answered["default_bg"].as_str().map(str::len), Some(7), "the tab's own background");
        forget("%7001");
        let err = screen(&json!({ "tab": 7001 })).unwrap_err();
        assert_eq!(err.kind, pumi_api::ErrorKind::ProviderUnavailable, "a closed tab's screen goes with it");
        assert!(answer("tab.something_else", &json!([])).is_none(), "only the app's own functions are answered here");
    }
    #[test]
    fn the_tab_is_its_object_or_given_as_itself() {
        publish("%7002", Arc::new(grid(&["a", "b", "c"])));
        for args in [json!([{ "tab": "%7002", "lines": 2 }]), json!(["%7002", 2]), json!([7002, 2])] {
            let answered = answer(FUNCTION, &args).and_then(Result::ok).unwrap_or_else(|| panic!("{args}"));
            assert_eq!(answered["lines"].as_array().map(Vec::len), Some(2), "{args}");
        }
        forget("%7002");
        let err = screen(&opts_of(&json!([]))).unwrap_err();
        assert!(err.message.contains("pumi tab cells %3"), "the error says how: {}", err.message);
    }
    #[test]
    fn its_schema_names_its_one_argument_as_the_server_reads_it() {
        // `fn.set` makes a function's parameters of its schema's properties: one, `opts`.
        let info = info();
        let names: Vec<&str> =
            info["params_schema"]["properties"].as_object().unwrap().keys().map(String::as_str).collect();
        assert_eq!(names, vec!["opts"]);
        assert_eq!(info["params_schema"]["properties"]["opts"]["required"], json!(["tab"]));
    }
}
