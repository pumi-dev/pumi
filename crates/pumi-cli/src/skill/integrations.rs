use serde_json::{Value, json};
use std::path::{Path, PathBuf};
pub const MARKER: &str = "pumi-context";
/// The hooks' version: a hook of another is `outdated` in `pumi skill list`.
pub const VERSION: u32 = 8;
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Shape {
    Hooks {
        file: &'static str,
        start: &'static str,
        prompt: Option<&'static str>,
        stop: Option<&'static str>,
        /// The event that runs when the agent would ask the user's permission: with
        /// `agents.permissions` on, the hook asks through `request.open` instead.
        permission: Option<&'static str>,
        /// The tools that wait on the user (a `PreToolUse` matcher): the hook says `blocked` before
        /// them, and `PostToolUse` says `working` again after any tool once the user answered.
        asks: Option<&'static str>,
        ms: bool,
    },
    Cursor {
        file: &'static str,
    },
    PiExtension {
        file: &'static str,
    },
    OpencodePlugin {
        file: &'static str,
    },
    None,
}
/// An agent pumi knows: how its hook is installed, how its session resumes, and what it takes.
#[derive(Debug, Clone, Copy)]
pub struct Agent {
    pub id: &'static str,
    pub label: &'static str,
    pub command: &'static str,
    pub shape: Shape,
    /// When it can be told something unseen: `prompt` (at start and at each prompt), `start`, `none`.
    pub reach: &'static str,
    /// It reports its own state (OSC 7501), so its hook does not.
    pub reports: bool,
    /// The program and arguments that resume a session, `{session}` where its id goes: the
    /// restored tab's `restore_command`. Empty: it does not resume. `agents.restore` overrides.
    pub resume: &'static [&'static str],
    /// What it takes besides hidden text (`@agent.accepts`): `queue` (a message at a turn's end,
    /// through its stop hook or its extension), `wake` (while idle too). Only what was checked.
    pub accepts: &'static [&'static str],
}
pub const AGENTS: &[Agent] = &[
    Agent {
        id: "pi",
        label: "pi",
        command: "pi",
        shape: Shape::PiExtension { file: ".pi/agent/extensions/pumi-context.ts" },
        reach: "prompt",
        reports: true,
        resume: &["pi", "--session", "{session}"],
        accepts: &["queue", "wake"],
    },
    Agent {
        id: "omp",
        label: "omp",
        command: "omp",
        shape: Shape::PiExtension { file: ".omp/agent/extensions/pumi-context.ts" },
        reach: "prompt",
        reports: false,
        resume: &[],
        accepts: &[],
    },
    Agent {
        id: "claude",
        label: "Claude Code",
        command: "claude",
        shape: Shape::Hooks {
            file: ".claude/settings.json",
            start: "SessionStart",
            prompt: Some("UserPromptSubmit"),
            stop: Some("Stop"),
            permission: Some("PermissionRequest"),
            asks: Some("AskUserQuestion|ExitPlanMode"),
            ms: false,
        },
        reach: "prompt",
        reports: false,
        resume: &["claude", "--resume", "{session}"],
        accepts: &["queue"],
    },
    Agent {
        id: "codex",
        label: "Codex",
        command: "codex",
        shape: Shape::Hooks {
            file: ".codex/hooks.json",
            start: "SessionStart",
            prompt: Some("UserPromptSubmit"),
            stop: Some("Stop"),
            permission: None,
            asks: None,
            ms: false,
        },
        reach: "prompt",
        reports: false,
        resume: &["codex", "resume", "{session}"],
        accepts: &[],
    },
    Agent {
        id: "gemini",
        label: "Gemini CLI",
        command: "gemini",
        shape: Shape::Hooks {
            file: ".gemini/settings.json",
            start: "SessionStart",
            prompt: Some("BeforeAgent"),
            stop: Some("AfterAgent"),
            permission: None,
            asks: None,
            ms: true,
        },
        reach: "prompt",
        reports: false,
        resume: &[],
        accepts: &[],
    },
    Agent {
        id: "qwen",
        label: "Qwen Code",
        command: "qwen",
        shape: Shape::Hooks {
            file: ".qwen/settings.json",
            start: "SessionStart",
            prompt: Some("UserPromptSubmit"),
            stop: Some("Stop"),
            permission: None,
            asks: None,
            ms: true,
        },
        reach: "prompt",
        reports: false,
        resume: &[],
        accepts: &[],
    },
    Agent {
        id: "droid",
        label: "Droid",
        command: "droid",
        shape: Shape::Hooks {
            file: ".factory/settings.json",
            start: "SessionStart",
            prompt: Some("UserPromptSubmit"),
            stop: Some("Stop"),
            permission: None,
            asks: None,
            ms: false,
        },
        reach: "prompt",
        reports: false,
        resume: &[],
        accepts: &[],
    },
    Agent {
        id: "cursor",
        label: "Cursor",
        command: "cursor-agent",
        shape: Shape::Cursor { file: ".cursor/hooks.json" },
        reach: "start",
        reports: false,
        resume: &["cursor-agent", "--resume", "{session}"],
        accepts: &[],
    },
    Agent {
        id: "opencode",
        label: "OpenCode",
        command: "opencode",
        shape: Shape::OpencodePlugin { file: ".config/opencode/plugins/pumi-context.js" },
        reach: "prompt",
        reports: false,
        resume: &["opencode", "--session", "{session}"],
        accepts: &[],
    },
    Agent {
        id: "kilo",
        label: "Kilo",
        command: "kilo",
        shape: Shape::OpencodePlugin { file: ".config/kilo/plugins/pumi-context.js" },
        reach: "prompt",
        reports: false,
        resume: &[],
        accepts: &[],
    },
    Agent {
        id: "copilot",
        label: "GitHub Copilot",
        command: "copilot",
        shape: Shape::None,
        reach: "none",
        reports: false,
        resume: &[],
        accepts: &[],
    },
    Agent {
        id: "kiro",
        label: "Kiro CLI",
        command: "kiro-cli",
        shape: Shape::None,
        reach: "none",
        reports: false,
        resume: &[],
        accepts: &[],
    },
    Agent {
        id: "hermes",
        label: "Hermes",
        command: "hermes",
        shape: Shape::None,
        reach: "none",
        reports: false,
        resume: &[],
        accepts: &[],
    },
    Agent {
        id: "amp",
        label: "Amp",
        command: "amp",
        shape: Shape::None,
        reach: "none",
        reports: false,
        resume: &[],
        accepts: &[],
    },
];
pub fn find(id: &str) -> Option<&'static Agent> {
    AGENTS.iter().find(|a| a.id == id)
}
/// The hook's line for `agent` at `event`, in `format` (`hooks`, `cursor`): a no-op outside pumi.
pub fn command(agent: &str, event: &str, format: &str) -> String {
    format!(
        "[ -n \"$PUMI_TAB\" ] && \"${{PUMI_BIN:-pumi}}\" skill hook {agent} {event} --format {format} || true # {MARKER} v{VERSION}"
    )
}
/// The line that resumes `agent`'s `session`: its recipe, or the one `overrides` (`agents.restore`)
/// gives it; `None` when it has none, or the session is not one word pumi will type.
pub fn resume_line(agent: &str, session: &str, overrides: &Value) -> Option<String> {
    let words = |value: &Value| -> Option<Vec<String>> {
        value.as_array()?.iter().map(|w| w.as_str().map(str::to_string)).collect()
    };
    let recipe = match overrides.get(agent) {
        Some(over) => words(over)?,
        None => find(agent)?.resume.iter().map(|w| w.to_string()).collect(),
    };
    resume_command(&recipe, session)
}
/// `recipe` with `session` in it, each word quoted for a shell.
pub fn resume_command(recipe: &[String], session: &str) -> Option<String> {
    let session = session.trim();
    let bad = session.is_empty() || session.starts_with('-') || session.chars().any(char::is_control);
    if bad || !recipe.iter().any(|w| w.contains(SESSION)) {
        return None;
    }
    let line: Vec<String> = recipe.iter().map(|w| quote(&w.replace(SESSION, session))).collect();
    Some(line.join(" "))
}
/// Where a recipe's session goes.
pub const SESSION: &str = "{session}";
fn quote(word: &str) -> String {
    if !word.is_empty() && word.chars().all(|c| c.is_ascii_alphanumeric() || "-_./=:,+@%".contains(c)) {
        word.to_string()
    } else {
        format!("'{}'", word.replace('\'', "'\\''"))
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum State {
    Installed,
    Outdated,
    NotInstalled,
    Unsupported,
}
impl State {
    pub fn as_str(self) -> &'static str {
        match self {
            State::Installed => "installed",
            State::Outdated => "outdated",
            State::NotInstalled => "not_installed",
            State::Unsupported => "unsupported",
        }
    }
}
fn home() -> PathBuf {
    pumi_proto::paths::home_dir()
}
pub fn on_path(command: &str) -> bool {
    std::env::var_os("PATH").is_some_and(|path| std::env::split_paths(&path).any(|dir| dir.join(command).is_file()))
}
pub fn file(agent: &Agent) -> Option<PathBuf> {
    match agent.shape {
        Shape::Hooks { file, .. }
        | Shape::Cursor { file }
        | Shape::PiExtension { file }
        | Shape::OpencodePlugin { file } => Some(home().join(file)),
        Shape::None => None,
    }
}
fn read_json(path: &Path) -> Result<Value, String> {
    match std::fs::read_to_string(path) {
        Ok(text) if text.trim().is_empty() => Ok(json!({})),
        Ok(text) => {
            serde_json::from_str(&text).map_err(|err| format!("{} is not JSON pumi can edit: {err}", path.display()))
        }
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(json!({})),
        Err(err) => Err(format!("{}: {err}", path.display())),
    }
}
fn write(path: &Path, text: &str) -> Result<(), String> {
    let fail = |err: std::io::Error| format!("{}: {err}", path.display());
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(fail)?;
    }
    let backup = PathBuf::from(format!("{}.pumi-bak", path.display()));
    if path.exists() && !backup.exists() {
        std::fs::copy(path, &backup).map_err(fail)?;
    }
    let tmp = PathBuf::from(format!("{}.pumi-tmp", path.display()));
    std::fs::write(&tmp, text).map_err(fail)?;
    std::fs::rename(&tmp, path).map_err(fail)
}
fn is_ours(hook: &Value) -> bool {
    hook["command"].as_str().is_some_and(|c| c.contains(MARKER))
}
pub fn strip(config: &mut Value, nested: bool) {
    let Some(hooks) = config.get_mut("hooks").and_then(Value::as_object_mut) else { return };
    for entries in hooks.values_mut() {
        let Some(list) = entries.as_array_mut() else { continue };
        if nested {
            for group in list.iter_mut() {
                if let Some(inner) = group.get_mut("hooks").and_then(Value::as_array_mut) {
                    inner.retain(|h| !is_ours(h));
                }
            }
            list.retain(|g| g["hooks"].as_array().is_none_or(|h| !h.is_empty()));
        } else {
            list.retain(|h| !is_ours(h));
        }
    }
    hooks.retain(|_, entries| entries.as_array().is_none_or(|l| !l.is_empty()));
}
pub fn merge(config: &mut Value, agent: &Agent) {
    if !config.is_object() {
        *config = json!({});
    }
    match agent.shape {
        Shape::Hooks { start, prompt, stop, permission, asks, ms, .. } => {
            strip(config, true);
            let unit = if ms { 1000 } else { 1 };
            let root = config.as_object_mut().expect("an object");
            let hooks = root.entry("hooks").or_insert_with(|| json!({}));
            if !hooks.is_object() {
                *hooks = json!({});
            }
            let hooks = hooks.as_object_mut().expect("an object");
            for (event, which) in
                [(Some(start), "start"), (prompt, "prompt"), (stop, "stop"), (permission, "permission")]
            {
                let Some(event) = event else { continue };
                // A permission waits for the user's answer, and gives up on its own first.
                let timeout = unit * if which == "permission" { super::PERMISSION_WAIT + 30 } else { 5 };
                let list = hooks.entry(event).or_insert_with(|| json!([]));
                if !list.is_array() {
                    *list = json!([]);
                }
                list.as_array_mut().expect("an array").push(json!({
                    "hooks": [{ "type": "command", "command": command(agent.id, which, "hooks"), "timeout": timeout }]
                }));
            }
            // A question (or a plan to approve) waits on the user: `blocked` before it, and
            // `working` again after a tool ran (it was answered, or a permission was given).
            if let Some(tools) = asks {
                for (event, matcher, which) in [
                    ("PreToolUse", tools, "question"),
                    ("PostToolUse", "", "resume"),
                    ("PostToolUseFailure", "", "resume"),
                ] {
                    let list = hooks.entry(event).or_insert_with(|| json!([]));
                    if !list.is_array() {
                        *list = json!([]);
                    }
                    let mut group = json!({
                        "hooks": [{ "type": "command", "command": command(agent.id, which, "hooks"), "timeout": unit * 5 }]
                    });
                    if !matcher.is_empty() {
                        group["matcher"] = json!(matcher);
                    }
                    list.as_array_mut().expect("an array").push(group);
                }
            }
        }
        Shape::Cursor { .. } => {
            strip(config, false);
            let root = config.as_object_mut().expect("an object");
            root.entry("version").or_insert(json!(1));
            let hooks = root.entry("hooks").or_insert_with(|| json!({}));
            if !hooks.is_object() {
                *hooks = json!({});
            }
            let list = hooks.as_object_mut().expect("an object").entry("sessionStart").or_insert_with(|| json!([]));
            if !list.is_array() {
                *list = json!([]);
            }
            list.as_array_mut()
                .expect("an array")
                .push(json!({ "command": command(agent.id, "start", "cursor"), "timeout": 5 }));
        }
        Shape::PiExtension { .. } | Shape::OpencodePlugin { .. } | Shape::None => {}
    }
}
pub fn owned_file(agent: &Agent) -> Option<String> {
    let header = format!(
        "// installed by pumi (Settings → Agents); reinstalling overwrites this file.\n// {MARKER} v{VERSION}\n"
    );
    match agent.shape {
        Shape::PiExtension { .. } => {
            let queue = if agent.accepts.contains(&"wake") { PI_QUEUE } else { PI_NO_QUEUE };
            Some(format!(
                "{header}{}",
                PI_EXTENSION.replace("{queue}", queue).replace("{live}", PI_LIVE).replace("{agent}", agent.id)
            ))
        }
        Shape::OpencodePlugin { .. } => Some(format!("{header}{}", OPENCODE_PLUGIN.replace("{agent}", agent.id))),
        _ => None,
    }
}
pub fn state(agent: &Agent) -> State {
    let Some(path) = file(agent) else { return State::Unsupported };
    let Ok(text) = std::fs::read_to_string(&path) else { return State::NotInstalled };
    if !text.contains(MARKER) {
        return State::NotInstalled;
    }
    let current = match owned_file(agent) {
        Some(file) => text == file,
        None => {
            let format = if matches!(agent.shape, Shape::Cursor { .. }) { "cursor" } else { "hooks" };
            serde_json::from_str::<Value>(&text).is_ok_and(|v| has_string(&v, &command(agent.id, "start", format)))
        }
    };
    if current { State::Installed } else { State::Outdated }
}
fn has_string(value: &Value, wanted: &str) -> bool {
    match value {
        Value::String(s) => s == wanted,
        Value::Array(items) => items.iter().any(|v| has_string(v, wanted)),
        Value::Object(map) => map.values().any(|v| has_string(v, wanted)),
        _ => false,
    }
}
pub fn install(agent: &Agent) -> Result<PathBuf, String> {
    let path = file(agent).ok_or_else(|| {
        format!(
            "{} has no hook pumi can install; `pumi skill hook {} prompt --full` still says what it would be told",
            agent.label, agent.id
        )
    })?;
    let text = match owned_file(agent) {
        Some(text) => text,
        None => {
            let mut config = read_json(&path)?;
            merge(&mut config, agent);
            serde_json::to_string_pretty(&config).map_err(|e| e.to_string())? + "\n"
        }
    };
    write(&path, &text)?;
    Ok(path)
}
pub fn uninstall(agent: &Agent) -> Result<(), String> {
    let Some(path) = file(agent) else { return Ok(()) };
    if !path.exists() {
        return Ok(());
    }
    if owned_file(agent).is_some() {
        let text = std::fs::read_to_string(&path).unwrap_or_default();
        if text.contains(MARKER) {
            std::fs::remove_file(&path).map_err(|e| format!("{}: {e}", path.display()))?;
        }
        return Ok(());
    }
    let mut config = read_json(&path)?;
    let before = config.clone();
    strip(&mut config, matches!(agent.shape, Shape::Hooks { .. }));
    if config != before {
        write(&path, &(serde_json::to_string_pretty(&config).map_err(|e| e.to_string())? + "\n"))?;
    }
    Ok(())
}
/// Every agent, whether it is here, and whether its hook is in: what `pumi skill list` shows.
/// An agent is told things while its hook is in (`on`).
pub fn report() -> Value {
    json!({
        "agents": AGENTS.iter().map(|a| {
            let hook = state(a);
            json!({
                "id": a.id,
                "label": a.label,
                "available": on_path(a.command),
                "hook": hook.as_str(),
                "reach": a.reach,
                "accepts": a.accepts,
                "resumes": !a.resume.is_empty(),
                "on": matches!(hook, State::Installed | State::Outdated),
                "file": file(a).map(|p| pumi_proto::paths::display(&p)),
            })
        }).collect::<Vec<_>>(),
    })
}
const PI_EXTENSION: &str = r#"import { execFile, spawn } from "node:child_process";
import { renameSync, unlinkSync, writeFileSync } from "node:fs";

