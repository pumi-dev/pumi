// GPUIX component definitions and native motion wrappers.

import { createElement, forwardRef, useContext, useEffect, useMemo } from "react"
import type { ReactElement, ReactNode } from "react"
import type { MotionProps, Props, PublicInstance, StyleDesc } from "../types/host.js"
import { PresenceContext, usePresence } from "./animate-presence.js"

let nextMotionGeneration = 0

/** What is said of an endless animation slowed for what its frames cost (`onSlowLoop`). */
export function slowLoopWarning(compartment: string | undefined): string {
  return `pumi: an endless animation${compartment ? ` in ${compartment}` : ""} is shown at a lower rate: each frame it moves in draws too much of the window again. Put the rows or boxes around it in a \`compartment\` (or \`cache\`) of their own.`
}

let slowLoop = (compartment: string | undefined) => console.warn(slowLoopWarning(compartment))

/**
 * Who is told when an endless animation is sampled less often for what its frames cost (the
 * renderer's budget): the name of the compartment it is drawn in, if it is in a named one. Once
 * for each animation. Gives back what puts the one before it back.
 */
export function onSlowLoop(listener: (compartment: string | undefined) => void): () => void {
  const before = slowLoop
  slowLoop = listener
  return () => {
    if (slowLoop === listener) slowLoop = before
  }
}

/** `Infinity` does not survive JSON (it becomes `null`): Rust reads `"infinite"`. */
function wireTransition(transition: MotionProps["transition"]): MotionProps["transition"] {
  if (!transition || transition.repeat !== Infinity) return transition
  return { ...transition, repeat: "infinite" }
}

export { PresenceContext, useIsPresent, usePresence } from "./animate-presence.js"

export const gpuixComponents = {
  div: "div",
  text: "text",
  img: "img",
  svg: "svg",
  canvas: "canvas",
  input: "input",
  textarea: "textarea",
  anchored: "anchored",
  "virtual-list": "virtual-list",
} as const

export type GpuixComponentType = keyof typeof gpuixComponents

export interface MotionDivProps extends MotionProps {
  title?: Props["title"]
  onDoubleClick?: Props["onDoubleClick"]
  onContextMenu?: Props["onContextMenu"]
  children?: ReactNode
  style?: StyleDesc
  onClick?: Props["onClick"]
  onMouseDown?: Props["onMouseDown"]
  onMouseUp?: Props["onMouseUp"]
  onMouseEnter?: Props["onMouseEnter"]
  onMouseLeave?: Props["onMouseLeave"]
  onMouseMove?: Props["onMouseMove"]
  onMouseDownOutside?: Props["onMouseDownOutside"]
  onKeyDown?: Props["onKeyDown"]
  onKeyUp?: Props["onKeyUp"]
  onFocus?: Props["onFocus"]
  onBlur?: Props["onBlur"]
  onScroll?: Props["onScroll"]
  onFileDrop?: Props["onFileDrop"]
  onMotionComplete?: Props["onMotionComplete"]
  autoFocus?: boolean
}

/** A host element whose `motion` the renderer plays natively. */
function motionComponent<P extends object>(type: string) {
  return forwardRef<PublicInstance, P & MotionProps & { onMotionComplete?: Props["onMotionComplete"] }>(function Motion(
  { initial, animate, exit, transition, onMotionComplete, ...props },
  ref
): ReactElement {
  const presence = useContext(PresenceContext)
  const [isPresent, safeToRemove] = usePresence()
  const resolvedInitial = presence?.initial === false ? false : initial
  const resolvedAnimate = !isPresent && exit ? exit : animate

  useEffect(() => {
    if (!isPresent && !exit) safeToRemove?.()
  }, [exit, isPresent, safeToRemove])

  const motionKey = JSON.stringify([isPresent, resolvedInitial, resolvedAnimate, wireTransition(transition)])
  const generation = useMemo(() => ++nextMotionGeneration, [motionKey])

  const motionDescription = {
    generation,
    isExit: !isPresent && exit !== undefined,
    initial: resolvedInitial,
    animate: resolvedAnimate,
    transition: wireTransition(transition),
  }
  const hostProps: Props = {
    ...props,
    ref,
    motion: motionDescription,
    onMotionThrottled: (event) => slowLoop(event.value ?? undefined),
  }
  if (!isPresent || onMotionComplete) {
    hostProps.onMotionComplete = (event) => {
      if (event.motionGeneration !== generation) return
      onMotionComplete?.(event)
      if (!isPresent) safeToRemove?.()
    }
  }
  return createElement(type, hostProps)
})
}

const MotionDiv = motionComponent<MotionDivProps>("div")

/** Native animations with a Motion-like declarative React API: `motion.div`, `motion.text`,
 *  `motion.img`, `motion.svg`. Keyframes, repeats, springs and colours play in Rust. */
export const motion = {
  div: MotionDiv,
  text: motionComponent<MotionDivProps>("text"),
  img: motionComponent<Omit<import("../types/host.js").ImgProps, "motion">>("img"),
  svg: motionComponent<Omit<import("../types/host.js").SvgProps, "motion">>("svg"),
} as const

// There is no `VirtualList` React wrapper. Windowing on the React side is the
// app's job: pass `itemCount`, `estimatedItemHeight` and `windowStart` to the
// host `<virtual-list>` and render only that slice. A generic wrapper cannot
// know when to widen its own window.
