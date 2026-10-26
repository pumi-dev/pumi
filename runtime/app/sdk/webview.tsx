// Web pages: `<WebView handle src/>` in a component, `useWebview(handle)` for what it shows, and
// `webview(handle, method)` to drive it. A plugin's own page (`src` without a scheme: one of its
// files) gets `window.pumi`, whose calls run here, in the window, as `call` does. Such a page is
// the window's own: the app draws its menus (`pumi.menu`, a right-click), and the app's drags
// reach it as DOM drag events.
import { useEffect, useMemo, useRef, useState } from "react"
import { call, engine, on } from "../app/engine.ts"
import { entries, invoke } from "../app/plugins.ts"
import { usePlugin } from "./contributions.ts"
import { useDragState, useDroppable } from "./drag.ts"
import { Icon } from "./elements.tsx"
import { paletteTokens, useAppearance, useMetrics, usePalette, useTheme } from "./theme.ts"
import { themeCss } from "./webview-theme.ts"

export interface WebState {
  url: string
  title: string
  loading: boolean
  progress: number
  back: boolean
  forward: boolean
  devtools: boolean
  /** Where it is in its window, `[x, y, width, height]`, once placed. */
  frame?: [number, number, number, number] | null
}

/** One item of a page's menu, as `pumi.menu` sends it: `run` is the page's callback. */
interface PageItem {
  title?: string
  icon?: string
  detail?: string
  checked?: boolean
  disabled?: boolean
  separator?: boolean
  run?: { ref: number }
}

/** What a mounted `<WebView>` answers for its page. */
interface Page {
  menu(items: PageItem[], x: number, y: number, done: (index: number | null) => void): void
  focused(): void
}

const pages = new Map<string, Page>()
/** Whether the page takes the drag over it, as it last said (`dragAccept`). */
const taking = new Map<string, boolean>()

const states = new Map<string, WebState>()
const listeners = new Map<string, Set<() => void>>()
const opens = new Map<string, (url: string) => void>()
/** The plugin each of its own pages is of (`owner/name`), by handle. */
const owners = new Map<string, string>()

on("webview", (data: { handle: string; bridge?: string; open?: string; notice?: string; focused?: boolean; url?: string } & Partial<WebState>) => {
  const { handle } = data
  if (data.bridge !== undefined) return bridge(handle, data.bridge)
  if (data.focused) return pages.get(handle)?.focused()
  if (data.open !== undefined) return (opens.get(handle) ?? openOutside)(data.open)
  if (data.notice !== undefined) return console.error(`pumi: web page ${handle}: ${data.notice}`)
  if (data.url === undefined) return
  states.set(handle, data as WebState)
  for (const listener of listeners.get(handle) ?? []) listener()
})

function openOutside(url: string) {
  // `start` is a cmd builtin, and its first argument is the window title: the empty one keeps it
  // from reading the URL's `&` as the end of the command.
  const argv =
    process.platform === "darwin" ? ["open", url] : process.platform === "win32" ? ["cmd", "/c", "start", "", url] : ["xdg-open", url]
  Bun.spawn(argv, { stdio: ["ignore", "ignore", "ignore"] })
}

/** A plugin page's `window.pumi` request: `call` runs it; the answer goes back into the page. */
async function bridge(handle: string, body: string) {
  let msg: { id?: number; op?: string; body?: any }
  try {
    msg = JSON.parse(body)
  } catch {
    return
  }
  if (typeof msg.id !== "number") return
  const reply = (ok: boolean, value: unknown) =>
    engine.webview(handle, "eval", `window.__pumi&&__pumi.reply(${msg.id},${ok},${JSON.stringify(value ?? null)})`)
  if (msg.op === "edit") {
    try {
      return reply(true, void engine.webview(handle, "edit", String(msg.body)))
    } catch (err: any) {
      return reply(false, { kind: "invalid_params", message: String(err?.message ?? err) })
    }
  }
  if (msg.op === "dragAccept") {
    taking.set(handle, msg.body === true)
    return reply(true, null)
  }
  if (msg.op === "menu") {
    const page = pages.get(handle)
    const items: PageItem[] = Array.isArray(msg.body?.items) ? msg.body.items : []
    if (!page || !items.length) return reply(true, null)
    return page.menu(items, Number(msg.body?.x) || 0, Number(msg.body?.y) || 0, (index) => reply(true, index))
  }
  if (msg.op !== "call" || typeof msg.body?.name !== "string") {
    return reply(false, { kind: "invalid_params", message: `a page can't ${msg.op} yet` })
  }
  const args = msg.body.args === undefined || msg.body.args === null ? [] : Array.isArray(msg.body.args) ? msg.body.args : [msg.body.args]
  // Its own plugin's `main.ts` (`<prefix>rpc`) goes as its `ui.tsx`'s calls do, which wait while
  // the app that runs the plugin joins (it just started, or the plugin reloaded) instead of failing.
  const owner = owners.get(handle)
  const prefix = owner ? entries.get(owner)?.prefix : undefined
  try {
    if (owner && prefix && msg.body.name === `${prefix}rpc`) {
      const asked = (args[0] ?? {}) as { name?: unknown; args?: unknown }
      const rest = Array.isArray(asked.args) ? asked.args : asked.args === undefined ? [] : [asked.args]
      return reply(true, await invoke(owner, String(asked.name), rest))
    }
    reply(true, await call(msg.body.name, ...args))
  } catch (err: any) {
    reply(false, { kind: err?.kind ?? "exception", message: String(err?.message ?? err) })
  }
}

