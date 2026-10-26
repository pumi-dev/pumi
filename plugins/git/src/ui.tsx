// What the git page draws, in the window's React tree: two tabs, Changes and History. Changes is a
// toolbar (the whole diff, what it adds up to, the filter, Stage All), the lines of the three groups
// — conflicts, staged, unstaged — as path trees with a checkbox each, the branch and its remote, and
// the commit box. History is the commits, newest first.
//
// Nothing here waits on git: the worker posts the whole page each time it changes (`useChannel`),
// the last post is drawn the moment the sidebar opens, the lines are a windowed `<virtual-list>`
// whose unchanged rows are not drawn again, and the commit box keeps its own text. The repository,
// the lines and every action are the worker's (`main.ts`, `panel.ts`, `tree.ts`).
import { memo, useEffect, useMemo, useRef, useState } from "react";
import { Icon, useChannel, useMetrics, usePlugin, useTab, useTheme, type RendererProps } from "pumi";

import {
  chose,
  committed,
  diff,
  diffed,
  dismissed,
  filtered,
  follow,
  opened,
  patchOf,
  picked,
  pressed,
  reveal,
  shown,
  synced,
  typed,
  type ChangesView,
  type HistoryView,
  type Line,
  type Shown,
  type Views,
} from "./main.ts";
import type { Commit, CommitMode, Sync } from "./git.ts";
import { when } from "./time.ts";

/** A line's height, and a commit's, and the filter box's, in pixels. */
const ROW = 26;
const COMMIT_ROW = 46;
const FILTER_ROW = 34;
/** The changes page's height that is not the commit message: the toolbar, the branch, the commit
 * button's row, the message's padding, and the filter with a few lines that always show. */
const FIXED = 34 + 34 + 34 + 12 + FILTER_ROW + ROW * 3;
/** A commit message line's height, in pixels. */
const MESSAGE_LINE = 18;
/** The page's height under which the commit box stops claiming its four rows and gives them, one at a
 * time, back to the lines above: at `ROOMY` and taller it keeps all four, shorter than that it shrinks. */
const ROOMY = 500;
/** The room the message is charged for and does not have, so its four rows run out at `ROOMY` and not
 * at the ~300 a page needs for them: a `ROOMY` page, less its four lines, less what is not the message. */
const CLAIMED = ROOMY - FIXED - 4 * MESSAGE_LINE;
/** A directory level's indent, and where its guide runs in it: under the middle of the folder icon. */
const INDENT = 16;
const GUIDE = 7;
const PAD = 10;
/** Lines made beyond each end of what shows. */
const OVERSCAN = 40;
/** How long the page waits to ask a worker that did not answer again, in ms. */
const RETRY = 1000;

/** A throw the page cannot answer: said in the log, not drawn over the window. */
function swallow(err: unknown): void {
  console.error(`git: ${err instanceof Error ? err.message : String(err)}`);
}

/** `1,417`. */
function count(n: number): string {
  return n.toLocaleString("en-US");
}

/** A menu's item. */
interface Item {
  label: string;
  run(): void;
  danger?: boolean;
}

type At = { x: number; y: number };

/** Where a click was, for the menu it opens. */
function at(e: any): At {
  return { x: e?.x ?? 0, y: e?.y ?? 0 };
}

/** The `panel` slot's component. Only the focused tab's directory is read here, so a tab's title
 * or status changing draws nothing of the page. */
export function Git() {
  const focused = useTab();
  const prefix = usePrefix();
  // A tab with no directory of its own (this plugin's diff, a page, nothing focused yet) is not
  // somewhere the user went: the page stays on the repository it was showing.
  const last = useRef<string | null>(null);
  const cwd = focused?.cwd;
  if (cwd && cwd !== "/" && focused?.block !== `${prefix}diff`) last.current = cwd;
  return <GitAt cwd={last.current} />;
}

/** The newer of what was posted and what an export answered. */
function newest<T extends { version: number }>(posted: T | undefined, answered: T | undefined): T | undefined {
  if (!posted) return answered;
  if (!answered) return posted;
  return answered.version > posted.version ? answered : posted;
}