const bin = process.env.PUMI_BIN || "pumi";

function pumi(args: string[]): Promise<string> {
  return new Promise((resolve) => {
    const child = execFile(bin, args, { timeout: 4000, maxBuffer: 1 << 20 }, (err, stdout) =>
      resolve(err ? "" : String(stdout)),
    );
    child.stdin?.end();
  });
}

export default function (pi: any) {
  if (!process.env.PUMI_TAB) return;
  const sessionOf = (ctx: any) => {
    try {
      return String(ctx?.sessionManager?.getSessionId?.() ?? "");
    } catch {
      return "";
    }
  };
  // The session's file and the model, for `@agent.transcript` and `@agent.model`.
  const about = (ctx: any) => {
    const args: string[] = [];
    try {
      const file = ctx?.sessionManager?.getSessionFile?.();
      if (typeof file === "string" && file.startsWith("/")) args.push("--transcript", file);
      const model = ctx?.model;
      if (model?.provider && model?.id) args.push("--model", `${model.provider}/${model.id}`);
    } catch {}
    return args;
  };
  const hook = (event: string, ctx: any, ...more: string[]) =>
    pumi(["skill", "hook", "{agent}", event, "--session", sessionOf(ctx), ...about(ctx), ...more]);
  // What pumi has for the agent (`agent.turn`), handed to it unseen: at a session's start with the
  // next turn, and before each prompt.
  const hidden = (content: string) => ({ customType: "pumi-context", content, display: false });
  pi.on("session_start", async (event: any, ctx: any) => {
    // What a session that ended mid-turn left of its live file no longer holds.
    live.at(ctx);
    live.clear();
    if (ctx?.mode !== "tui") return;
    const resumed = ["resume", "reload", "fork"].includes(event?.reason) ? ["--resumed"] : [];
    const text = (await hook("start", ctx, ...resumed, "--format", "text")).trim();
    if (text) {
      try {
        pi.sendMessage(hidden(text), { deliverAs: "nextTurn" });
      } catch {}
    }
    queue.idle(ctx);
  });
  pi.on("session_compact", async (_event: any, ctx: any) => {
    if (sessionOf(ctx)) await hook("compact", ctx);
  });
  pi.on("before_agent_start", async (event: any, ctx: any) => {
    queue.busy();
    if (ctx?.mode !== "tui" || event?.messageType !== "user") return;
    const text = (await hook("prompt", ctx, "--format", "text")).trim();
    if (!text) return;
    return { message: hidden(text) };
  });
  pi.on("agent_end", async (_event: any, ctx: any) => {
    if (ctx?.mode !== "tui") return;
    // The turn is over: what is queued for it (`pumi agent queue`) is the next message.
    const next = (await hook("stop", ctx, "--format", "text")).trim();
    if (next) queue.send(next);
    else queue.idle(ctx);
  });
  pi.on("session_shutdown", async () => {
    queue.busy();
    live.clear();
  });
{queue}
{live}
}
"#;
/// What pi says and runs before its session file has it, for a chat to show as it comes: the
/// assistant message being streamed (its text and thinking) and each running tool's output so far.
/// It is the whole of it each time, at most every 100 ms, in `<session file>.live` (JSON, `v: 1`),
/// which goes when the turn ends. pi lists only `.jsonl` files, so it never takes it for a session.
const PI_LIVE: &str = r#"  const live = (() => {
    const most = 64 * 1024;
    let file = "";
    let message: any = null;
    let tools: Record<string, { name: string; output: string }> = {};
    let timer: ReturnType<typeof setTimeout> | undefined;
    const flush = () => {
      timer = undefined;
      if (!file) return;
      try {
        writeFileSync(`${file}.tmp`, JSON.stringify({ v: 1, message, tools }));
        renameSync(`${file}.tmp`, file);
      } catch {}
    };
    const later = () => {
      timer ??= setTimeout(flush, 100);
    };
    const textOf = (content: any) =>
      Array.isArray(content) ? content.map((block: any) => (block?.type === "text" && typeof block.text === "string" ? block.text : "")).join("") : "";
    return {
      at(ctx: any) {
        let next = "";
        try {
          const path = ctx?.sessionManager?.getSessionFile?.();
          if (typeof path === "string" && path.startsWith("/")) next = `${path}.live`;
        } catch {}
        if (next !== file) this.clear();
        file = next;
      },
      message(value: any) {
        if (value?.role !== "assistant") return;
        const content = (Array.isArray(value.content) ? value.content : []).flatMap((block: any) =>
          block?.type === "text" ? [{ type: "text", text: String(block.text ?? "") }]
          : block?.type === "thinking" ? [{ type: "thinking", text: String(block.thinking ?? "") }]
          : [],
        );
        message = { timestamp: value.timestamp, content };
        later();
      },
      ended() {
        message = null;
        later();
      },
      tool(id: string, name: string, output: string | undefined) {
        if (output === undefined) delete tools[id];
        else tools[id] = { name, output: output.length > most ? output.slice(-most) : output };
        later();
      },
      textOf,
      clear() {
        if (timer) clearTimeout(timer);
        timer = undefined;
        message = null;
        tools = {};
        if (file) {
          try {
            unlinkSync(file);
          } catch {}
        }
      },
    };
  })();
  pi.on("message_update", async (event: any, ctx: any) => {
    live.at(ctx);
    live.message(event?.message);
  });
  pi.on("message_end", async (event: any) => {
    if (event?.message?.role === "assistant") live.ended();
  });
  pi.on("tool_execution_start", async (event: any, ctx: any) => {
    if (event?.parentToolCallId || !event?.toolCallId) return;
    live.at(ctx);
    live.tool(event.toolCallId, String(event.toolName ?? ""), "");
  });
  pi.on("tool_execution_update", async (event: any) => {
    if (event?.parentToolCallId || !event?.toolCallId) return;
    live.tool(event.toolCallId, String(event.toolName ?? ""), live.textOf(event.partialResult?.content));
  });
  pi.on("tool_execution_end", async (event: any) => {
    if (event?.parentToolCallId || !event?.toolCallId) return;
    live.tool(event.toolCallId, "", undefined);
  });
  pi.on("agent_end", async () => live.clear());"#;
