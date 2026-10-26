// Questions over the window, drawn by the theme's `Dialog` part: `confirm` (go ahead or not) and
// `prompt` (a name), each a promise of the answer. What ends a whole tree (a session's every view
// and program) asks first with one; a rename nothing on screen can take in place asks with the
// other. One at a time, in the window in front.
import { createElement, useState, useSyncExternalStore } from "react"
import { useWindow } from "../app/state.ts"
import { useHoldKeys } from "../app/commands.ts"
import { useComponents } from "./theme.ts"

export interface ConfirmOptions {
  title: string
  detail?: string
  /** The button that goes ahead (`OK`). */
  confirm?: string
  cancel?: string
  /** Going ahead ends something: the theme draws it as a warning. */
  danger?: boolean
}

export interface PromptOptions extends ConfirmOptions {
  /** What the field holds to start with. */
  value?: string
  placeholder?: string
}

interface Request {
  options: PromptOptions
  input: boolean
  done(answer: string | undefined): void
}

let queue: Request[] = []
const listeners = new Set<() => void>()

function changed() {
  for (const l of listeners) l()
}

function ask(options: PromptOptions, input: boolean): Promise<string | undefined> {
  return new Promise((resolve) => {
    queue = [...queue, { options, input, done: resolve }]
    changed()
  })
}

/** Asks whether to go ahead: true when the user does. */
export async function confirm(options: ConfirmOptions): Promise<boolean> {
  return (await ask(options, false)) !== undefined
}

/** Asks for a line of text: what the user gave, or nothing when they cancelled. */
export function prompt(options: PromptOptions): Promise<string | undefined> {
  return ask(options, true)
}

function answer(request: Request, value: string | undefined) {
  queue = queue.filter((r) => r !== request)
  changed()
  request.done(value)
}

const subscribe = (l: () => void) => {
  listeners.add(l)
  return () => {
    listeners.delete(l)
  }
}

/** The question asked first, while there is one: the app draws it over the theme's window. */
export function Dialogs() {
  const window = useWindow()
  const first = useSyncExternalStore(subscribe, () => queue[0])
  // In the window in front; another draws nothing (the first window when none is in front).
  if (!first || !window.active) return null
  return createElement(Asking, { key: queue.indexOf(first), request: first })
}

function Asking({ request }: { request: Request }) {
  const C = useComponents()
  const [value, setValue] = useState(request.options.value ?? "")
  useHoldKeys()
  const o = request.options
  return createElement(C.Dialog, {
    title: o.title,
    detail: o.detail,
    confirm: o.confirm ?? "OK",
    cancel: o.cancel ?? "Cancel",
    danger: o.danger,
    input: request.input ? { value, placeholder: o.placeholder, onChange: setValue } : undefined,
    onConfirm: () => answer(request, request.input ? value : ""),
    onCancel: () => answer(request, undefined),
  })
}
