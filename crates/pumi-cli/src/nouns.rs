//! The API as commands: `pumi tab get %3`, `pumi option set linesize 1.4`, `pumi tab read %3
//! --lines 80`. A command is a function's name in words (`tab get` is `tab.get`), as the server
//! describes it (`registry.list("fn")`): its id and its other arguments come first, in the
//! function's own order, and the options of its last object argument (`opts`, `patch`, `place`)
//! are kebab-case flags. `--no-x` is `false`, `--x` is `true`, and a value is JSON when it reads
//! as JSON. `tab.call`'s methods are verbs of `tab` (`pumi tab read`, `pumi tab wait`), from
//! the server's `registry.list("tabMethod")`, and `pumi call` names a function instead of
//! spelling it.
use crate::client::{Address, Client};
use pumi_api::Error;
use serde_json::{Map, Value, json};
use std::io::{IsTerminal, Write};

/// The commands that are a `tab.call` method: the last argument is the method's own object.
const TAB: &str = "tab.call";

/// Whether `word` could start a command: a word of a function's name (`tab`, `plugin`).
pub fn could_be(word: &str) -> bool {
    !word.is_empty() && word.chars().all(|c| c.is_ascii_lowercase())
}

/// A command's argument that is not its options object: an id or a value, in the function's order.
#[derive(Clone, Debug, PartialEq)]
pub struct Param {
    pub name: String,
    pub required: bool,
    /// A field of the options object, written first, as an argument: a function whose one
    /// argument is an object (`tab.cells({tab, lines})`) takes its required fields so
    /// (`pumi tab cells %3 --lines 5`).
    pub option: bool,
}

/// An option of a command's last object argument, as a flag: `--title VALUE`, or `--state` and
/// `--no-state` when it takes a boolean.
#[derive(Clone, Debug, PartialEq)]
pub struct Flag {
    /// The option's name, camelCase, as the wire has it.
    pub key: String,
    pub boolean: bool,
    pub desc: String,
}
impl Flag {
    pub fn kebab(&self) -> String {
        kebab(&self.key)
    }
}

/// A command: a function as words, or one of `tab.call`'s methods.
#[derive(Clone, Debug, PartialEq)]
pub struct Command {
    /// `["tab", "open"]`
    pub words: Vec<String>,
    /// The function the command calls: `tab.open`, or `tab.call` for a tab method.
    pub name: String,
    /// The method, for a tab method.
    pub method: Option<String>,
    pub desc: String,
    pub params: Vec<Param>,
    pub flags: Vec<Flag>,
    /// Whether the options object is a required argument (`tab.update`'s `patch`).
    pub flags_required: bool,
}
impl Command {
    /// The command line in words: `tab get [TAB] [--state|--no-state]`.
    pub fn form(&self) -> String {
        let mut out = format!("pumi {}", self.words.join(" "));
        for param in &self.params {
            let name = param.name.to_uppercase();
            out.push(' ');
            out.push_str(&if param.required { name } else { format!("[{name}]") });
        }
        for flag in self.flags.iter().filter(|flag| !self.params.iter().any(|p| p.option && p.name == flag.key)) {
            out.push(' ');
            out.push_str(&if flag.boolean {
                format!("[--{}|--no-{}]", flag.kebab(), flag.kebab())
            } else {
                format!("[--{} VALUE]", flag.kebab())
            });
        }
        out
    }
    /// What `pumi help` says of it: how it is written, what it does, and its flags.
    pub fn help(&self) -> String {
        let mut out = self.form();
        out.push('\n');
        for line in self.desc.trim().lines() {
            out.push_str("    ");
            out.push_str(line);
            out.push('\n');
        }
        if !self.flags.is_empty() {
            out.push_str("\nflags:\n");
            let width = self.flags.iter().map(|f| f.kebab().len()).max().unwrap_or(0);
            for flag in &self.flags {
                let name = format!("--{}", flag.kebab());
                out.push_str(&format!("    {name:width$}  {}\n", flag.desc));
            }
        }
        out
    }
    fn flag(&self, key: &str) -> Option<&Flag> {
        self.flags.iter().find(|f| f.kebab() == key)
    }
    /// The arguments a call sends: the positionals in the function's order, and its options.
    fn wire(&self, given: &Given) -> Value {
        let mut options = given.flags.clone();
        let mut args: Vec<Value> = Vec::new();
        for (i, param) in self.params.iter().enumerate() {
            let value = given.positionals.get(i).cloned().unwrap_or(Value::Null);
            if param.option {
                if !value.is_null() {
                    options.insert(param.name.clone(), value);
                }
            } else {
                args.push(value);
            }
        }
        if let Some(method) = &self.method {
            args.push(json!(method));
        }
        if self.method.is_some() || !self.flags.is_empty() || self.flags_required {
            args.push(Value::Object(options));
        }
        Value::Array(args)
    }
}

