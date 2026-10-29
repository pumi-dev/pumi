//! `pumi help [NAME]`: a function, an event, a block, a model, or a noun's commands. A function's
//! answer is the server's own (`registry.list("fn")`): how it is written as a command, what it
//! does, its flags and its example; a model's is `registry.list("model")`: what it is and its
//! fields. Events are the ones pumi fires itself, which the API knows statically.
use crate::client::{Address, Client};
use crate::nouns::{self, Command};
use pumi_api::{Error, ErrorKind};
use serde_json::{Value, json};

const USAGE: &str = "usage: pumi help [NAME]   a function (`tab.open`), an event (`tab.changed`), a block (`file`), a model (`session`), or a noun (`tab`)";

/// `pumi help [NAME]`, with `--json` for the entry as the server describes it.
pub fn main(address: &Address, args: &[String]) -> Result<i32, Error> {
    let json = args.iter().any(|word| word == "--json");
    // `tab.read`, or `tab read` as the command is written.
    let words: Vec<&str> = args.iter().filter(|word| !word.starts_with("--")).map(String::as_str).collect();
    let joined = words.join(".");
    let Some(name) = (!words.is_empty()).then_some(&joined) else {
        print!("{}", crate::usage_text());
        // And every command there is, one line each, when there is a server to ask.
        if let Ok(mut client) = Client::open(address, false) {
            client.introduce("cli")?;
            println!("\ncommands (`pumi help NAME` for one):");
            print!("{}", listing(&nouns::table(&mut client)?));
        }
        return Ok(0);
    };
    match Client::open(address, false) {
        Ok(mut client) => {
            client.introduce("cli")?;
            let commands = nouns::table(&mut client)?;
            // A command as it is written, a tab method's included: `tab.read` is `tab.call`'s.
            let parts: Vec<&str> = name.split('.').collect();
            if parts.len() == 2
                && let Some(command) = commands.iter().find(|command| command.words == parts)
                && command.method.is_some()
            {
                if json {
                    println!(
                        "{}",
                        serde_json::to_string_pretty(&json!({ "name": name, "desc": command.desc }))
                            .unwrap_or_default()
                    );
                    return Ok(0);
                }
                print!("{}", command.help());
                println!("\n    calls tab.call(TAB, \"{}\", {{…flags}})", parts[1]);
                return Ok(0);
            }
            if let Some(model) = model_of(&mut client, name)? {
                return model_help(&model, &commands, json);
            }
            if let Some(noun) = commands.iter().find(|command| command.words.len() > 1 && command.words[0] == *name) {
                return noun_help(&commands, &noun.words[0]);
            }
            let functions = client.call("registry.list", json!(["fn"]))?;
            if let Some(function) = functions.as_array().into_iter().flatten().find(|f| f["name"] == name.as_str()) {
                return function_help(function, json);
            }
            let blocks = client.call("registry.list", json!(["block", { "name": name }]));
            if let Ok(blocks) = blocks
                && let Some(block) = blocks.as_array().and_then(|blocks| blocks.first())
            {
                return block_help(block, json);
            }
        }
        Err(err) if err.kind != ErrorKind::ProviderUnavailable => return Err(err),
        // No server: the events are the API's own, and they are all a help can say without one.
        Err(_) => {}
    }
    if let Some(event) = pumi_api::events::all().iter().find(|event| event.name == *name) {
        return event_help(event, json);
    }
    Err(Error::not_found(format!("no function, event, block or model `{name}`\n{USAGE}")))
}

/// Every command, one line each: its words and the first line of what it does.
pub fn listing(commands: &[Command]) -> String {
    let mut rows: Vec<(String, String)> = commands
        .iter()
        .map(|command| (command.words.join(" "), command.desc.trim().lines().next().unwrap_or("").to_string()))
        .collect();
    rows.sort();
    rows.dedup();
    let width = rows.iter().map(|(words, _)| words.len()).max().unwrap_or(0);
    rows.iter().map(|(words, desc)| format!("  {words:width$}  {desc}\n")).collect()
}

/// A function: how it is written, what it does, its flags and its example.
fn function_help(function: &Value, json: bool) -> Result<i32, Error> {
    if json {
        println!("{}", serde_json::to_string_pretty(function).unwrap_or_default());
        return Ok(0);
    }
    let name = function["name"].as_str().unwrap_or_default();
    let form = match nouns::command_of(function) {
        Some(command) => command.form(),
        None => format!("pumi call {name}"),
    };
    println!("{form}");
    println!(
        "    {} · {} · runs on {}",
        function["provider"].as_str().unwrap_or(""),
        function["effect"].as_str().unwrap_or(""),
        function["runs_on"].as_str().unwrap_or("")
    );
    for line in function["desc"].as_str().unwrap_or("").trim().lines() {
        println!("    {line}");
    }
    if let Some(example) = function["examples"].as_array().and_then(|examples| examples.first()) {
        println!("\nexample:");
        println!("    pumi call {name} '{}'", example["args"]);
        if !example["result"].is_null() {
            println!("      → {}", example["result"]);
        }
    }
    for note in function["notes"].as_array().into_iter().flatten().filter_map(Value::as_str) {
        println!("\nnote: {note}");
    }
    println!("\narguments: {}", function["params_schema"]);
    Ok(0)
}

