// browser: a page in a tab. The app has the webview (`<WebView>`, `useWebview`); everything a
// browser is beyond it — the tab, its toolbar, its address bar, its settings, where a terminal's
// links go — is this plugin, bundled with pumi and turned off like any other.
//
//   pumi call pumi.browser.open '{"url":"https://pumi.dev"}'   # a page beside this one
//   pumi call pumi.browser.openView '{"url":"example.com"}'
//   pumi call pumi.browser.navigate %3 'example.com'           # the page in tab %3
//   pumi call pumi.browser.back %3
//
// The window's webview keeps the page's own state (`useWebview`: its address, its title,
// whether it is loading, whether it can go back), so the package keeps none of it: a navigable
// tab's `url` option is its address, a method of the tab is the block's own function
// (`pumi.browser.page.navigate`, `tab.call` of one reaching it), and what a caller reads of a
// tab is that option and the `title` it reports. What is here is what the app and other callers
// reach; `ui.tsx` draws the page.
import { ApiError, call, fn, option, self, ui, workspace } from "pumi";

import { resolve } from "./address.ts";

/** The tab option a page is on and goes to: the block's own (`pumi.browser.url`). */
const PAGE = "url";

/** Where a search goes when `search_url` cannot be read. */
const SEARCH = "https://duckduckgo.com/?q={query}";

/** The block a page is in: `pumi.browser.page`. */
const BLOCK = self.plugin.prefixed("page");

/** The topic the page hears its block's own methods on, in whichever window draws it. */
const TOPIC = "page";

/** Where a page opens: beside a tab, or in a view of its own. */
type Place = Record<string, unknown>;

/** The address a call gives: the second argument, or the first's `url`. */
function addressOf(first: unknown, second: unknown): string {
  const url = typeof first === "object" && first !== null ? (first as { url?: unknown }).url : second;
  if (typeof url !== "string" || url.trim() === "") throw new ApiError("invalid_params", "navigate takes the tab and the address");
  return url;
}

/** The tab a call names — `back("%3")`, `back({ tab: "%3" })` or `back(3)`, as `pumi call` writes
 * each of them — as its wire id. */
function tabOf(what: unknown): string {
  const named = typeof what === "object" && what !== null ? (what as { tab?: unknown }).tab : what;
  const id = typeof named === "number" ? `%${named}` : typeof named === "string" ? (named.startsWith("%") ? named : `%${Number(named)}`) : "";
  if (!/^(?:[A-Za-z0-9._-]+\/)?%[1-9][0-9]*$/.test(id)) throw new ApiError("not_found", `no browser page in tab ${String(named)}`);
  return id;
}

/** One of the plugin's global options (`home_url`): the manifest declares them, Settings sets them. */
async function setting(name: string): Promise<string | null> {
  const value = await option.get(name, { scope: "global" }).catch(() => null);
  return typeof value === "string" ? value : null;
}

/** What `open` is given: an address, or `{ url }`; the app adds `tab` and `link` for a terminal's
 * cmd+clicked link (`shift` when shift was held too). */
interface Opening {
  url?: unknown;
  tab?: unknown;
  link?: unknown;
  shift?: unknown;
}

/** A page in a tab: beside this one, or in a view of its own. Its address is the argument
 * (`{ url }`), else `home_url`, else the address bar. A terminal's link (`link: true`) opens beside
 * its terminal, or in the system's browser when `open_links` says so (shift says the other), as
 * does a link no page can be (`mailto:`). */
async function open(args: unknown, place: Place): Promise<string | null> {
  const given: Opening = typeof args === "string" ? { url: args } : ((args ?? {}) as Opening);
  const asked = typeof given.url === "string" ? given.url : "";
  if (given.link === true && asked !== "") {
    const system = ((await setting("open_links")) === "system") !== (given.shift === true);
    if (system || resolve(asked, (await setting("search_url")) ?? SEARCH)?.kind === "system") {
      await call("fs.open", asked);
      return null;
    }
  }
  const url = asked !== "" ? asked : ((await setting("home_url")) ?? "");
  const beside = typeof given.tab === "string" && given.tab !== "" ? { beside: given.tab } : {};
  // The block's option, set as it opens: the page the tab starts on.
  const opened = await workspace.openTab(BLOCK, { [self.plugin.prefixed(PAGE)]: url }, { ...place, ...beside });
  return opened.id;
}

/** An option of the page in `id`, as the page left it. */
async function said(id: string, name: string): Promise<string> {
  const value = await option.get(name, { scope: "tab", id }).catch(() => null);
  return typeof value === "string" ? value : "";
}

/** Tells the page in `id` to do `method`: what only the window's webview can do (go back, reload,
 * take what was typed). The tab's `url` option is what survives it. */
function tell(id: string, method: string, args?: unknown): null {
  ui.post(TOPIC, { tab: id, method, args: args ?? {} });
  return null;
}

/** The page in `id` goes to `url`, and answers where it went. */
async function navigate(id: string, url: string): Promise<string> {
  await option.set(PAGE, url, { scope: "tab", id });
  return url;
}