/// The commands the server has: every function as words, and `tab.call`'s methods as `tab`'s.
pub fn table(client: &mut Client) -> Result<Vec<Command>, Error> {
    let functions = client.call("registry.list", json!(["fn"]))?;
    let mut out = Vec::new();
    for function in functions.as_array().into_iter().flatten() {
        let Some(name) = function["name"].as_str() else { continue };
        if name == "call" {
            continue;
        }
        if name == TAB {
            for method in client.call("registry.list", json!(["tabMethod"]))?.as_array().into_iter().flatten() {
                if let Some(command) = tab_method(method) {
                    out.push(command);
                }
            }
            continue;
        }
        // `owner.name.build` is a plugin's own function, not a noun and a verb.
        out.extend(command_of(function));
    }
    Ok(out)
}

/// A function as a command, from what `registry.list("fn")` says of it.
pub fn command_of(function: &Value) -> Option<Command> {
    let name = function["name"].as_str()?.to_string();
    let words: Vec<String> = name.split('.').map(str::to_string).collect();
    if words.len() != 2 {
        return None;
    }
    let required: Vec<&str> =
        function["params_schema"]["required"].as_array().into_iter().flatten().filter_map(Value::as_str).collect();
    let names: Vec<String> = function["parameters"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|param| Some(param.get(1)?.as_str()?.to_string()))
        .collect();
    let properties = &function["params_schema"]["properties"];
    // The last object argument is the flags: `opts` for most, `patch` for an update, `place` for
    // an open.
    let options = names.last().filter(|last| ["opts", "patch", "place"].contains(&last.as_str())).cloned();
    let mut params: Vec<Param> = names
        .iter()
        .filter(|param| options.as_ref() != Some(*param))
        .map(|param| Param { name: param.clone(), required: required.contains(&param.as_str()), option: false })
        .collect();
    // One argument, an object: its required fields are written first, as its arguments are.
    if params.is_empty()
        && let Some(options) = &options
    {
        params = required_fields(&properties[options.as_str()])
            .into_iter()
            .map(|name| Param { name, required: true, option: true })
            .collect();
    }
    let flags = options.as_ref().map(|options| flags_of(&properties[options.as_str()])).unwrap_or_default();
    let flags_required = options.as_ref().is_some_and(|options| required.contains(&options.as_str()));
    Some(Command {
        words,
        name,
        method: None,
        desc: function["desc"].as_str().unwrap_or_default().to_string(),
        params,
        flags,
        flags_required,
    })
}

/// One of `tab.call`'s methods, as `registry.list("tabMethod")` describes it: `tab.read
/// [TAB] [--lines VALUE]`.
fn tab_method(method: &Value) -> Option<Command> {
    let name = method["name"].as_str()?;
    Some(Command {
        words: vec!["tab".into(), name.into()],
        name: TAB.into(),
        method: Some(name.into()),
        desc: method["desc"].as_str().unwrap_or_default().to_string(),
        params: vec![Param { name: "tab".into(), required: false, option: false }],
        flags: flags_of(&method["paramsSchema"]),
        flags_required: false,
    })
}