/// pi takes queued messages while idle: a `pumi agent take %3 --wait` child waits for one, and goes
/// while pi is busy. An item is sent as the user's message, which leaves the editor alone.
const PI_QUEUE: &str = r#"  const queue = (() => {
    let waiting: ReturnType<typeof spawn> | undefined;
    let busy = false;
    let held: string | undefined;
    let backoff = 1000;
    const stop = () => {
      const child = waiting;
      waiting = undefined;
      child?.kill();
    };
    const send = (text: string) => {
      if (busy) {
        held = text;
        return;
      }
      try {
        pi.sendUserMessage(text, { deliverAs: "followUp" });
      } catch {}
    };
    const idle = (ctx: any) => {
      busy = false;
      if (held !== undefined) {
        const text = held;
        held = undefined;
        return send(text);
      }
      const session = sessionOf(ctx);
      if (waiting || ctx?.mode !== "tui" || !session) return;
      let out = "";
      const tab = String(process.env.PUMI_TAB);
      const child = spawn(bin, ["agent", "take", tab, "--session", session, "--wait", "--json"], { stdio: ["ignore", "pipe", "ignore"] });
      waiting = child;
      child.stdout?.on("data", (data) => (out += data));
      child.on("close", () => {
        const mine = waiting === child;
        if (mine) waiting = undefined;
        let item: any = null;
        try {
          item = JSON.parse(out);
        } catch {}
        if (typeof item?.text === "string" && item.text) {
          backoff = 1000;
          return send(item.text);
        }
        // Nothing came (the session changed, the server went): wait again, more slowly.
        if (mine && !busy) setTimeout(() => !busy && idle(ctx), backoff);
        backoff = Math.min(backoff * 2, 30000);
      });
    };
    return {
      idle,
      send,
      busy() {
        busy = true;
        stop();
      },
    };
  })();"#;
