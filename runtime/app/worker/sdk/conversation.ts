// The conversation convention: how anything finds and speaks to a tab's conversation, whichever
// plugin serves it (a chat over a TUI's session file, an ACP client, …). The tab option
// `@agent.conversation` names that plugin (`owner/name`); the plugin provides
//
//   <prefix>conversation.read(tab, since?)    where it stands (`{snapshot, cursor}`), or what
//                                             changed after `since` (`{changes, cursor}`)
//   <prefix>conversation.send(tab, command)   a prompt, a cancel, an answer, …
//
// and fires `<prefix>conversation.changed` `{tab, changes, cursor}` as it moves. The snapshot,
// change, command and cursor are the serving plugin's (a chat plugin keeps them): pumi names
// the functions and the event, and routes them, and knows nothing of what they carry.
// No `import … from "pumi"` here: both halves use it.
import type { TabId } from "./api.types.gen.ts";

/** The tab option naming the plugin (`owner/name`) that serves the tab's conversation. */
export const AGENT_CONVERSATION = "@agent.conversation";

/** What `<prefix>conversation.read` answers: the whole of it, or what followed `since`. */
export type ConversationRead<Snapshot = unknown, Change = unknown, Cursor = unknown> =
  | { snapshot: Snapshot; cursor: Cursor }
  | { changes: Change[]; cursor: Cursor };

/** The data of `<prefix>conversation.changed`. */
export interface ConversationChanged<Change = unknown, Cursor = unknown> {
  /** The tab, by its id (`%3`). */
  tab: TabId;
  changes: Change[];
  cursor: Cursor;
}

/**
 * What a plugin serving conversations provides (with `provide`, under the names below); `send`
 * answers `Sent` (`null` unless the plugin says otherwise).
 */
export interface ConversationProvider<Snapshot = unknown, Change = unknown, Command = unknown, Cursor = unknown, Sent = null> {
  read(tab: TabId, since?: Cursor): ConversationRead<Snapshot, Change, Cursor> | Promise<ConversationRead<Snapshot, Change, Cursor>>;
  send(tab: TabId, command: Command): Sent | Promise<Sent>;
}

/**
 * The functions and the event of the plugin `plugin` (`owner/name`, as `@agent.conversation` names
 * it) under its prefix: `pumi_dev/agent-chat` is `pumi_dev.agent_chat.conversation.read`, … (a
 * plugin's prefix: `/` as `.`, `-` as `_`).
 */
export function conversationNames(plugin: string): { read: string; send: string; changed: string } {
  const prefix = `${plugin.replace("/", ".").replaceAll("-", "_")}.conversation.`;
  return { read: `${prefix}read`, send: `${prefix}send`, changed: `${prefix}changed` };
}