/// The options of an object schema: its properties, or, for one that is `oneOf` (an `input`'s
/// `{text}` or `{keys}`), all of theirs.
fn flags_of(schema: &Value) -> Vec<Flag> {
    properties(schema)
        .into_iter()
        .map(|(key, schema)| Flag {
            boolean: accepts(&schema, "boolean"),
            desc: schema["description"].as_str().unwrap_or_default().to_string(),
            key,
        })
        .collect()
}

/// The fields an object schema requires, in its order (`anyOf [T, null]`'s `T`'s).
fn required_fields(schema: &Value) -> Vec<String> {
    if let Some(required) = schema.get("required").and_then(Value::as_array) {
        return required.iter().filter_map(Value::as_str).map(str::to_string).collect();
    }
    ["oneOf", "anyOf"]
        .iter()
        .filter_map(|key| schema.get(*key)?.as_array())
        .flatten()
        .map(required_fields)
        .find(|fields| !fields.is_empty())
        .unwrap_or_default()
}

fn properties(schema: &Value) -> Map<String, Value> {
    if let Some(properties) = schema.get("properties").and_then(Value::as_object) {
        return properties.clone();
    }
    // `oneOf` the shapes it may have, and `anyOf [T, null]` for one that may be left out.
    let mut merged = Map::new();
    for key in ["oneOf", "anyOf"] {
        for option in schema.get(key).and_then(Value::as_array).into_iter().flatten() {
            merged.extend(properties(option));
        }
    }
    merged
}

/// Whether a schema takes a value of `kind` (`boolean`), so that the flag can be `--x`/`--no-x`.
fn accepts(schema: &Value, kind: &str) -> bool {
    match &schema["type"] {
        Value::String(one) => one == kind,
        Value::Array(types) => types.iter().any(|one| one.as_str() == Some(kind)),
        _ => ["oneOf", "anyOf"]
            .iter()
            .filter_map(|key| schema.get(*key)?.as_array())
            .flatten()
            .any(|option| accepts(option, kind)),
    }
}

/// A name as a flag: `activeView` is `active-view`.
fn kebab(name: &str) -> String {
    let mut out = String::new();
    for c in name.chars() {
        if c.is_ascii_uppercase() {
            out.push('-');
            out.push(c.to_ascii_lowercase());
        } else {
            out.push(c);
        }
    }
    out
}

/// The sigil an id's parameter takes (`%3`, `:4`, `@2`, `$1`, `+4`), when it is one.
fn sigil(param: &str) -> Option<char> {
    match param {
        "tab" => Some('%'),
        "pane" => Some(':'),
        "view" => Some('@'),
        "session" => Some('$'),
        "request" => Some('+'),
        _ => None,
    }
}

/// A bare id, `3`, as an id's parameter says it: a sigil and the number.
fn bare_id(word: &str) -> Option<String> {
    (!word.is_empty() && word.chars().all(|c| c.is_ascii_digit()) && !word.starts_with('0')).then(|| word.to_string())
}

/// The command line's arguments, as far as they have been read.
pub struct Given {
    pub positionals: Vec<Value>,
    pub flags: Map<String, Value>,
    pub json: bool,
}