const GitAt = memo(function GitAt({ cwd }: { cwd: string | null }) {
  const t = useTheme();
  const [tab, setTab] = useState<"changes" | "history">("changes");
  const [menu, setMenu] = useState<{ at: At; items: Item[] } | null>(null);
  const postedChanges = useChannel<ChangesView | undefined>("changes", undefined);
  const postedHistory = useChannel<HistoryView | undefined>("history", undefined);
  const [answer, setAnswer] = useState<Views | undefined>(undefined);
  // Raised to ask again: the worker was not there to answer (the app still starting its plugins, the
  // plugin reloading), so nothing was posted either, and the page would stay blank.
  const [retry, setRetry] = useState(0);
  useEffect(() => {
    // Asked with no directory too: the call starts the worker, which then posts the page.
    let live = true;
    let timer: ReturnType<typeof setTimeout> | undefined;
    follow(cwd).then(
      (views) => live && setAnswer(views),
      (err) => {
        swallow(err);
        if (live) timer = setTimeout(() => setRetry((n) => n + 1), RETRY);
      },
    );
    return () => {
      live = false;
      clearTimeout(timer);
    };
  }, [cwd, retry]);
  const changes = newest(postedChanges, answer?.changes);
  const history = newest(postedHistory, answer?.history);
  const open = (where: At, items: Item[]) => setMenu({ at: where, items });

  return (
    <div style={{ display: "flex", flexDirection: "column", flexGrow: 1, minHeight: 0, color: t.text }}>
      <div style={{ display: "flex", flexDirection: "row", height: 32, flexShrink: 0 }}>
        {(["changes", "history"] as const).map((each) => {
          const active = tab === each;
          return (
            <div
              key={each}
              onClick={() => setTab(each)}
              style={{
                display: "flex",
                flexDirection: "row",
                flexGrow: 1,
                flexBasis: 0,
                alignItems: "center",
                justifyContent: "center",
                gap: 4,
                position: "relative",
                cursor: "pointer",
                backgroundColor: active ? t.editor : undefined,
                borderBottomWidth: active ? 0 : 1,
                borderRightWidth: each === "changes" ? 1 : 0,
                borderColor: t.border,
                hover: { backgroundColor: active ? t.editor : t.hover },
              }}
            >
              {active ? <div style={{ position: "absolute", top: 0, left: 0, width: "100%", height: 2, backgroundColor: t.border }} /> : null}
              <text style={{ fontSize: 13, color: active ? t.text : t.muted }}>{each === "changes" ? "Changes" : "History"}</text>
              {each === "changes" && changes && changes.total > 0 ? <text style={{ fontSize: 12, color: t.faint }}>{`(${count(changes.total)})`}</text> : null}
            </div>
          );
        })}
      </div>
      <div style={{ display: "flex", flexDirection: "column", flexGrow: 1, minHeight: 0, backgroundColor: t.editor }}>
        {tab === "changes" ? <Changes view={changes} menu={open} /> : <History view={history} />}
      </div>
      {menu ? <Menu at={menu.at} items={menu.items} close={() => setMenu(null)} /> : null}
    </div>
  );
});

/** What a line does, by its id: one object for every line and every render, so a line that has not
 * changed is not drawn again for a new closure. */
interface LineActions {
  pick(id: string): void;
  open(id: string): void;
  check(line: Line): void;
  menu(line: Line, where: At): void;
}

const LABELS: Record<string, string> = {
  stage: "Stage",
  unstage: "Unstage",
  discard: "Discard Changes",
  ignore: "Add to .gitignore",
  ours: "Take Ours",
  theirs: "Take Theirs",
};