fn.set(
  "open",
  {
    desc: "Open a page beside this tab (or `tab`). With `link: true` it is a terminal's link, which `open_links` may send to the system's browser instead.",
    effect: "write",
    examples: [{ args: [{ url: "https://pumi.dev" }] }, { args: ["example.com"] }],
    returns: "The tab it opened, or `null` when the system's browser took the link.",
    returns_schema: { type: ["string", "null"] },
  },
  (args?: unknown) => open(args, { kind: "tiled", split: "right" }),
);
fn.set("openView", { desc: "Open a page in a view of its own.", effect: "write", returns_schema: { type: ["string", "null"] } }, (args?: unknown) =>
  open(args, { kind: "view", name: "Browser" }),
);

// The block's own methods: `tab.call("%3", "back")` reaches `pumi.browser.page.back` with the
// tab's id first, then the call's arguments.

/** Show a page in a browser tab: it takes what an address bar takes, a URL or a search. */
async function go(first: unknown, second: unknown): Promise<string> {
  const id = tabOf(first);
  const url = addressOf(first, second);
  return navigate(id, url);
}

fn.set(
  "page.navigate",
  {
    desc: "Show a page in a browser tab: it takes what an address bar takes, a URL or a search.",
    effect: "write",
    parameters: [
      ["string", "tab"],
      ["table", "args"],
    ],
    returns_schema: { type: "string" },
  },
  (tab: unknown, args: unknown) => go(tab, args),
);
fn.set(
  "page.back",
  { desc: "Step back in a page's history, as `cmd+[` does.", effect: "write", parameters: [["string", "tab"]] },
  (tab: unknown) => tell(tabOf(tab), "back"),
);
fn.set(
  "page.forward",
  { desc: "Step forward in a page's history, as `cmd+]` does.", effect: "write", parameters: [["string", "tab"]] },
  (tab: unknown) => tell(tabOf(tab), "forward"),
);
fn.set(
  "page.reload",
  { desc: "Load the page again, as `cmd+r` does.", effect: "write", parameters: [["string", "tab"]] },
  (tab: unknown) => tell(tabOf(tab), "reload"),
);
fn.set(
  "page.input",
  {
    desc: "What is typed into a page's address bar: `{text}`, `{paste}`, or `{keys}` with Enter among them.",
    effect: "write",
    parameters: [
      ["string", "tab"],
      ["table", "args"],
    ],
  },
  (tab: unknown, args: unknown) => tell(tabOf(tab), "input", args),
);
fn.set(
  "page.describe",
  { desc: "What the page says of itself: its title, its address, and the text it shows.", effect: "read", parameters: [["string", "tab"]] },
  (tab: unknown) => said(tabOf(tab), PAGE),
);

// What a caller of the package asks: the same methods, by the names a script knows.

fn.set(
  "navigate",
  {
    desc: "Show a page in a browser tab: it takes what an address bar takes, a URL or a search.",
    effect: "write",
    parameters: [
      ["number", "tab"],
      ["string", "url"],
    ],
    examples: [{ args: [3, "pumi.dev"] }, { args: [{ tab: 3 }, "pumi.dev"] }],
    returns: "The address it went to.",
    returns_schema: { type: "string" },
  },
  async (first: unknown, second: unknown) => go(first, second),
);
fn.set(
  "back",
  {
    desc: "Step back in a page's history, as `cmd+[` does.",
    effect: "write",
    parameters: [["number", "tab"]],
    examples: [{ args: [3] }, { args: [{ tab: 3 }] }],
    returns: "The address it is on now.",
    returns_schema: { type: "string" },
  },
  async (first: unknown) => {
    const id = tabOf(first);
    tell(id, "back");
    return said(id, PAGE);
  },
);
fn.set(
  "forward",
  {
    desc: "Step forward in a page's history, as `cmd+]` does.",
    effect: "write",
    parameters: [["number", "tab"]],
    examples: [{ args: [3] }, { args: [{ tab: 3 }] }],
    returns: "The address it is on now.",
    returns_schema: { type: "string" },
  },
  async (first: unknown) => {
    const id = tabOf(first);
    tell(id, "forward");
    return said(id, PAGE);
  },
);
fn.set(
  "reload",
  {
    desc: "Load the page again, as `cmd+r` does.",
    effect: "write",
    parameters: [["number", "tab"]],
    examples: [{ args: [3] }],
  },
  (first: unknown) => tell(tabOf(first), "reload"),
);
fn.set(
  "url",
  {
    desc: "The address a browser tab is on; empty for a tab that has not left the address bar.",
    effect: "read",
    parameters: [["number", "tab"]],
    examples: [{ args: [3] }],
    returns_schema: { type: "string" },
  },
  (first: unknown) => said(tabOf(first), PAGE),
);
fn.set(
  "title",
  {
    desc: "What a browser tab is called: the page's own title, else its host.",
    effect: "read",
    parameters: [["number", "tab"]],
    examples: [{ args: [3] }],
    returns_schema: { type: "string" },
  },
  (first: unknown) => said(tabOf(first), "title"),
);