const PI_NO_QUEUE: &str = r#"  const queue = { idle(_ctx: any) {}, send(_text: string) {}, busy() {} };"#;
const OPENCODE_PLUGIN: &str = r#"import { execFile } from "node:child_process";

function pumi(args) {
  return new Promise((resolve) => {
    const child = execFile(process.env.PUMI_BIN || "pumi", args, { timeout: 4000, maxBuffer: 1 << 20 }, (err, stdout) =>
      resolve(err ? "" : String(stdout)),
    );
    child.stdin?.end();
  });
}

const known = new Map();

export const PumiContextPlugin = async () => {
  if (!process.env.PUMI_TAB) return {};
  return {
    // What pumi has for the agent, in its system prompt from each prompt on: everything, each time.
    "chat.message": async ({ sessionID }) => {
      if (!sessionID) return;
      const text = (await pumi(["skill", "hook", "{agent}", "prompt", "--session", sessionID, "--full", "--format", "text"])).trim();
      if (text) known.set(sessionID, text);
      else known.delete(sessionID);
    },
    // The turn is over: the agent is done (OSC 7501, which opencode does not say itself).
    event: async ({ event }) => {
      if (event?.type === "session.idle") {
        await pumi(["skill", "hook", "{agent}", "stop", "--session", event.properties?.sessionID ?? ""]);
      } else if (event?.type === "session.compacted") {
        await pumi(["skill", "hook", "{agent}", "compact", "--session", event.properties?.sessionID ?? ""]);
      }
    },
    "experimental.chat.system.transform": async ({ sessionID }, output) => {
      const text = sessionID && known.get(sessionID);
      if (text && Array.isArray(output?.system)) output.system.push(text);
    },
  };
};