/** The changes: the toolbar, the lines, the branch, and the commit box. */
function Changes({ view, menu }: { view: ChangesView | undefined; menu(where: At, items: Item[]): void }) {
  const t = useTheme();
  const m = useMetrics();
  const list = useRef<any>(null);
  const [made, setMade] = useState({ start: 0, end: OVERSCAN * 2 });
  // Raised by each `/`: the filter box is drawn from its mount again, and takes the keys.
  const [focus, setFocus] = useState(0);
  // The last `/` a mounted filter box took: scrolled out and back, it does not take the keys again.
  const claimed = useRef(0);
  const shownRange = useRef({ start: 0, end: 0 });
  const [height, setHeight] = useState(0);

  const latest = useRef(menu);
  latest.current = menu;
  const actions = useRef<LineActions>({
    pick: (id) => {
      void picked(id).catch(swallow);
      if (id.includes(":file:")) void diffed(id).catch(swallow);
    },
    open: (id) => void opened(id).catch(swallow),
    check: (line) => {
      if (line.kind === "header") void chose(line.id === "group:staged" ? "unstage-all" : "stage-all", "").catch(swallow);
      else void chose(line.checkbox === "on" ? "unstage" : "stage", line.id).catch(swallow);
    },
    menu: (line, where) => {
      if (line.actions.length === 0) return;
      latest.current(
        where,
        line.actions.map((name) => ({ label: LABELS[name] ?? name, danger: name === "discard", run: () => void chose(name, line.id).catch(swallow) })),
      );
    },
  }).current;

  const lines = view?.lines ?? [];
  // The cursor moved by the keyboard stays in sight.
  const selected = lines.find((line) => line.selected)?.id ?? null;
  useEffect(() => {
    if (selected === null) return;
    const found = lines.findIndex((line) => line.id === selected);
    const { start, end } = shownRange.current;
    if (found < 0) return;
    const index = found + 1;
    if (index < start) list.current?.scrollToItem?.(index);
    else if (end > start && index >= end - 1) {
      const viewport = list.current?.listScrollTop?.()?.[2] ?? (end - start) * ROW;
      list.current?.scrollToItem?.(index, -(viewport - ROW));
    }
  }, [selected]);

  if (!view) return <div style={{ flexGrow: 1 }} />;
  if (view.error !== null) {
    return (
      <div style={{ padding: PAD }}>
        <text style={{ fontSize: 12, color: t.faint }}>{view.error.includes("not a git repository") ? "No repository here" : view.error}</text>
      </div>
    );
  }
  // The list's rows: the filter box, then the lines.
  const rows = lines.length + 1;
  const start = Math.max(0, Math.min(made.start, rows - 1));
  const end = Math.min(rows, Math.max(made.end, start));
  const everything = view.staged === view.total && view.total > 0;
  const filter = <Filter key="filter" value={view.filter} focus={focus} claimed={claimed} />;

  return (
    <div onLayout={(e: any) => setHeight(e?.height ?? 0)} style={{ display: "flex", flexDirection: "column", flexGrow: 1, minHeight: 0 }}>
      {/* The toolbar: the whole diff and what it adds up to, and Stage All. */}
      <div style={{ display: "flex", flexDirection: "row", alignItems: "center", gap: 6, height: 34, flexShrink: 0, paddingLeft: PAD, paddingRight: 8 }}>
        <div
          onClick={() => void diff().catch(swallow)}
          style={{ display: "flex", flexDirection: "row", alignItems: "center", gap: 6, height: 24, paddingLeft: 2, paddingRight: 4, borderRadius: m.radiusSmall, cursor: "pointer", flexShrink: 1, minWidth: 0, overflow: "hidden", hover: { backgroundColor: t.hover } }}
        >
          {/* Narrow, the label gives way first and the counts stay; narrower still, the button clips. */}
          <text style={{ fontSize: 13, color: t.muted, flexShrink: 0 }}>±</text>
          <text style={{ fontSize: 13, color: t.text, whiteSpace: "nowrap", textOverflow: "ellipsis", overflow: "hidden", minWidth: 0, flexShrink: 1 }}>View Diff</text>
          {view.added > 0 ? <text style={{ fontSize: 13, color: t.success, whiteSpace: "nowrap", flexShrink: 0 }}>{`+${count(view.added)}`}</text> : null}
          {view.deleted > 0 ? <text style={{ fontSize: 13, color: t.error, whiteSpace: "nowrap", flexShrink: 0 }}>{`−${count(view.deleted)}`}</text> : null}
        </div>
        <div style={{ flexGrow: 1 }} />
        <SplitButton
          label={everything ? "Unstage All" : "Stage All"}
          disabled={view.total === 0}
          onClick={() => void chose(everything ? "unstage-all" : "stage-all", "").catch(swallow)}
          onMenu={(e) =>
            menu(at(e), [
              { label: "Stage All", run: () => void chose("stage-all", "").catch(swallow) },
              { label: "Unstage All", run: () => void chose("unstage-all", "").catch(swallow) },
            ])
          }
        />
      </div>
      {view.notice && !view.notice.ok ? (
        <div style={{ display: "flex", flexDirection: "row", alignItems: "flex-start", gap: 6, paddingLeft: PAD, paddingRight: 8, paddingBottom: 4, flexShrink: 0 }}>
          <text style={{ fontSize: 12, color: t.error, flexGrow: 1, flexShrink: 1, minWidth: 0 }}>{view.notice.text}</text>
          {/* The notice stays until it is put away: what it says is worth reading again. */}
          <div aria-label="Dismiss" onClick={() => dismissed()} style={{ display: "flex", paddingTop: 1, flexShrink: 0, cursor: "pointer", hover: { opacity: 0.7 } }}>
            <Icon name="close" size={12} color={t.muted} />
          </div>
        </div>
      ) : null}
      <div
        tabIndex={0}
        onKeyDown={(e: any) => {
          const key = e?.key === " " ? "space" : e?.key;
          if (typeof key !== "string" || e?.modifiers?.cmd || e?.modifiers?.ctrl || e?.modifiers?.alt) return;
          if (!["j", "k", "up", "down", "enter", "space", "/", "escape"].includes(key)) return;
          e?.preventDefault?.();
          if (key === "/") {
            list.current?.scrollToItem?.(0);
            setFocus((n) => n + 1);
          }
          void pressed(key).catch(swallow);
        }}
        style={{ display: "flex", flexDirection: "column", flexGrow: 1, minHeight: 0 }}
      >
        {lines.length > 0 ? (
          <virtual-list
            ref={list}
            itemCount={rows}
            estimatedItemHeight={ROW}
            windowStart={start}
            overdraw={ROW * 10}
            aria-label="Changes"
            onVisibleRange={(e: any) => {
              const first = e?.startIndex ?? 0;
              const last = e?.endIndex ?? first;
              shownRange.current = { start: first, end: last };
              setMade((had) => {
                if (first - had.start >= OVERSCAN / 2 && had.end - last >= OVERSCAN / 2) return had;
                const next = { start: Math.max(0, first - OVERSCAN), end: last + OVERSCAN };
                return next.start === had.start && next.end === had.end ? had : next;
              });
            }}
            style={{ flexGrow: 1, minHeight: 0 }}
          >
            {Array.from({ length: end - start }, (_, i) => {
              const line = lines[start + i - 1];
              return line ? <LineView key={line.id} line={line} actions={actions} /> : filter;
            })}
          </virtual-list>
        ) : (
          <div style={{ display: "flex", flexDirection: "column", flexGrow: 1 }}>
            {filter}
            <div style={{ display: "flex", paddingLeft: PAD, paddingTop: 8 }}>
              {view.looked ? <text style={{ fontSize: 13, color: t.faint }}>{view.filter !== "" ? `Nothing matches “${view.filter}”` : "No changes"}</text> : null}
            </div>
          </div>
        )}
      </div>
      <Branch view={view} menu={menu} />
      <CommitBox view={view} menu={menu} room={height === 0 ? null : height - FIXED} />
    </div>
  );
}

/** Two lines that say the same: the memoised row keeps what it drew. */
function sameLine(a: Line, b: Line): boolean {
  return (
    a.id === b.id &&
    a.kind === b.kind &&
    a.depth === b.depth &&
    a.open === b.open &&
    a.text === b.text &&
    a.status === b.status &&
    a.checkbox === b.checkbox &&
    a.count === b.count &&
    a.selected === b.selected &&
    a.actions.join() === b.actions.join()
  );
}