/// Reads a command's arguments: its positionals in order, and its flags.
fn given(command: &Command, rest: &[String], json: bool) -> Result<Given, Error> {
    let mut given = Given { positionals: Vec::new(), flags: Map::new(), json };
    let mut words = rest.iter().peekable();
    let mut literal = false;
    let mut program: Option<Vec<Value>> = None;
    while let Some(word) = words.next() {
        if !literal && word == "--" {
            // What follows is a program and its words, for a command that runs one: a
            // terminal's `command` (`pumi tab open pumi_dev.terminal.shell -- pi "hi"`,
            // `pumi tab run %3 -- ls`).
            if runs_a_program(command) {
                program = Some(words.by_ref().map(|word| Value::String(word.clone())).collect());
                break;
            }
            literal = true;
            continue;
        }
        if !literal && word == "--json" {
            given.json = true;
            continue;
        }
        if !literal && word.starts_with("--") {
            let long = &word[2..];
            let (key, inline) = long.split_once('=').map_or((long, None), |(key, value)| (key, Some(value)));
            if let Some(name) = key.strip_prefix("no-") {
                match command.flag(name) {
                    Some(flag) if flag.boolean => given.flags.insert(flag.key.clone(), Value::Bool(false)).map(|_| ()),
                    _ => return Err(usage(command, format!("`--no-{name}` is not one of its flags"))),
                };
                continue;
            }
            let flag = command.flag(key).ok_or_else(|| {
                let flags: Vec<String> = command.flags.iter().map(|f| format!("--{}", f.kebab())).collect();
                let flags = if flags.is_empty() { "none".to_string() } else { flags.join(", ") };
                usage(command, format!("`--{key}` is not one of its flags ({flags})"))
            })?;
            let value = match inline {
                Some(value) => text(value),
                None if flag.boolean => Value::Bool(true),
                None => match words.next_if(|next| !next.starts_with("--")) {
                    Some(next) => text(next),
                    None => return Err(usage(command, format!("--{key} needs a value"))),
                },
            };
            given.flags.insert(flag.key.clone(), value);
            continue;
        }
        if !literal && word.starts_with('-') && word.len() > 1 {
            return Err(usage(command, format!("`{word}` is not a flag: `--{} VALUE`", word.trim_start_matches('-'))));
        }
        given.positionals.push(value(&given.positionals, command, word));
    }
    if let Some(program) = program.filter(|words| !words.is_empty()) {
        run_program(command, &mut given, program)?;
    }
    infer_kind(command, &mut given)?;
    fill_defaults(command, &mut given)?;
    if given.positionals.len() > command.params.len() {
        let takes: Vec<String> = command.params.iter().map(|param| param.name.to_uppercase()).collect();
        return Err(usage(
            command,
            format!(
                "it takes {} argument(s){}",
                command.params.len(),
                if takes.is_empty() { String::new() } else { format!(": {}", takes.join(" ")) }
            ),
        ));
    }
    for (i, param) in command.params.iter().enumerate() {
        let flagged = param.option && given.flags.contains_key(&param.name);
        if param.required && !flagged && given.positionals.get(i).is_none_or(Value::is_null) {
            return Err(usage(command, format!("`{}` is missing", param.name.to_uppercase())));
        }
    }
    let fields_given = command
        .params
        .iter()
        .enumerate()
        .any(|(i, param)| param.option && given.positionals.get(i).is_some_and(|value| !value.is_null()));
    if command.flags_required && given.flags.is_empty() && !fields_given {
        let flags: Vec<String> = command.flags.iter().map(|f| format!("--{}", f.kebab())).collect();
        return Err(usage(command, format!("it needs one of its flags: {}", flags.join(", "))));
    }
    Ok(given)
}

/// Whether a command runs a program: it has a block's `args` (`tab open`), or a `command` option
/// (`tab run`).
fn runs_a_program(command: &Command) -> bool {
    command.params.iter().any(|param| param.name == "args") || command.flag("command").is_some()
}

/// The words after `--` as the program a command runs: `args.command` for `tab open` (merged
/// into an `args` object given too), else the `command` option.
fn run_program(command: &Command, given: &mut Given, program: Vec<Value>) -> Result<(), Error> {
    let Some(at) = command.params.iter().position(|param| param.name == "args") else {
        given.flags.insert("command".into(), Value::Array(program));
        return Ok(());
    };
    while given.positionals.len() < at {
        given.positionals.push(Value::Null);
    }
    match given.positionals.get_mut(at) {
        None => given.positionals.push(json!({ "command": program })),
        Some(Value::Null) => given.positionals[at] = json!({ "command": program }),
        Some(Value::Object(args)) if !args.contains_key("command") => {
            args.insert("command".into(), Value::Array(program));
        }
        Some(_) => {
            return Err(usage(command, "a program after `--`, or `command` in ARGS, not both"));
        }
    }
    Ok(())
}