export default { id: "pumi.context", server: PumiContextPlugin, setup() {} };
"#;
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn claude_hooks_merge_beside_the_users_and_come_out_cleanly() {
        let user = json!({
            "model": "opus",
            "hooks": { "SessionStart": [{ "matcher": "startup", "hooks": [{ "type": "command", "command": "mine.sh" }] }] }
        });
        let claude = find("claude").unwrap();
        let mut config = user.clone();
        merge(&mut config, claude);
        merge(&mut config, claude);
        let start = config["hooks"]["SessionStart"].as_array().unwrap();
        assert_eq!(start.len(), 2);
        assert_eq!(start[1]["hooks"][0]["command"], command("claude", "start", "hooks"));
        assert_eq!(start[1]["hooks"][0]["timeout"], 5);
        assert_eq!(config["hooks"]["UserPromptSubmit"][0]["hooks"][0]["command"], command("claude", "prompt", "hooks"));
        assert_eq!(config["model"], "opus");
        strip(&mut config, true);
        assert_eq!(config, user);
    }
    #[test]
    fn claude_asks_permissions_through_pumi_and_waits_longer_for_them() {
        let mut config = json!({});
        merge(&mut config, find("claude").unwrap());
        let hook = &config["hooks"]["PermissionRequest"][0]["hooks"][0];
        assert_eq!(hook["command"], command("claude", "permission", "hooks"));
        assert_eq!(hook["timeout"], json!(super::super::PERMISSION_WAIT + 30));
        let mut codex = json!({});
        merge(&mut codex, find("codex").unwrap());
        assert!(codex["hooks"].get("PermissionRequest").is_none());
    }
    #[test]
    fn claude_says_blocked_before_a_question_and_working_after_a_tool() {
        let mut config = json!({});
        merge(&mut config, find("claude").unwrap());
        let pre = &config["hooks"]["PreToolUse"][0];
        assert_eq!(pre["matcher"], "AskUserQuestion|ExitPlanMode");
        assert_eq!(pre["hooks"][0]["command"], command("claude", "question", "hooks"));
        for event in ["PostToolUse", "PostToolUseFailure"] {
            let post = &config["hooks"][event][0];
            assert!(post.get("matcher").is_none(), "after any tool: {event}");
            assert_eq!(post["hooks"][0]["command"], command("claude", "resume", "hooks"));
        }
        strip(&mut config, true);
        assert_eq!(config, json!({ "hooks": {} }));
        let mut codex = json!({});
        merge(&mut codex, find("codex").unwrap());
        assert!(codex["hooks"].get("PreToolUse").is_none());
    }
    #[test]
    fn gemini_uses_before_agent_and_milliseconds() {
        let mut config = json!({});
        merge(&mut config, find("gemini").unwrap());
        assert_eq!(config["hooks"]["BeforeAgent"][0]["hooks"][0]["timeout"], 5000);
        assert_eq!(config["hooks"]["AfterAgent"][0]["hooks"][0]["command"], command("gemini", "stop", "hooks"));
    }
    #[test]
    fn a_turn_ending_says_done_unless_the_agent_says_it_itself() {
        let mut config = json!({});
        merge(&mut config, find("claude").unwrap());
        assert_eq!(config["hooks"]["Stop"][0]["hooks"][0]["command"], command("claude", "stop", "hooks"));
        strip(&mut config, true);
        assert_eq!(config, json!({ "hooks": {} }), "nothing of pumi's left");
        let pi = owned_file(find("pi").unwrap()).unwrap();
        let omp = owned_file(find("omp").unwrap()).unwrap();
        assert!(pi.contains("\"agent\", \"take\"") && pi.contains("sendUserMessage"), "pi takes queued messages");
        assert!(!omp.contains("\"agent\", \"take\"") && !omp.contains("sendUserMessage"), "omp is not checked: {omp}");
        for file in [&pi, &omp] {
            assert!(!file.contains("{agent}") && !file.contains("{queue}") && file.contains("\"compact\""), "{file}");
        }
        assert!(omp.contains("\"skill\", \"hook\", \"omp\", event"), "{omp}");
        let opencode = owned_file(find("opencode").unwrap()).unwrap();
        assert!(opencode.contains("session.idle") && !opencode.contains("{agent}"));
    }
    #[test]
    fn cursor_gets_session_start_only() {
        let mut config = json!({ "version": 1, "hooks": { "stop": [{ "command": "x" }] } });
        merge(&mut config, find("cursor").unwrap());
        assert_eq!(config["hooks"]["sessionStart"][0]["command"], command("cursor", "start", "cursor"));
        assert!(config["hooks"].get("beforeSubmitPrompt").is_none());
        strip(&mut config, false);
        assert_eq!(config, json!({ "version": 1, "hooks": { "stop": [{ "command": "x" }] } }));
    }
    #[test]
    fn a_session_resumes_by_its_recipe_or_the_users() {
        for agent in AGENTS.iter().filter(|a| !a.resume.is_empty()) {
            assert!(resume_line(agent.id, "abc", &Value::Null).is_some(), "{}", agent.id);
        }
        assert_eq!(resume_line("claude", "abc-123", &Value::Null).unwrap(), "claude --resume abc-123");
        assert_eq!(resume_line("claude", "a b; rm -rf ~", &Value::Null).unwrap(), "claude --resume 'a b; rm -rf ~'");
        assert_eq!(resume_line("claude", "--dangerously-skip-permissions", &Value::Null), None);
        assert_eq!(resume_line("claude", "a\nb", &Value::Null), None);
        let overrides = json!({ "claude": ["claude", "--resume", "{session}", "--model", "opus"], "codex": [], "aider": ["aider", "--restore", "{session}"] });
        assert_eq!(resume_line("claude", "x", &overrides).unwrap(), "claude --resume x --model opus");
        assert_eq!(resume_line("codex", "x", &overrides), None, "an empty recipe turns it off");
        assert_eq!(resume_line("aider", "x", &overrides).unwrap(), "aider --restore x", "an agent pumi does not know");
        assert_eq!(resume_line("codex", "x", &Value::Null).unwrap(), "codex resume x");
        assert_eq!(resume_command(&["claude".into()], "abc"), None);
    }
    #[test]
    fn only_checked_agents_accept_a_queue() {
        let accepting: Vec<&str> = AGENTS.iter().filter(|a| a.accepts.contains(&"queue")).map(|a| a.id).collect();
        assert_eq!(accepting, ["pi", "claude"]);
        for agent in AGENTS {
            assert!(agent.accepts.iter().all(|w| ["queue", "wake"].contains(w)), "{}", agent.id);
        }
    }
    #[test]
    fn the_command_is_a_no_op_outside_pumi() {
        // The hook line is run by a POSIX shell, which a Windows runner does not have at `/bin/sh`.
        if cfg!(windows) {
            return;
        }
        let out = std::process::Command::new("/bin/sh")
            .arg("-c")
            .arg(command("claude", "start", "hooks"))
            .env_remove("PUMI_TAB")
            .output()
            .unwrap();
        assert!(out.status.success());
        assert!(out.stdout.is_empty() && out.stderr.is_empty());
    }
}