/** One line: a group's header, its empty note, a folder or a file of its tree, or a conflict's note. */
const LineView = memo(
  function LineView({ line, actions }: { line: Line; actions: LineActions }) {
    const t = useTheme();
    const m = useMetrics();
    if (line.kind === "empty") {
      return (
        <div style={{ display: "flex", alignItems: "center", width: "100%", height: ROW, paddingLeft: PAD }}>
          <text style={{ fontSize: 13, color: t.faint }}>{line.text}</text>
        </div>
      );
    }
    if (line.kind === "note") {
      return (
        <div style={{ display: "flex", flexDirection: "row", alignItems: "center", width: "100%", height: ROW, paddingLeft: PAD, overflow: "hidden" }}>
          <Indent depth={line.depth} />
          <text style={{ fontSize: 11, color: t.faint, whiteSpace: "nowrap", textOverflow: "ellipsis", overflow: "hidden", minWidth: 0 }}>{line.text}</text>
        </div>
      );
    }
    const header = line.kind === "header";
    return (
      <div
        onClick={() => actions.pick(line.id)}
        onDoubleClick={() => {
          if (line.kind === "file") actions.open(line.id);
        }}
        onAuxClick={(e: any) => {
          if (e?.isRightClick) actions.menu(line, at(e));
        }}
        style={{
          display: "flex",
          flexDirection: "row",
          alignItems: "center",
          gap: 6,
          width: "100%",
          minWidth: 0,
          height: ROW,
          flexShrink: 0,
          paddingLeft: PAD,
          paddingRight: 8,
          overflow: "hidden",
          cursor: "pointer",
          backgroundColor: line.selected ? t.active : undefined,
          hover: { backgroundColor: line.selected ? t.active : t.hover },
        }}
      >
        {header ? (
          <Icon name={line.open ? "chevron_down" : "chevron_right"} size={12} color={t.muted} />
        ) : (
          <>
            <Indent depth={line.depth} />
            {line.kind === "folder" ? <Icon name={line.open ? "folder_open" : "folder"} size={14} color={t.muted} /> : <Status status={line.status} />}
          </>
        )}
        <div style={{ display: "flex", flexGrow: 1, flexShrink: 1, minWidth: 0, overflow: "hidden" }}>
          <text
            style={{
              fontSize: 13,
              color: header ? t.muted : line.status === "deleted" ? t.muted : t.text,
              whiteSpace: "nowrap",
              textOverflow: "ellipsis",
              overflow: "hidden",
              minWidth: 0,
            }}
          >
            {line.text}
          </text>
        </div>
        {header && line.count > 0 && line.id !== "group:staged" ? (
          <div style={{ display: "flex", alignItems: "center", height: 18, paddingLeft: 6, paddingRight: 6, flexShrink: 0, borderRadius: m.radiusSmall, backgroundColor: t.active }}>
            <text style={{ fontSize: 11, color: t.muted }}>{count(line.count)}</text>
          </div>
        ) : null}
        <Checkbox state={line.checkbox} onClick={() => actions.check(line)} />
      </div>
    );
  },
  (a, b) => a.actions === b.actions && sameLine(a.line, b.line),
);

/** The room a line's depth takes, with a guide down each level above it. */
function Indent({ depth }: { depth: number }) {
  const t = useTheme();
  if (depth === 0) return null;
  // The row's gap sits between the indent and the icon: the guides are measured from the row's edge.
  return (
    <div style={{ position: "relative", width: depth * INDENT - 6, height: ROW, flexShrink: 0 }}>
      {Array.from({ length: depth }, (_, level) => (
        <div key={level} style={{ position: "absolute", left: level * INDENT + GUIDE, top: 0, width: 1, height: ROW, backgroundColor: t.border }} />
      ))}
    </div>
  );
}

/** A file's mark: what git says of it, as a square in its colour. */
function Status({ status }: { status: string }) {
  const t = useTheme();
  const [name, color] =
    status === "added" || status === "untracked"
      ? ["square_plus", t.success]
      : status === "deleted"
        ? ["square_minus", t.error]
        : status === "conflict"
          ? ["warning", t.error]
          : status === "renamed"
            ? ["square_dot", t.accent]
            : ["square_dot", t.warning];
  return <Icon name={name} size={14} color={color} />;
}

/** A line's checkbox: git's own three states. */
function Checkbox({ state, onClick }: { state: string; onClick(): void }) {
  const t = useTheme();
  if (state === "") return <div style={{ width: 16, flexShrink: 0 }} />;
  const on = state !== "off";
  return (
    <div
      aria-label="Stage or unstage"
      onClick={(e: any) => {
        e?.stopPropagation?.();
        onClick();
      }}
      style={{
        display: "flex",
        width: 16,
        height: 16,
        flexShrink: 0,
        alignItems: "center",
        justifyContent: "center",
        borderRadius: 3,
        borderWidth: 1,
        borderColor: on ? t.accent : t.border,
        backgroundColor: on ? t.accent : t.editor,
        cursor: "pointer",
        hover: { borderColor: on ? t.accent : t.faint },
      }}
    >
      {on ? <Icon name={state === "partial" ? "dash" : "check"} size={11} color={t.accentText} /> : null}
    </div>
  );
}

/** The filter box, the first row of the lines' list, as the agents' search: a strip the sidebar's
 * width between two rules. The tree keeps what holds its text, every directory open. `/` mounts it
 * again so that it takes the keys. */