/// The options only a tiled place has, and only a floating one.
const TILED: &[&str] = &["pane", "split", "ratio", "size", "full", "replace", "beside"];
const FLOATING: &[&str] = &[
    "relative",
    "anchor",
    "row",
    "col",
    "width",
    "height",
    "border",
    "focusable",
    "mouse",
    "dismiss",
    "backdrop",
    "zIndex",
];

/// A place's `kind`, where its other flags say it: `--split right` is tiled, `--anchor NE`
/// floats. One that names both, or neither, is left to say it itself (`--kind`).
fn infer_kind(command: &Command, given: &mut Given) -> Result<(), Error> {
    if command.flag("kind").is_none() || command.flag("split").is_none() || given.flags.contains_key("kind") {
        return Ok(());
    }
    let named = |keys: &[&str]| keys.iter().find(|key| given.flags.contains_key(**key)).map(|key| kebab(key));
    let kind = match (named(TILED), named(FLOATING)) {
        (Some(tiled), Some(floating)) => {
            return Err(usage(
                command,
                format!(
                    "--{tiled} is a tiled place's and --{floating} a floating one's: add --kind tiled or --kind float"
                ),
            ));
        }
        (Some(_), None) => "tiled",
        (None, Some(_)) => "float",
        (None, None) => return Ok(()),
    };
    given.flags.insert("kind".into(), json!(kind));
    Ok(())
}

/// A server's error, with how to fix the command line where it can say: a place that is missing
/// its `kind` (`argument \`place\`: missing field \`kind\``) needs `--kind`.
fn explained(command: &Command, err: Error) -> Error {
    if command.flag("kind").is_some() && err.message.contains("missing field `kind`") {
        let mut err = err;
        err.message = format!(
            "{}: add --kind tiled (in a pane: --split, --pane, --beside), --kind float, --kind view or --kind session",
            err.message
        );
        return err;
    }
    err
}

/// A positional argument: an id's bare number takes its sigil (`3` is `%3`), and anything else is
/// JSON when it reads as JSON and text when it doesn't.
fn value(taken: &[Value], command: &Command, word: &str) -> Value {
    if let Some(name) = command.params.get(taken.len()).map(|param| param.name.as_str())
        && let Some(sigil) = sigil(name)
        && let Some(id) = bare_id(word)
    {
        return json!(format!("{sigil}{id}"));
    }
    text(word)
}

/// A word as a value: JSON when it is JSON (`3`, `true`, `{"a":1}`), the text otherwise.
fn text(word: &str) -> Value {
    serde_json::from_str(word).unwrap_or_else(|_| Value::String(word.to_string()))
}

/// The ids a tab's environment names, where a command's own argument leaves it out: `$PUMI_SESSION`
/// and `$PUMI_VIEW`.
fn fill_defaults(command: &Command, given: &mut Given) -> Result<(), Error> {
    for (i, param) in command.params.iter().enumerate() {
        if given.positionals.len() > i {
            continue;
        }
        let var = match param.name.as_str() {
            "session" => "PUMI_SESSION",
            "view" => "PUMI_VIEW",
            _ => continue,
        };
        let Some(id) = std::env::var(var).ok().filter(|id| !id.is_empty()) else { continue };
        while given.positionals.len() < i {
            given.positionals.push(Value::Null);
        }
        given.positionals.push(json!(id));
    }
    Ok(())
}

/// A command's own usage error, with the command's form.
fn usage(command: &Command, message: impl std::fmt::Display) -> Error {
    crate::usage(format!("{message}\n{}", command.form()))
}

/// Runs `words` as a command, if they name one; `None` when they don't.
pub fn run(address: &Address, words: &[String], json: bool) -> Result<Option<i32>, Error> {
    run_with(address, words, json, |_| Ok(()))
}