/// A noun's commands: `pumi help tab` lists what a tab is and can do.
fn noun_help(commands: &[Command], noun: &str) -> Result<i32, Error> {
    println!("pumi {noun} VERB [ID] [ARGS…]");
    for command in commands.iter().filter(|command| command.words.first().map(String::as_str) == Some(noun)) {
        let verbs = command.words[1..].join(" ");
        let verb = verbs.split(' ').next().unwrap_or("");
        println!("    {verb:<12}  {}", command.desc.trim().lines().next().unwrap_or(""));
    }
    println!("\nflags and arguments: {}", format!("pumi help {noun} VERB"));
    Ok(0)
}

/// The model `name` names, if one does: the server's `registry.list("model", {name})`, which knows
/// a model by its name in any case (`pumi help session` describes `Session`).
fn model_of(client: &mut Client, name: &str) -> Result<Option<Value>, Error> {
    match client.call("registry.list", json!(["model", { "name": name }])) {
        Ok(models) => Ok(models.as_array().and_then(|models| models.first()).cloned()),
        Err(err) if err.kind == ErrorKind::NotFound => Ok(None),
        Err(err) => Err(err),
    }
}

/// A model, as `registry.list("model")` describes it: what it is, its fields, and the verbs of the
/// noun that names it (`pumi session list` for `Session`).
fn model_help(model: &Value, commands: &[Command], json: bool) -> Result<i32, Error> {
    if json {
        println!("{}", serde_json::to_string_pretty(model).unwrap_or_default());
        return Ok(0);
    }
    let name = model["name"].as_str().unwrap_or_default();
    let id = match model["idPrefix"].as_str().filter(|prefix| !prefix.is_empty()) {
        Some(prefix) => format!(" ({prefix}N)"),
        None => String::new(),
    };
    println!("model {name}{id} · {}", model["desc"].as_str().unwrap_or_default());
    let fields: Vec<&Value> = model["fields"].as_array().into_iter().flatten().collect();
    let width = fields.iter().filter_map(|field| field["name"].as_str().map(str::len)).max().unwrap_or(0);
    for field in fields {
        let field_name = field["name"].as_str().unwrap_or_default();
        let watched = if field["watch"].as_bool().unwrap_or(false) { " · watched" } else { "" };
        println!(
            "    {field_name:width$}  {}  {}{watched}",
            field["type"].as_str().unwrap_or_default(),
            field["desc"].as_str().unwrap_or_default()
        );
    }
    let noun = name.to_lowercase();
    let verbs: Vec<String> = commands
        .iter()
        .filter(|command| command.words.first().map(String::as_str) == Some(noun.as_str()))
        .map(|command| command.words[1..].join(" "))
        .collect();
    if !verbs.is_empty() {
        println!("\nverbs: pumi {noun} VERB · {}", verbs.join(", "));
    }
    Ok(0)
}

/// An event: who fires it, what it is matched by, and the fields of its data.
fn event_help(event: &pumi_api::events::EventInfo, json: bool) -> Result<i32, Error> {
    if json {
        println!("{}", serde_json::to_string_pretty(event).unwrap_or_default());
        return Ok(0);
    }
    println!("event {}", event.name);
    println!(
        "    {} · matched by {}",
        event.provider,
        if event.matched.is_empty() { "nothing: it has none" } else { &event.matched }
    );
    for line in event.desc.trim().lines() {
        println!("    {line}");
    }
    println!("\ndata: {}", event.data.join(", "));
    println!("\nlisten: pumi event on '{}'", event.name);
    Ok(0)
}

/// A block, as `registry.list("block")` describes it: what it holds and what it answers.
fn block_help(block: &Value, json: bool) -> Result<i32, Error> {
    if json {
        println!("{}", serde_json::to_string_pretty(block).unwrap_or_default());
        return Ok(0);
    }
    let name = block["id"].as_str().or(block["name"].as_str()).unwrap_or_default();
    println!("block {name} · {}", block["provider"].as_str().unwrap_or(""));
    let list = |key: &str| -> String {
        let words: Vec<&str> = block[key].as_array().into_iter().flatten().filter_map(Value::as_str).collect();
        if words.is_empty() { "none".into() } else { words.join(", ") }
    };
    println!("    methods (tab.call): read, input, selection, report, {}", list("methods"));
    println!("    options: {}", list("options"));
    println!("\nopen: pumi tab open {name}");
    Ok(0)
}
