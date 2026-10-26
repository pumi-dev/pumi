//! `pumi completion zsh|bash|fish`: what a shell completes for `pumi`, from the commands the CLI
//! answers — the ones it answers itself (`crate::OWN`), and every function as the server describes
//! it (`crate::nouns::table`).
use crate::client::Address;
use crate::nouns::{self, Command};
use pumi_api::Error;

/// A word a shell completes, and what it does.
struct Word {
    word: String,
    desc: String,
}

/// `pumi completion zsh|bash|fish`, with the server's commands when there is a server.
pub fn main(address: &Address, args: &[String]) -> Result<i32, Error> {
    let Some(shell) = args.first().map(String::as_str) else {
        return Err(crate::usage("which shell? (`pumi completion zsh|bash|fish`)"));
    };
    let commands = served(address);
    print!("{}", generate(shell, &commands)?);
    Ok(0)
}

/// The server's commands, when one answers: a completion is written once, and starting a server to
/// write one would be a surprise.
fn served(address: &Address) -> Vec<Command> {
    let Ok(mut client) = crate::client::Client::open(address, false) else { return Vec::new() };
    if client.introduce("cli").is_err() {
        return Vec::new();
    }
    nouns::table(&mut client).unwrap_or_default()
}

fn generate(shell: &str, commands: &[Command]) -> Result<String, Error> {
    match shell {
        "zsh" => Ok(zsh(commands)),
        "bash" => Ok(bash(commands)),
        "fish" => Ok(fish(commands)),
        other => Err(crate::usage(format!("a shell is zsh, bash or fish, not `{other}`"))),
    }
}

/// The first word of every command, `pumi`'s verbs, each with one line about it.
fn nouns(commands: &[Command]) -> Vec<Word> {
    let mut out: Vec<Word> =
        crate::OWN.iter().map(|(noun, _, desc)| Word { word: noun.to_string(), desc: desc.to_string() }).collect();
    for command in commands {
        let Some(noun) = command.words.first() else { continue };
        if out.iter().any(|word| word.word == *noun) {
            continue;
        }
        out.push(Word { word: noun.clone(), desc: command.desc.clone() });
    }
    out
}

/// A noun's verbs: the commands whose first word it is.
fn verbs(commands: &[Command], noun: &str) -> Vec<Word> {
    let mut out: Vec<Word> = Vec::new();
    for (own, own_verbs, desc) in crate::OWN {
        if *own != noun {
            continue;
        }
        for verb in *own_verbs {
            out.push(Word { word: verb.to_string(), desc: desc.to_string() });
        }
    }
    for command in commands {
        if command.words.first().map(String::as_str) != Some(noun) {
            continue;
        }
        let Some(verb) = command.words.get(1) else { continue };
        if !out.iter().any(|word| word.word == *verb) {
            out.push(Word { word: verb.clone(), desc: command.desc.clone() });
        }
    }
    out
}

/// The commands under a noun, by their second word.
fn under<'a>(commands: &'a [Command], noun: &str) -> impl Iterator<Item = &'a Command> {
    let noun = noun.to_string();
    commands.iter().filter(move |command| command.words.first() == Some(&noun))
}

/// The flags of a command, in a shell's own syntax.
fn flags(command: &Command) -> Vec<Word> {
    let mut out = Vec::new();
    for flag in &command.flags {
        let kebab = flag.kebab();
        out.push(Word { word: format!("--{kebab}"), desc: flag.desc.clone() });
        if flag.boolean {
            out.push(Word { word: format!("--no-{kebab}"), desc: format!("no {}", flag.desc) });
        }
    }
    out
}

/// A description a shell's completion can carry: one line, short, and no `'`, `[`, `]` or `:`.
fn desc(text: &str) -> String {
    let one: String = text
        .lines()
        .next()
        .unwrap_or("")
        .chars()
        .map(|c| if matches!(c, '\'' | '[' | ']' | ':' | '\\' | '(' | ')') { ' ' } else { c })
        .collect();
    let one = one.split_whitespace().collect::<Vec<_>>().join(" ");
    if one.chars().count() <= 70 { one } else { one.chars().take(69).collect::<String>() + "…" }
}

/// The words as zsh's `_describe` takes them: `'word:description'`.
fn described(words: &[Word]) -> String {
    words.iter().map(|word| format!("'{}:{}'", word.word, desc(&word.desc))).collect::<Vec<_>>().join(" ")
}

