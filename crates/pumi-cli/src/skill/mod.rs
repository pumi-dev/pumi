//! `pumi skill`: agents' hooks — `add`, `remove` and `list` install them (the integrations in
//! `integrations.rs`), and `hook` is what an agent's hook runs (`hook.rs`): at each point of its
//! life it tells the core, prints what the agent is to be told, and at a turn's end hands it what
//! was queued.
/// What an agent's hook runs, and the words it takes.
pub mod hook;
pub mod integrations;
pub mod settings;
pub mod options {
    pub const ENABLED: &str = "agents.enabled";
    pub const RESTORE: &str = "agents.restore";
    pub const PERMISSIONS: &str = "agents.permissions";
    pub const RESTORE_COMMAND: &str = "restore_command";
}
/// How long a permission hook waits for an answer in pumi before the agent asks in its own
/// prompt, in seconds.
pub const PERMISSION_WAIT: u64 = 120;
use crate::client::Address;
use integrations::Shape;
use pumi_api::Error;
use serde_json::json;

const USAGE: &str = "usage: pumi skill add [AGENT…]              a hook for each agent, or for those on the PATH
       pumi skill remove [AGENT…]           take them out again
       pumi skill list [--json]             what is installed, and what each agent accepts
       pumi skill hook AGENT EVENT [flags]  what a hook runs: start, prompt, stop, compact, permission";

pub fn main(address: &Address, args: &[String]) -> Result<i32, Error> {
    let json = args.iter().any(|word| word == "--json");
    let words: Vec<&String> = args.iter().filter(|word| !word.starts_with("--")).collect();
    match words.first().map(|word| word.as_str()) {
        Some("add" | "remove") => {
            let add = words[0] == "add";
            let chosen: Vec<&'static integrations::Agent> = if words.len() > 1 {
                words[1..]
                    .iter()
                    .map(|id| integrations::find(id).ok_or_else(|| Error::not_found(format!("no agent `{id}`"))))
                    .collect::<Result<_, _>>()?
            } else {
                integrations::AGENTS
                    .iter()
                    .filter(|agent| {
                        agent.shape != Shape::None && (if add { integrations::on_path(agent.command) } else { true })
                    })
                    .collect()
            };
            for agent in chosen {
                if add {
                    match integrations::install(agent) {
                        Ok(file) => println!("{}: hook in {}", agent.label, pumi_proto::paths::display(&file)),
                        Err(err) => eprintln!("{}: {err}", agent.label),
                    }
                } else {
                    match integrations::uninstall(agent) {
                        Ok(()) => println!("{}: removed", agent.label),
                        Err(err) => eprintln!("{}: {err}", agent.label),
                    }
                }
            }
            Ok(0)
        }
        Some("list") | None => {
            let report = integrations::report();
            if json {
                println!("{report}");
                return Ok(0);
            }
            for agent in report["agents"].as_array().cloned().unwrap_or_default() {
                let accepts: Vec<&str> =
                    agent["accepts"].as_array().into_iter().flatten().filter_map(|word| word.as_str()).collect();
                println!(
                    "{:<16} {:<14} {:<11} {}",
                    agent["label"].as_str().unwrap_or(""),
                    agent["hook"].as_str().unwrap_or(""),
                    if accepts.is_empty() { "-".to_string() } else { accepts.join(",") },
                    if agent["available"] == json!(true) { "" } else { "(not installed here)" }
                );
            }
            Ok(0)
        }
        Some("hook") => {
            let at = args.iter().position(|word| word == "hook").map_or(0, |at| at + 1);
            Ok(hook::run(address, &args[at..]))
        }
        Some(other) => Err(crate::usage(format!("`{other}` is not a skill command\n{USAGE}"))),
    }
}
