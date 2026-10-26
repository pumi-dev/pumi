// What the address bar makes of what is typed: a page to load, a link the system opens, or
// nothing at all: a URL, a host typed bare (`example.com`, `localhost:3000`) as an address, and
// anything else as a search of the `search_url` template, `{query}` where the text goes.

/** Where a line typed in the address bar goes: a page in the pane, or a link the OS opens. */
export type Target = { kind: "web"; url: string } | { kind: "system"; url: string };

/** The schemes a page can be. Every other one is a link (`mailto:`, `slack:`) or refused. */
const WEB = new Set(["http", "https", "file", "about", "data"]);

/** The hosts that are an address without a scheme, as a terminal is used to. */
const LOCAL = ["localhost", "127.0.0.1", "[::1]"];

/** The scheme `input` starts with and what follows its colon, when it starts with one: `app` and
 *  `//settings` for `app://settings`. A scheme is letters, digits, `+`, `.` or `-`, and starts
 *  with a letter. */
function schemeOf(input: string): [string, string] | null {
  const at = input.indexOf(":");
  if (at <= 0) return null;
  const scheme = input.slice(0, at);
  if (!/^[A-Za-z][A-Za-z0-9+.-]*$/.test(scheme)) return null;
  return [scheme.toLowerCase(), input.slice(at + 1)];
}

/** Whether what follows a scheme is a port and a path (`:3000/app`), which makes the line an
 *  address with a host rather than a link of another scheme. */
function ported(rest: string): boolean {
  const port = rest.split(/[/?#]/)[0] ?? "";
  return port !== "" && /^[0-9]+$/.test(port);
}

/** Where what is typed goes: `search` is the template a search goes to, `{query}` where the text
 *  goes (`https://duckduckgo.com/?q={query}`), and `null` is nothing to do. */
export function resolve(input: string, search: string): Target | null {
  const text = input.trim();
  if (text === "") return null;
  const lower = text.toLowerCase();
  const scheme = schemeOf(text);
  const words = /\s/.test(text);
  if (scheme?.[0] === "javascript") return null;
  if (scheme && WEB.has(scheme[0])) return { kind: "web", url: text };
  // `mailto:a@b.c` is the system's, but `devbox:8000` is this machine's devbox.
  if (scheme && !ported(scheme[1]) && !words) return { kind: "system", url: text };
  const local = LOCAL.some((host) => lower.startsWith(host) && /^$|^[:/?#]/.test(lower.slice(host.length)));
  if (!words && (local || scheme !== null)) return { kind: "web", url: `http://${text}` };
  if (!words && text.includes(".")) return { kind: "web", url: `https://${text}` };
  return { kind: "web", url: search.replace("{query}", percentEncode(text)) };
}

/** `text` as a URL's query takes it: the unreserved characters as they are, a space as `+`, and
 *  everything else as `%XX` of its bytes. */
export function percentEncode(text: string): string {
  let out = "";
  for (const byte of new TextEncoder().encode(text)) {
    const char = String.fromCharCode(byte);
    if (/[A-Za-z0-9\-_.~]/.test(char)) out += char;
    else if (char === " ") out += "+";
    else out += `%${byte.toString(16).toUpperCase().padStart(2, "0")}`;
  }
  return out;
}

/** `url` without what does not change the page: the `#` at its end, the case of its scheme and
 *  host, and a path at all (`HTTPS://Example.COM` and `https://example.com/` are one page). */
export function normalize(url: string): string {
  let text = url.trim();
  if (text.endsWith("#")) text = text.slice(0, -1);
  const at = text.indexOf("://");
  if (at < 0) return text;
  const scheme = text.slice(0, at).toLowerCase();
  const rest = text.slice(at + 3);
  const stop = rest.search(/[/?#]/);
  const host = (stop < 0 ? rest : rest.slice(0, stop)).toLowerCase();
  let path = stop < 0 ? "" : rest.slice(stop);
  if (path === "" || path.startsWith("?") || path.startsWith("#")) path = `/${path}`;
  return `${scheme}://${host}${path}`;
}

/** The host `url` is on, which names a page whose title it was not told:
 *  `https://a.dev/b?c` is `a.dev`, and `about:blank` is nothing. */
export function hostOf(url: string): string {
  const at = url.indexOf("://");
  if (at < 0) return "";
  const rest = url.slice(at + 3);
  const stop = rest.search(/[/?#]/);
  return stop < 0 ? rest : rest.slice(0, stop);
}