/// `run`, with what was read of the command line made into its call by `adapt`: a shortcut's
/// own arguments (`pumi open PATH`) as the function's.
pub fn run_with(
    address: &Address,
    words: &[String],
    json: bool,
    adapt: impl FnOnce(&mut Given) -> Result<(), Error>,
) -> Result<Option<i32>, Error> {
    let Some(first) = words.first() else { return Ok(None) };
    if !could_be(first) {
        return Ok(None);
    }
    let mut client = Client::open(address, true)?;
    client.introduce("cli")?;
    let commands = table(&mut client)?;
    let Some(command) =
        commands.iter().filter(|command| words.starts_with(&command.words)).max_by_key(|c| c.words.len())
    else {
        return Err(unknown(&commands, words));
    };
    let rest = &words[command.words.len()..];
    if rest.iter().any(|word| word == "--help" || word == "-h") {
        print!("{}", command.help());
        return Ok(Some(0));
    }
    let mut given = given(command, rest, json)?;
    adapt(&mut given)?;
    if command.method.as_deref() == Some("read")
        && !given.flags.contains_key("lines")
        && !given.flags.contains_key("source")
    {
        // Enough to see where a program is, not its whole scrollback: `--lines` says more.
        given.flags.insert("lines".into(), json!(READ_LINES));
    }
    let answer = client.call(&command.name, command.wire(&given)).map_err(|err| explained(command, err))?;
    match (command.name.as_str(), command.method.as_deref()) {
        (TAB, Some("wait")) => Ok(Some(waited(&answer, given.flags.contains_key("timeout")))),
        ("event.on", _) => {
            stream(&mut client, &answer, given.json)?;
            Ok(Some(0))
        }
        _ => {
            show(&answer, given.json)?;
            Ok(Some(0))
        }
    }
}

/// The lines `pumi tab read` reads when it is not told how many, or which.
pub const READ_LINES: u64 = 50;

/// `tab wait`'s exit status: the program's, 0 for text it showed, `TIMED_OUT` when its time ran
/// out first.
const TIMED_OUT: i32 = 124;
fn waited(answer: &Value, timed: bool) -> i32 {
    match answer["exitCode"].as_i64() {
        Some(code) => code as i32,
        None if timed && answer["match"].is_null() => TIMED_OUT,
        None => 0,
    }
}

/// `event.on`: every event the subscription answers, one to a line, until the CLI is told to stop.
fn stream(client: &mut Client, subscription: &Value, json: bool) -> Result<(), Error> {
    let mut out = std::io::stdout().lock();
    loop {
        let message = client.notification()?;
        if message["method"] != "rpc.event" || message["params"][0] != *subscription {
            continue;
        }
        let event = &message["params"][1];
        let line = if json { event.to_string() } else { heard(event) };
        if writeln!(out, "{line}").and_then(|()| out.flush()).is_err() {
            return Ok(());
        }
    }
}

/// An event as a line: its turn, its name, what matched, the ids it is about, and its data.
fn heard(event: &Value) -> String {
    let name = event["event"].as_str().unwrap_or("");
    let matched = event["match"].as_str().filter(|m| !m.is_empty()).map(|m| format!(" {m}")).unwrap_or_default();
    let mut about = String::new();
    for key in ["session", "view", "tab", "request"] {
        if let Some(id) = event[key].as_str() {
            about.push(' ');
            about.push_str(id);
        }
    }
    let data = match &event["data"] {
        Value::Null => String::new(),
        data => format!(" {data}"),
    };
    format!("{} {name}{matched}{about}{data}", event["seq"])
}

/// What a command answers: a string as it is, a list as a table on a terminal, and JSON
/// otherwise (and always with `--json`).
fn show(answer: &Value, json: bool) -> Result<(), Error> {
    let terminal = std::io::stdout().is_terminal();
    match answer {
        Value::Null => Ok(()),
        Value::String(text) if !json => {
            println!("{text}");
            Ok(())
        }
        // `{text}` (`tab read`, `tab selection`) is its text.
        Value::Object(only) if !json && only.len() == 1 && only.get("text").is_some_and(Value::is_string) => {
            println!("{}", only["text"].as_str().unwrap_or_default());
            Ok(())
        }
        Value::Array(rows) if !json && terminal => {
            print!("{}", rows_table(rows));
            Ok(())
        }
        other if !json && terminal => {
            println!("{}", serde_json::to_string_pretty(other).unwrap_or_default());
            Ok(())
        }
        other => {
            println!("{other}");
            Ok(())
        }
    }
}