function Filter({ value, focus, claimed }: { value: string; focus: number; claimed: { current: number } }) {
  const t = useTheme();
  const [text, setText] = useState(value);
  const [focused, setFocused] = useState(false);
  const take = focus > claimed.current;
  useEffect(() => {
    claimed.current = Math.max(claimed.current, focus);
  }, [focus]);
  // What the worker cleared (a commit, another repository) empties the box too.
  useEffect(() => setText(value), [value]);
  const clear = () => {
    setText("");
    filtered("");
  };
  return (
    <div style={{ display: "flex", flexDirection: "row", alignItems: "center", gap: 8, width: "100%", height: FILTER_ROW, flexShrink: 0, paddingLeft: PAD + 2, paddingRight: 8, borderTopWidth: 1, borderBottomWidth: 1, borderColor: t.border }}>
      <Icon name="search" size={14} color={focused ? t.muted : t.faint} />
      <input
        key={focus}
        value={text}
        autoFocus={take}
        placeholder="Filter the changes…"
        aria-label="Filter the changes"
        onFocus={() => setFocused(true)}
        onBlur={() => setFocused(false)}
        onChange={(e: any) => {
          const next = e?.value ?? "";
          setText(next);
          filtered(next);
        }}
        onKeyDown={(e: any) => {
          // The box is inside the list: the list's own keys typed here are the box's.
          const key = e?.key === " " ? "space" : e?.key;
          if (["j", "k", "up", "down", "enter", "space", "/", "escape"].includes(key)) e?.stopPropagation?.();
          if (key === "escape") clear();
        }}
        style={{ display: "flex", flexGrow: 1, minWidth: 0, height: 22, fontSize: 13, color: t.text, backgroundColor: "transparent" }}
      />
      {text === "" ? null : (
        <div aria-label="Clear the filter" onClick={clear} style={{ display: "flex", cursor: "pointer" }}>
          <Icon name="close" size={12} color={t.muted} />
        </div>
      )}
    </div>
  );
}

/** The branch, and what its button does with the remote: publish it, push, pull, or fetch. */
function Branch({ view, menu }: { view: ChangesView; menu(where: At, items: Item[]): void }) {
  const t = useTheme();
  const { upstream, remote, ahead, behind } = view.remote;
  const primary: { what: Sync; label: string; icon: string | null } =
    upstream === null
      ? { what: "publish", label: "Publish", icon: "arrow_up" }
      : behind > 0
        ? { what: "pull", label: `Pull ${behind}`, icon: null }
        : ahead > 0
          ? { what: "push", label: `Push ${ahead}`, icon: "arrow_up" }
          : { what: "fetch", label: "Fetch", icon: "rotate_cw" };
  const busy = view.busy;
  const run = (what: Sync) => void synced(what).catch(swallow);
  return (
    <div style={{ display: "flex", flexDirection: "row", alignItems: "center", gap: 6, height: 34, flexShrink: 0, paddingLeft: PAD, paddingRight: 8, borderTopWidth: 1, borderColor: t.border }}>
      <Icon name="git_branch" size={13} color={t.muted} />
      <text style={{ fontSize: 13, color: t.text, flexGrow: 1, flexShrink: 1, minWidth: 0, whiteSpace: "nowrap", textOverflow: "ellipsis", overflow: "hidden" }}>{view.branch || "no branch"}</text>
      {remote !== null ? (
        <SplitButton
          icon={busy === null ? primary.icon : "loader_circle"}
          label={busy === null ? primary.label : `${busy[0]!.toUpperCase()}${busy.slice(1)}ing…`}
          disabled={busy !== null}
          onClick={() => run(primary.what)}
          onMenu={(e) =>
            menu(at(e), [
              { label: "Fetch", run: () => run("fetch") },
              { label: "Pull", run: () => run("pull") },
              upstream === null ? { label: "Publish", run: () => run("publish") } : { label: "Push", run: () => run("push") },
            ])
          }
        />
      ) : null}
    </div>
  );
}

/** The commit box: the message, kept as it is typed (the worker hears it, nothing is drawn back),
 * and the button that commits it — what is staged, or every tracked change when nothing is. `room`
 * is the height the message may take before the lines above it lose their few rows; null unmeasured. */
function CommitBox({ view, menu, room }: { view: ChangesView; menu(where: At, items: Item[]): void; room: number | null }) {
  const t = useTheme();
  const [draft, setDraft] = useState(view.message);
  const [tall, setTall] = useState(false);
  const fits = room === null ? Infinity : Math.max(1, Math.floor((room - CLAIMED) / MESSAGE_LINE));
  const minRows = Math.min(tall ? 14 : 4, fits);
  const maxRows = Math.max(minRows, Math.min(tall ? 40 : 10, fits));
  const mode: CommitMode = view.staged > 0 || view.tracked === 0 ? "staged" : "tracked";
  const empty = draft.trim() === "";
  const can = !empty && (mode === "staged" ? view.staged > 0 : view.tracked > 0);
  const commit = (how: CommitMode) => {
    const text = draft;
    void committed(text, how)
      .then((done) => {
        if (done) setDraft((now) => (now === text ? "" : now));
      })
      .catch(swallow);
  };
  return (
    <div style={{ display: "flex", flexDirection: "column", flexShrink: 0, borderTopWidth: 1, borderColor: t.border }}>
      <div style={{ display: "flex", flexDirection: "row" }}>
        <textarea
          aria-label="Commit message"
          value={draft}
          placeholder="Enter commit message"
          minRows={minRows}
          maxRows={maxRows}
          theme={{ bg: t.editor, border: t.editor, text: t.text, textMuted: t.muted, textFaint: t.faint, accent: t.accent, caret: t.accent }}
          onChange={(e: any) => {
            const next = e?.value ?? "";
            setDraft(next);
            typed(next);
          }}
          onKeyDown={(e: any) => {
            const mod = e?.modifiers?.cmd === true || e?.modifiers?.ctrl === true;
            if (e?.key !== "enter" || !mod) return;
            e?.preventDefault?.();
            e?.stopPropagation?.();
            if (can) commit(mode);
          }}
          style={{ display: "flex", flexGrow: 1, minWidth: 0, paddingLeft: PAD, paddingRight: 4, paddingTop: 8, paddingBottom: 4, fontSize: 13, color: t.text }}
        />
        <div style={{ display: "flex", flexDirection: "column", paddingTop: 6, paddingRight: 6 }}>
          <IconButton name={tall ? "minimize" : "maximize"} label={tall ? "A smaller box" : "A bigger box"} on={false} onClick={() => setTall(!tall)} />
        </div>
      </div>
      <div style={{ display: "flex", flexDirection: "row", alignItems: "center", justifyContent: "flex-end", paddingLeft: PAD, paddingRight: 8, paddingBottom: 8, paddingTop: 2 }}>
        <SplitButton
          label={mode === "staged" ? "Commit" : "Commit Tracked"}
          disabled={!can}
          onClick={() => commit(mode)}
          onMenu={(e) =>
            menu(at(e), [
              { label: "Commit Staged", run: () => commit("staged") },
              { label: "Commit Tracked", run: () => commit("tracked") },
              { label: "Amend Last Commit", run: () => commit("amend") },
            ])
          }
        />
      </div>
    </div>
  );
}