fn zsh(commands: &[Command]) -> String {
    let mut out = String::from(
        "#compdef pumi\n# What `pumi completion zsh` writes: `pumi`'s commands.\n_pumi() {\n  local -a words\n  if (( CURRENT == 2 )); then\n",
    );
    out.push_str(&format!("    words=({})\n", described(&nouns(commands))));
    out.push_str("    _describe -t commands 'pumi' words\n    return\n  fi\n  case \"${words[2]}\" in\n");
    for noun in nouns(commands) {
        let verbs = verbs(commands, &noun.word);
        if verbs.is_empty() {
            // `pumi open PATH`: its arguments are its own words.
            out.push_str(&format!("    {})\n      _files\n      ;;\n", noun.word));
            continue;
        }
        out.push_str(&format!("    {})\n", noun.word));
        out.push_str(&format!(
            "      if (( CURRENT == 3 )); then\n        words=({})\n        _describe -t commands '{}' words\n        return\n      fi\n",
            described(&verbs),
            noun.word
        ));
        out.push_str("      case \"${words[3]}\" in\n");
        for command in under(commands, &noun.word) {
            let Some(verb) = command.words.get(1) else { continue };
            let flags = flags(command);
            if flags.is_empty() {
                continue;
            }
            let specs: Vec<String> =
                flags.iter().map(|flag| format!("'{}[{}]'", flag.word, desc(&flag.desc))).collect();
            out.push_str(&format!("        {verb}) _arguments {} ;;\n", specs.join(" ")));
        }
        out.push_str("      esac\n      ;;\n");
    }
    out.push_str("  esac\n}\ncompdef _pumi pumi\n");
    out
}

fn bash(commands: &[Command]) -> String {
    let words = |words: &[Word]| words.iter().map(|word| word.word.clone()).collect::<Vec<_>>().join(" ");
    let mut out = String::from(
        "# What `pumi completion bash` writes: `pumi`'s commands.\n_pumi() {\n  local cur first second\n  cur=\"${COMP_WORDS[COMP_CWORD]}\"\n  first=\"${COMP_WORDS[1]}\"\n  second=\"${COMP_WORDS[2]}\"\n  case \"$COMP_CWORD\" in\n    1)\n",
    );
    out.push_str(&format!("      COMPREPLY=( $(compgen -W \"{}\" -- \"$cur\") )\n      ;;\n", words(&nouns(commands))));
    out.push_str("    2)\n      case \"$first\" in\n");
    for noun in nouns(commands) {
        let verbs = verbs(commands, &noun.word);
        if verbs.is_empty() {
            out.push_str(&format!("        {}) COMPREPLY=( $(compgen -f -- \"$cur\") ) ;;\n", noun.word));
        } else {
            out.push_str(&format!(
                "        {}) COMPREPLY=( $(compgen -W \"{}\" -- \"$cur\") ) ;;\n",
                noun.word,
                words(&verbs)
            ));
        }
    }
    out.push_str("      esac\n      ;;\n    *)\n      case \"$first:$second\" in\n");
    for command in commands {
        let (Some(noun), Some(verb)) = (command.words.first(), command.words.get(1)) else { continue };
        let flags = flags(command);
        if flags.is_empty() {
            continue;
        }
        out.push_str(&format!(
            "        {noun}:{verb}) COMPREPLY=( $(compgen -W \"{}\" -- \"$cur\") ) ;;\n",
            words(&flags)
        ));
    }
    out.push_str("      esac\n      ;;\n  esac\n}\ncomplete -o default -F _pumi pumi\n");
    out
}

fn fish(commands: &[Command]) -> String {
    let mut out = String::from("# What `pumi completion fish` writes: `pumi`'s commands.\ncomplete -c pumi -f\n");
    for noun in nouns(commands) {
        out.push_str(&format!(
            "complete -c pumi -n \"__fish_use_subcommand\" -a {} -d \"{}\"\n",
            noun.word,
            desc(&noun.desc).replace('"', "")
        ));
        let verbs = verbs(commands, &noun.word);
        for verb in &verbs {
            out.push_str(&format!(
                "complete -c pumi -n \"__fish_seen_subcommand_from {}\" -a {} -d \"{}\"\n",
                noun.word,
                verb.word,
                desc(&verb.desc).replace('"', "")
            ));
        }
    }
    for command in commands {
        let Some(verb) = command.words.get(1) else { continue };
        let noun = &command.words[0];
        for flag in flags(command) {
            let long = flag.word.trim_start_matches("--");
            out.push_str(&format!(
                "complete -c pumi -n \"__fish_seen_subcommand_from {noun} {verb}\" -l {long} -d \"{}\"\n",
                desc(&flag.desc).replace('"', "")
            ));
        }
    }
    out
}
