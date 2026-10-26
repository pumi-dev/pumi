// Haptic feedback, for a component's own controls (the settings kit plays it as a switch turns and a
// slider steps). A Force Touch trackpad taps; a mouse, or the theme's `haptics` setting off (pumi's
// own theme's is the `haptics` option), does nothing.
// Apple's HIG: confirm an alignment or a change of level, not a plain click.
import { engine } from "../app/engine.ts"
import { hapticsOn } from "../app/theme-settings.ts"

export type HapticPattern = "generic" | "alignment" | "levelChange"

/** Plays `pattern` now, unless the theme's `haptics` setting is off. */
export function haptic(pattern: HapticPattern = "generic"): void {
  if (!hapticsOn()) return
  try {
    ;(engine as { haptic?: (pattern: string) => void }).haptic?.(pattern)
  } catch (err) {
    // A haptic is a nicety: one the platform refused is logged, and the control goes on.
    console.error("pumi: haptic:", (err as Error)?.message ?? err)
  }
}