/** A small square button with one of the app's icons. */
function IconButton({ name, label, on, onClick }: { name: string; label: string; on: boolean; onClick(): void }) {
  const t = useTheme();
  const m = useMetrics();
  return (
    <div
      aria-label={label}
      onClick={onClick}
      style={{ display: "flex", width: 24, height: 24, flexShrink: 0, alignItems: "center", justifyContent: "center", borderRadius: m.radiusSmall, cursor: "pointer", backgroundColor: on ? t.active : undefined, hover: { backgroundColor: on ? t.active : t.hover } }}
    >
      <Icon name={name} size={14} color={on ? t.text : t.muted} />
    </div>
  );
}

/** A bordered button with a menu beside it: `[ Stage All | ▾ ]`. */
function SplitButton({ label, icon, disabled, onClick, onMenu }: { label: string; icon?: string | null; disabled: boolean; onClick(): void; onMenu(e: any): void }) {
  const t = useTheme();
  const m = useMetrics();
  return (
    <div style={{ display: "flex", flexDirection: "row", alignItems: "stretch", height: 24, flexShrink: 0, borderRadius: m.radiusSmall, borderWidth: 1, borderColor: t.border, overflow: "hidden" }}>
      <div
        onClick={() => {
          if (!disabled) onClick();
        }}
        style={{ display: "flex", flexDirection: "row", alignItems: "center", gap: 5, paddingLeft: 8, paddingRight: 8, cursor: disabled ? "default" : "pointer", hover: { backgroundColor: disabled ? undefined : t.hover } }}
      >
        {icon ? <Icon name={icon} size={12} color={disabled ? t.faint : t.muted} /> : null}
        <text style={{ fontSize: 13, color: disabled ? t.faint : t.text, whiteSpace: "nowrap" }}>{label}</text>
      </div>
      <div
        aria-label="More"
        onClick={(e: any) => onMenu(e)}
        style={{ display: "flex", alignItems: "center", justifyContent: "center", width: 22, borderLeftWidth: 1, borderColor: t.border, cursor: "pointer", hover: { backgroundColor: t.hover } }}
      >
        <Icon name="chevron_down" size={11} color={t.muted} />
      </div>
    </div>
  );
}

/** The history: the commits, newest first, a subject and who, when and which under it. */
function History({ view }: { view: HistoryView | undefined }) {
  const t = useTheme();
  const [made, setMade] = useState({ start: 0, end: OVERSCAN * 2 });
  if (!view) return <div style={{ flexGrow: 1 }} />;
  if (view.root === null || view.commits === null || view.commits.length === 0) {
    return (
      <div style={{ padding: PAD }}>
        <text style={{ fontSize: 12, color: t.faint }}>{view.root === null ? "No repository here" : view.commits === null ? "" : "No commits yet"}</text>
      </div>
    );
  }
  const commits = view.commits;
  const start = Math.max(0, Math.min(made.start, commits.length - 1));
  const end = Math.min(commits.length, Math.max(made.end, start));
  const now = Date.now();
  return (
    <virtual-list
      itemCount={commits.length}
      estimatedItemHeight={COMMIT_ROW}
      windowStart={start}
      overdraw={COMMIT_ROW * 6}
      aria-label="History"
      onVisibleRange={(e: any) => {
        const first = e?.startIndex ?? 0;
        const last = e?.endIndex ?? first;
        setMade((had) => {
          if (first - had.start >= OVERSCAN / 2 && had.end - last >= OVERSCAN / 2) return had;
          const next = { start: Math.max(0, first - OVERSCAN), end: last + OVERSCAN };
          return next.start === had.start && next.end === had.end ? had : next;
        });
      }}
      style={{ flexGrow: 1, minHeight: 0 }}
    >
      {commits.slice(start, end).map((commit) => (
        <CommitView key={commit.sha} commit={commit} date={when(commit.timestamp, view.dates, now)} author={view.author} />
      ))}
    </virtual-list>
  );
}

