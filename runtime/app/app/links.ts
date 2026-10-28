// A terminal's cmd+clicked link (the engine's `terminal` event, `openUrl`). The app has no browser:
// the link goes to the browser plugin's `pumi_dev.browser.open` (pumi-dev/plugins/browser), which
// opens it beside the terminal or in the system's browser as its `open_links` says. Without that
// plugin its function is not there, and the link opens in the system's browser.
import { call, on } from "./engine.ts"

/** The function a link goes to: the browser plugin's (`pumi_dev/browser`), while it is installed and on. */
export const BROWSER_OPEN = "pumi_dev.browser.open"

/** What the engine says of a terminal's click: a link (`openUrl`, `external` with shift held). */
interface Clicked {
  tab?: string
  openUrl?: string
  external?: boolean
}

/** Where `url` goes: to `open` (the browser plugin's function), else to the system's browser. */
export async function openLink(url: string, tab: string | undefined, shift: boolean, open = call): Promise<void> {
  try {
    await open(BROWSER_OPEN, { url, tab, link: true, shift })
  } catch {
    // The plugin is off (no such function) or failed: the system's browser has it.
    await open("fs.open", url).catch((err: Error) => console.error(`pumi: opening ${url}:`, err?.message ?? err))
  }
}

/** Hears terminals' links from now on. */
export function start() {
  on("terminal", (data: Clicked) => {
    if (typeof data?.openUrl === "string" && data.openUrl !== "") void openLink(data.openUrl, data.tab, data.external === true)
  })
}