/** What the page `handle` shows, following it. */
export function useWebview(handle: string): WebState | undefined {
  const [, redraw] = useState(0)
  useEffect(() => {
    const set = listeners.get(handle) ?? new Set()
    const listener = () => redraw((n) => n + 1)
    set.add(listener)
    listeners.set(handle, set)
    return () => void set.delete(listener)
  }, [handle])
  return states.get(handle)
}

/** Drives the page `handle`: `back`, `forward`, `reload`, `hardReload`, `stop`, `devtools`, `zoom`,
 *  `eval`, `load`, `focus`, `state`, `edit` (`copy`, `cut`, `paste`, `selectAll`, `undo`, `redo`).
 *  Answers its state. */
export function webview(handle: string, method: string, arg?: unknown): WebState {
  return engine.webview(handle, method, arg as any) as WebState
}

export interface WebViewProps {
  /** Names the page, unique in the app (a tab's id, say). */
  handle: string
  /** A URL, or one of the plugin's files (`index.html`). Changing it loads it. */
  src: string
  /** What the plugin's page reads as `pumi.args`. */
  args?: unknown
  zoom?: number
  autoFocus?: boolean
  /** A link the page opens in a new window; by default, the system's browser. */
  onOpen?(url: string): void
  /**
   * The colour under a plugin's page where the system can't show the window through it (Windows,
   * Linux; on macOS the page is clear): the tab's, `editor`, unless given.
   */
  background?: string
  /**
   * Chords the page keeps though the app's bindings take them (`["cmd+d"]` for an editor's own).
   * Every other chord of the app's goes to the app while the page has the keyboard.
   */
  keepKeys?: string[]
  style?: Record<string, unknown>
}

export function WebView({ handle, src, args, zoom, autoFocus, onOpen, background, keepKeys, style }: WebViewProps) {
  const plugin = usePlugin()
  const tokens = useTheme()
  const palette = usePalette()
  // The page reads the palette beside the theme's tokens (`palette.foreground`, …): a component
  // that should look like the terminal beside it can.
  const theme = useMemo(() => ({ ...tokens, ...paletteTokens(palette) }), [tokens, palette])
  const metrics = useMetrics()
  const appearance = useAppearance()
  const css = useMemo(() => themeCss(theme, metrics, appearance), [theme, metrics, appearance])
  const [menu, setMenu] = useState<{ at: { x: number; y: number }; items: PageItem[]; done(index: number | null): void }>()
  useEffect(() => {
    if (!onOpen) return
    opens.set(handle, onOpen)
    return () => void opens.delete(handle)
  }, [handle, onOpen])
  useEffect(() => {
    pages.set(handle, {
      menu(items, x, y, done) {
        const [ox, oy] = webview(handle, "state").frame ?? [0, 0]
        setMenu((open) => {
          open?.done(null)
          return { at: { x: ox + x, y: oy + y }, items, done }
        })
      },
      // A press in the page is a press outside the menu: the app never hears it.
      focused: () => setMenu((open) => void open?.done(null) as undefined),
    })
    return () => {
      pages.delete(handle)
      taking.delete(handle)
    }
  }, [handle])
  const own = plugin && !/^[a-z][a-z0-9+.-]*:/i.test(src)
  const entry = own ? entries.get(plugin) : undefined
  useEffect(() => {
    if (!entry || !plugin) return
    owners.set(handle, plugin)
    return () => void owners.delete(handle)
  }, [handle, entry, plugin])
  const drop = usePageDrops(handle, !!entry)
  const close = (index: number | null) => {
    if (!menu) return
    setMenu(undefined)
    const run = index === null ? undefined : menu.items[index]?.run
    if (run) engine.webview(handle, "eval", `window.__pumi&&__pumi.callback(${Number(run.ref)},[])`)
    menu.done(index)
  }
  return (
    <>
      <webview
        ref={drop}
        handle={handle}
        src={src}
        plugin={entry ? plugin : undefined}
        dir={entry?.dir}
        args={args}
        theme={theme}
        themeCss={entry ? css : undefined}
        background={background ?? theme.editor}
        keepKeys={keepKeys ?? []}
        zoom={zoom}
        autoFocus={autoFocus}
        style={{ flexGrow: 1, minWidth: 0, minHeight: 0, ...style }}
      />
      {menu ? <PageMenu at={menu.at} items={menu.items} close={close} /> : null}
    </>
  )
}

