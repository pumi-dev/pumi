// `openSettings(page, link)`: Settings in front, at a page (`general`, `appearance`, `keys`,
// `plugins`, a plugin's) and the option or section `link` names (`me.remote.tailscale`). The
// window registers itself here as the app starts (`settings/window.tsx`), so a plugin's component
// and the commands reach it without importing the window.
type Opener = (page?: string, link?: string) => void

let opener: Opener | undefined
let waiting: Parameters<Opener> | undefined

export function setOpener(open: Opener) {
  opener = open
  if (waiting) open(...waiting)
  waiting = undefined
}

/** Opens Settings (or brings it to the front), at `page`, and scrolls to what `link` names. */
export function openSettings(page?: string, link?: string): void {
  if (opener) opener(page, link)
  else waiting = [page, link]
}
