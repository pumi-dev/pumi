// What the app draws over any theme's window, whatever the theme is: a question asked
// (`confirm`, `prompt`, in the theme's `Dialog`).
import { Dialogs } from "../sdk/dialogs.ts"
import "./actions.ts"

export function AppLayers() {
  return <Dialogs />
}