/** What a drag carries, as JSON can say it (the page gets a copy). */
function plain(data: unknown): unknown {
  try {
    return JSON.parse(JSON.stringify(data ?? null))
  } catch {
    return null
  }
}

/**
 * The app's drags over a plugin's page: each move over it goes in as `dragover` (`over`), leaving
 * it as `dragleave`, a release it takes as `drop`. It is a drop target only while the page says it
 * takes the drag (its `dragover` was prevented), so a drag it ignores falls to what is under it
 * (the pane's own split targets).
 */
function usePageDrops(handle: string, enabled: boolean) {
  const drag = useDragState()
  const inside = useRef(false)
  const { ref } = useDroppable({
    accept: (_data: unknown): _data is unknown => enabled && taking.get(handle) === true,
    onDrop: (data, at) => {
      inside.current = false
      taking.delete(handle)
      engine.webview(handle, "drag", { kind: "drop", x: at.x, y: at.y, data: plain(data) } as any)
    },
  })
  useEffect(() => {
    if (!enabled) return
    if (!drag) {
      if (inside.current) engine.webview(handle, "drag", { kind: "leave", x: 0, y: 0, data: null } as any)
      inside.current = false
      taking.delete(handle)
      return
    }
    const answer = engine.webview(handle, "drag", { kind: "over", x: drag.x, y: drag.y, data: plain(drag.data) } as any) as { inside?: boolean }
    if (inside.current && !answer.inside) {
      engine.webview(handle, "drag", { kind: "leave", x: drag.x, y: drag.y, data: null } as any)
      taking.delete(handle)
    }
    inside.current = !!answer.inside
  }, [handle, enabled, drag?.x, drag?.y, drag !== null])
  return ref
}

/** A page's menu, drawn by the app where the page asked, over it. */
function PageMenu({ at, items, close }: { at: { x: number; y: number }; items: PageItem[]; close(index: number | null): void }) {
  const t = useTheme()
  return (
    <anchored position={at} fit="snap" snapMargin={8} deferred occlude priority={1} style={{ backgroundColor: "transparent" }}>
      <div
        onMouseDownOutside={() => close(null)}
        style={{ display: "flex", flexDirection: "column", minWidth: 180, padding: 4, borderRadius: 8, backgroundColor: t.surface, borderWidth: 1, borderColor: t.border, boxShadow: { offsetX: 0, offsetY: 6, blurRadius: 18, spreadRadius: 0, color: "#00000066" } }}
      >
        {items.map((item, index) =>
          item.separator ? (
            <div key={index} style={{ height: 1, flexShrink: 0, marginTop: 4, marginBottom: 4, backgroundColor: t.border }} />
          ) : (
            <div
              key={index}
              title={item.detail}
              onClick={item.disabled ? undefined : () => close(index)}
              style={{ display: "flex", flexDirection: "row", alignItems: "center", gap: 8, height: 26, flexShrink: 0, paddingLeft: 8, paddingRight: 8, borderRadius: 5, opacity: item.disabled ? 0.45 : 1, cursor: item.disabled ? "default" : "pointer", ...(item.disabled ? {} : { hover: { backgroundColor: t.hover } }) }}
            >
              {item.icon ? <Icon name={item.icon} size={12} color={t.muted} /> : null}
              <text style={{ fontSize: 12, whiteSpace: "nowrap", textOverflow: "ellipsis", flexShrink: 1, minWidth: 0 }}>{item.title ?? ""}</text>
              {item.checked !== undefined ? <div style={{ flexGrow: 1, minWidth: 12 }} /> : null}
              {item.checked ? <Icon name="check" size={12} color={t.accent} /> : null}
            </div>
          ),
        )}
      </div>
    </anchored>
  )
}
