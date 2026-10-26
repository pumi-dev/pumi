// The terminal block's renderer. pumid runs the terminal (its program, its screen, its
// scrollback); `<terminal>` attaches to it, sizes the program to its box and takes the keys while
// it is focused. Any plugin can draw terminals the same way: turn this one off, or set
// `renderers` (`{ terminal: "me/term" }`) or a tab's `@tab_renderer`. The tab it is given is
// the SDK's model: its id is what `<terminal>` attaches to.
import type { RendererProps } from "pumi";

export function Terminal({ tab, focused }: RendererProps) {
  // No padding here: the terminal keeps its own (Settings' padding, Ghostty's `window-padding-*`)
  // and fills it, with the edge cells' colours when a program paints its whole screen.
  return <terminal tab={tab.id} autoFocus={focused} style={{ flexGrow: 1, minWidth: 0, minHeight: 0 }} />;
}
