// A web page's look, from the app's theme: `themeCss` is its `--pumi-*` variables plus a base
// sheet (`@layer pumi`, so a page's own CSS wins) that `src/ui/webview_bridge.js` puts in the
// page's `<style id="pumi-theme">`.
import type { Appearance, Metrics, Tokens } from "./theme.ts"

const UI_FONT = `-apple-system, BlinkMacSystemFont, "Segoe UI", system-ui, Ubuntu, Cantarell, sans-serif`
const MONO_FONT = `ui-monospace, SFMono-Regular, Menlo, Consolas, "DejaVu Sans Mono", monospace`

/** `accentText` -> `accent-text`, `agent.working` -> `agent-working`: a token's name in CSS. */
function kebab(name: string): string {
  return name.replace(/\./g, "-").replace(/[A-Z]/g, (c) => `-${c.toLowerCase()}`)
}

/** `#rrggbbaa` of `color` at `alpha` (0–1); `#rgb` and `#rrggbb` take it too, and anything else
 *  (a page's own `rgb(…)`, say) is left as it is. */
function withAlpha(color: string, alpha: number): string {
  const a = Math.round(Math.min(1, Math.max(0, alpha)) * 255)
    .toString(16)
    .padStart(2, "0")
  if (/^#[0-9a-f]{3}$/i.test(color)) return `#${color.slice(1).replace(/./g, (c) => c + c)}${a}`
  if (/^#[0-9a-f]{6}$/i.test(color)) return `${color}${a}`
  if (/^#[0-9a-f]{8}$/i.test(color)) return `${color.slice(0, 7)}${a}`
  return color
}

/** The stylesheet for a plugin's page: every token and metric as a `--pumi-*` variable, and a base
 *  sheet a page can override, since it is all in `@layer pumi`. */
export function themeCss(tokens: Tokens, metrics: Metrics, appearance: Appearance): string {
  const vars = Object.entries(tokens).map(([name, value]) => `--pumi-${kebab(name)}: ${value};`)
  for (const [name, value] of Object.entries(metrics)) vars.push(`--pumi-${kebab(name)}: ${value}px;`)
  const thumb = withAlpha(tokens.faint, 0.5)
  return `@layer pumi {
  :root {
    ${vars.join("\n    ")}
    --pumi-font: ${UI_FONT};
    --pumi-font-mono: ${MONO_FONT};
    color-scheme: ${appearance};
  }
  :where(html, body) {
    margin: 0;
    background: transparent;
    color: var(--pumi-text);
    font-family: var(--pumi-font);
    font-size: var(--pumi-font-size);
    -webkit-font-smoothing: antialiased;
    -webkit-tap-highlight-color: transparent;
  }
  :where(html) {
    caret-color: var(--pumi-accent);
    scrollbar-color: ${thumb} transparent;
    scrollbar-width: thin;
  }
  ::selection {
    background: var(--pumi-selected);
    color: var(--pumi-text);
  }
  :where(:focus-visible) {
    outline: 2px solid var(--pumi-accent);
    outline-offset: -2px;
  }
  :where(a) {
    color: var(--pumi-accent);
  }
  :where(input, button, select, textarea) {
    color: inherit;
    font: inherit;
  }
  ::-webkit-scrollbar {
    width: 10px;
    height: 10px;
  }
  ::-webkit-scrollbar-track {
    background: transparent;
  }
  ::-webkit-scrollbar-corner {
    background: transparent;
  }
  ::-webkit-scrollbar-thumb {
    border: 2px solid transparent;
    border-radius: var(--pumi-radius);
    background: ${thumb};
    background-clip: content-box;
  }
  ::-webkit-scrollbar-thumb:hover {
    background: ${withAlpha(tokens.muted, 0.7)};
  }
}`
}