const CommitView = memo(function CommitView({ commit, date, author }: { commit: Commit; date: string; author: boolean }) {
  const t = useTheme();
  const meta = { fontSize: 12, color: t.muted, whiteSpace: "nowrap" } as const;
  const dot = <text style={{ fontSize: 12, color: t.faint }}>•</text>;
  return (
    <div
      onClick={() => void shown(commit.sha).catch(swallow)}
      style={{ display: "flex", flexDirection: "column", justifyContent: "center", gap: 3, width: "100%", minWidth: 0, height: COMMIT_ROW, paddingLeft: PAD, paddingRight: PAD, overflow: "hidden", cursor: "pointer", hover: { backgroundColor: t.hover } }}
    >
      <div style={{ display: "flex", flexDirection: "row", alignItems: "center", gap: 6, minWidth: 0, overflow: "hidden" }}>
        <text style={{ fontSize: 13, color: t.text, whiteSpace: "nowrap", textOverflow: "ellipsis", overflow: "hidden", minWidth: 0, flexShrink: 1 }}>{commit.subject}</text>
        {commit.tags.map((tag) => (
          <text key={tag} style={{ fontSize: 11, color: t.accent, flexShrink: 0 }}>
            {tag}
          </text>
        ))}
      </div>
      <div style={{ display: "flex", flexDirection: "row", alignItems: "center", gap: 6, minWidth: 0, overflow: "hidden" }}>
        {author ? (
          <>
            <div style={{ display: "flex", width: 16, height: 16, flexShrink: 0, alignItems: "center", justifyContent: "center", borderRadius: 8, borderWidth: 1, borderColor: t.border }}>
              <Icon name="person" size={10} color={t.faint} />
            </div>
            <text style={{ ...meta, textOverflow: "ellipsis", overflow: "hidden", minWidth: 0, flexShrink: 1 }}>{commit.author}</text>
            {dot}
          </>
        ) : null}
        <text style={{ ...meta, flexShrink: 0 }}>{date}</text>
        {dot}
        <text style={{ ...meta, flexShrink: 0 }}>{commit.sha.slice(0, 7)}</text>
        {commit.unpushed ? <Icon name="arrow_up" size={11} color={t.warning} /> : null}
      </div>
    </div>
  );
});

/** A menu where it was pressed. */
function Menu({ at: where, items, close }: { at: At; items: Item[]; close(): void }) {
  const t = useTheme();
  const m = useMetrics();
  return (
    <anchored position={where} fit="snap" deferred occlude onMouseDownOutside={close}>
      <div style={{ display: "flex", flexDirection: "column", minWidth: 170, padding: 4, borderRadius: m.radius, backgroundColor: t.surface, borderWidth: 1, borderColor: t.border, boxShadow: { offsetX: 0, offsetY: 6, blurRadius: 18, spreadRadius: 0, color: "#00000066" } }}>
        {items.map((item) => (
          <div
            key={item.label}
            onClick={() => {
              close();
              item.run();
            }}
            style={{ display: "flex", alignItems: "center", height: 26, paddingLeft: 8, paddingRight: 8, borderRadius: m.radiusSmall, cursor: "pointer", hover: { backgroundColor: t.hover } }}
          >
            <text style={{ fontSize: 13, color: item.danger ? t.error : t.text }}>{item.label}</text>
          </div>
        ))}
      </div>
    </anchored>
  );
}

// ---- the diff tab

/** One line of a patch, and what it is: a file's header, a hunk's, an addition, a deletion. */
export interface PatchLine {
  /** The file it is in, as its `diff --git` header names it. */
  file: string | null;
  kind: "file" | "hunk" | "add" | "del" | "plain";
  text: string;
}

/** A patch as lines, each keeping the file it is in. */
export function patchLines(patch: string): PatchLine[] {
  const lines: PatchLine[] = [];
  let file: string | null = null;
  for (const text of patch.split("\n")) {
    if (text.startsWith("diff --git ")) {
      const at = text.indexOf(" b/");
      file = at >= 0 ? text.slice(at + 3) : text.slice(11);
      lines.push({ file, kind: "file", text });
    } else if (text.startsWith("@@")) lines.push({ file, kind: "hunk", text });
    else if (text.startsWith("+++") || text.startsWith("---") || text.startsWith("index ")) lines.push({ file, kind: "plain", text });
    else if (text.startsWith("+")) lines.push({ file, kind: "add", text });
    else if (text.startsWith("-")) lines.push({ file, kind: "del", text });
    else lines.push({ file, kind: "plain", text });
  }
  return lines;
}

/** This plugin's options carry its prefix (`pumi_dev.git.`), which is how the tab's own `diff` is
 * named in the state the window reads. */
function usePrefix(): string {
  const id = usePlugin();
  return id === undefined ? "" : `${id.replace("/", ".").replaceAll("-", "_")}.`;
}

/** The files of a patch, and what it adds and deletes, read in one pass over its lines. */
function summary(patch: string): { files: string[]; added: number; deleted: number } {
  const files: string[] = [];
  let added = 0;
  let deleted = 0;
  let at = 0;
  while (at < patch.length) {
    let end = patch.indexOf("\n", at);
    if (end < 0) end = patch.length;
    const c = patch.charCodeAt(at);
    if (c === 43 /* + */) {
      if (!patch.startsWith("+++ ", at)) added += 1;
    } else if (c === 45 /* - */) {
      if (!patch.startsWith("--- ", at)) deleted += 1;
    } else if (c === 100 /* d */ && patch.startsWith("diff --git ", at)) {
      const line = patch.slice(at, end);
      const b = line.lastIndexOf(" b/");
      files.push(b >= 0 ? line.slice(b + 3) : line.slice(11));
    }
    at = end + 1;
  }
  return { files, added, deleted };
}

/** A diff tab: what the page said to show (the tab's `diff` option), read through the worker and
 * drawn here — its files' lines in the theme's own additions and deletions, each file header
 * folding the lines under it. What it drew stays up while the next read runs, so a change in the
 * work tree never blanks it. */
