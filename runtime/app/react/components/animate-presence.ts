import { createContext, useCallback, useContext, useId, useLayoutEffect } from "react"

export interface PresenceContextValue {
  id: string
  isPresent: boolean
  initial: false | undefined
  onExitComplete: (childId: string) => void
  register: (childId: string) => () => void
}

export const PresenceContext = createContext<PresenceContextValue | null>(null)

export type SafeToRemove = () => void

export function usePresence(): [true] | [true, null] | [false, SafeToRemove] {
  const context = useContext(PresenceContext)
  const id = useId()
  const register = context?.register
  useLayoutEffect(() => {
    if (!register) return
    return register(id)
  }, [id, register])

  const onExitComplete = context?.onExitComplete
  const safeToRemove = useCallback(() => {
    onExitComplete?.(id)
  }, [id, onExitComplete])

  if (context === null) return [true, null]
  return !context.isPresent ? [false, safeToRemove] : [true]
}

export function useIsPresent(): boolean {
  const context = useContext(PresenceContext)
  return context === null ? true : context.isPresent
}
