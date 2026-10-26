// What a settings component is drawn inside: the options there are and their values, how one is set,
// and what a search or a link asks to show. Settings gives it; a kit control drawn anywhere else
// (a plugin's own panel) reads and sets the options itself.
import { createContext, useContext } from "react"
import type { Decl, OptionValue } from "./options.ts"

export interface SettingsKit {
  /** Every option Settings shows (`settable`), in the order they were declared. */
  decls: Decl[]
  decl(name: string): Decl | undefined
  /** Its value: its own, else its default. */
  value(name: string): OptionValue
  /** It has a value of its own (one a reset takes away). */
  isSet(name: string): boolean
  /** Sets it globally and keeps it in settings.json; `null` resets it to its default. */
  set(name: string, value: OptionValue): void
  /** The option or section a link or a search hit names: its row is highlighted and scrolled to. */
  highlight?: string
  /** What the search finds, lowercased; rows that do not match it are drawn as usual. */
  query?: string
  /** Shows a page (`general`, a plugin's), and the row or section `link` names on it. */
  open(page: string, link?: string): void
  /** Says something went wrong (a value that does not parse), at the foot of the page. */
  report(message: string | undefined): void
}

export const SettingsContext = createContext<SettingsKit | null>(null)

/** The Settings a component is drawn in, if it is in one. */
export function useSettings(): SettingsKit | null {
  return useContext(SettingsContext)
}