export function Diff({ tab }: RendererProps) {
  const t = useTheme();
  const m = useMetrics();
  const each = useTab(tab.id);
  const prefix = usePrefix();
  const value = (each?.options?.[`${prefix}diff`] as string | undefined) ?? "";
  const spec = useMemo<Shown | null>(() => {
    try {
      return value === "" ? null : (JSON.parse(value) as Shown);
    } catch {
      return null;
    }
  }, [value]);
  // The work tree's diff is read again whenever the page is posted anew; a commit's never changes.
  const changes = useChannel<ChangesView | undefined>("changes", undefined);
  const stamp = spec?.kind === "work" ? changes?.version ?? 0 : 0;
  const [read, setRead] = useState<{ value: string; patch: string; error: string | null } | null>(null);
  const [folded, setFolded] = useState<Set<string>>(() => new Set());

  useEffect(() => {
    if (value === "") return;
    let live = true;
    void patchOf(value)
      .then((answer) => {
        if (!live) return;
        setRead((had) => (had && had.value === value && had.patch === answer.patch && had.error === answer.error ? had : { value, ...answer }));
      })
      .catch(swallow);
    return () => {
      live = false;
    };
  }, [value, stamp]);
  // Another diff is another set of files: none of them folded.
  useEffect(() => setFolded(new Set()), [value]);

  const patch = read?.patch ?? "";
  const lines = useMemo(() => patchLines(patch), [patch]);
  const info = useMemo(() => summary(patch), [patch]);
  const many = info.files.length > 1;
  const allFolded = many && folded.size >= info.files.length;
  const path = spec?.kind === "work" ? spec.path : undefined;

  return (
    <div style={{ display: "flex", flexDirection: "column", flexGrow: 1, minHeight: 0, color: t.text }}>
      <div style={{ display: "flex", flexDirection: "row", alignItems: "center", gap: 8, height: 34, flexShrink: 0, paddingLeft: 12, paddingRight: 8, borderBottomWidth: 1, borderColor: t.border }}>
        <Icon name={spec?.kind === "commit" ? "status_dot" : "git_branch"} size={13} color={t.muted} />
        <text style={{ fontSize: 13, color: t.text, whiteSpace: "nowrap", textOverflow: "ellipsis", overflow: "hidden", minWidth: 0, flexShrink: 1 }}>
          {path ?? spec?.title ?? "Diff"}
        </text>
        {spec?.kind === "work" && spec.path === undefined ? null : spec?.kind === "work" && spec.side === "staged" ? <Tag text="Staged" /> : null}
        {many ? <text style={{ fontSize: 12, color: t.faint, whiteSpace: "nowrap" }}>{`${count(info.files.length)} files`}</text> : null}
        {info.added > 0 ? <text style={{ fontSize: 12, color: t.success, whiteSpace: "nowrap" }}>{`+${count(info.added)}`}</text> : null}
        {info.deleted > 0 ? <text style={{ fontSize: 12, color: t.error, whiteSpace: "nowrap" }}>{`−${count(info.deleted)}`}</text> : null}
        <div style={{ flexGrow: 1 }} />
        {many ? (
          <IconButton
            name={allFolded ? "chevron_down" : "chevron_up_down"}
            label={allFolded ? "Open every file" : "Fold every file"}
            on={false}
            onClick={() => setFolded(allFolded ? new Set() : new Set(info.files))}
          />
        ) : null}
        {spec?.kind === "work" && path !== undefined ? (
          <IconButton name="arrow_up_right" label="Open the file" on={false} onClick={() => void reveal(spec.root, path, null).catch(swallow)} />
        ) : null}
      </div>
      {read === null ? (
        <div style={{ flexGrow: 1, backgroundColor: t.editor }} />
      ) : read.error !== null ? (
        <div style={{ padding: 12 }}>
          <text style={{ fontSize: 12, color: t.error }}>{read.error}</text>
        </div>
      ) : patch === "" ? (
        <div style={{ display: "flex", flexGrow: 1, alignItems: "center", justifyContent: "center", backgroundColor: t.editor }}>
          <text style={{ fontSize: 13, color: t.faint }}>No changes</text>
        </div>
      ) : (
        <div style={{ display: "flex", flexDirection: "column", flexGrow: 1, minHeight: 0, overflowY: "scroll", backgroundColor: t.editor, borderRadius: m.radiusSmall }}>
          {lines.map((line, index) => {
            if (line.file !== null && folded.has(line.file) && line.kind !== "file") return null;
            const wash = line.kind === "add" ? (t.diffAdd ?? t.active) : line.kind === "del" ? (t.diffDel ?? t.selected) : undefined;
            return (
              <div
                key={index}
                onClick={
                  line.file === null || line.kind !== "file"
                    ? undefined
                    : () =>
                        setFolded((had) => {
                          const next = new Set(had);
                          if (!next.delete(line.file!)) next.add(line.file!);
                          return next;
                        })
                }
                style={{ display: "flex", flexShrink: 0, paddingLeft: 8, paddingRight: 8, cursor: line.kind === "file" ? "pointer" : "default", backgroundColor: wash }}
              >
                <text style={{ fontSize: m.fontSize - 1, color: line.kind === "file" || line.kind === "hunk" ? t.muted : t.text, whiteSpace: "nowrap" }}>{line.text}</text>
              </div>
            );
          })}
        </div>
      )}
    </div>
  );
}

function Tag({ text }: { text: string }) {
  const t = useTheme();
  const m = useMetrics();
  return (
    <div style={{ display: "flex", alignItems: "center", height: 18, paddingLeft: 6, paddingRight: 6, flexShrink: 0, borderRadius: m.radiusSmall, backgroundColor: t.active }}>
      <text style={{ fontSize: 11, color: t.muted }}>{text}</text>
    </div>
  );
}