/// Rows as a table: `id` first, then every other field in the order they are first seen.
fn rows_table(rows: &[Value]) -> String {
    let objects: Vec<&Map<String, Value>> = rows.iter().filter_map(Value::as_object).collect();
    if objects.len() != rows.len() {
        let text: Vec<String> = rows.iter().map(|row| row.to_string()).collect();
        return if text.is_empty() { String::new() } else { format!("{}\n", text.join("\n")) };
    }
    if objects.is_empty() {
        return String::new();
    }
    let mut columns: Vec<String> = Vec::new();
    if objects.iter().any(|object| object.contains_key("id")) {
        columns.push("id".into());
    }
    for object in &objects {
        for key in object.keys() {
            if !columns.contains(key) {
                columns.push(key.clone());
            }
        }
    }
    let cells: Vec<Vec<String>> =
        objects.iter().map(|object| columns.iter().map(|column| cell(&object[column.as_str()])).collect()).collect();
    let widths: Vec<usize> = columns
        .iter()
        .enumerate()
        .map(|(i, column)| cells.iter().map(|row| row[i].len()).chain([column.len()]).max().unwrap_or(0))
        .collect();
    let mut out = String::new();
    let line = |values: &[String], out: &mut String| {
        let last = values.len().saturating_sub(1);
        for (i, value) in values.iter().enumerate() {
            if i == last {
                out.push_str(value);
            } else {
                out.push_str(&format!("{value:width$}  ", width = widths[i]));
            }
        }
        out.push('\n');
    };
    line(&columns.iter().map(|c| c.to_uppercase()).collect::<Vec<_>>(), &mut out);
    for row in &cells {
        line(row, &mut out);
    }
    out
}

/// One cell: its first line, and short.
fn cell(value: &Value) -> String {
    match value {
        Value::Null => String::new(),
        Value::String(text) => short(text.lines().next().unwrap_or("")),
        other => short(&other.to_string()),
    }
}

fn short(text: &str) -> String {
    const MOST: usize = 60;
    if text.chars().count() <= MOST {
        return text.to_string();
    }
    let mut out: String = text.chars().take(MOST - 1).collect();
    out.push('…');
    out
}

/// What a word that names no command means: the noun's verbs, or the nouns.
fn unknown(commands: &[Command], words: &[String]) -> Error {
    let noun = &words[0];
    let verbs: Vec<&str> = commands
        .iter()
        .filter(|command| command.words.len() > 1 && command.words[0] == *noun)
        .filter_map(|command| command.words.get(1).map(String::as_str))
        .collect();
    if verbs.is_empty() {
        let mut nouns: Vec<&str> =
            commands.iter().filter_map(|command| command.words.first().map(String::as_str)).collect();
        nouns.dedup();
        return crate::usage(format!("no command `{}`; the nouns are: {}", words.join(" "), nouns.join(", ")));
    }
    let wanted = words.get(1).map_or("", String::as_str);
    let name = format!("{noun}.{wanted}");
    if APP_FUNCTIONS.contains(&name.as_str()) {
        return Error::new(
            pumi_api::ErrorKind::ProviderUnavailable,
            format!(
                "`{name}` is the app's, and no app is connected to this server (is pumi's window open?); `pumi tab read` reads a tab's text without it"
            ),
        );
    }
    Error::not_found(format!("`{noun}` has no `{wanted}`; it has: {}", verbs.join(", ")))
}

/// The functions the app provides (`src/system/mod.rs`), there only while it is connected: a command
/// for one is unknown without it, and says why.
const APP_FUNCTIONS: &[&str] = &["tab.cells", "app.profile"];
