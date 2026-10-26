/** Shared state, slot, and positioning helpers for headless floating controls. */

import React, {
  cloneElement,
  createContext,
  forwardRef,
  isValidElement,
  useCallback,
  useContext,
  useLayoutEffect,
  useMemo,
  useRef,
  useState,
} from "react"
import type { ReactElement, ReactNode, Ref } from "react"
import type { EventPayload } from "../../native/addon"
import { pushDismissLayer } from "../../native/host"
import type {
  DismissLayer,
  KeyEvent,
  Props,
  PublicInstance,
  StyleDesc,
} from "../types/host.js"
import { useGpuix } from "../hooks/use-gpuix.js"

export type FloatingSide = "top" | "right" | "bottom" | "left"
export type FloatingAlign = "start" | "center" | "end"
export type StateStyle<State> = StyleDesc | ((state: State) => StyleDesc)

export interface FloatingContentProps extends Omit<Props, "children"> {
  children?: ReactNode
  side?: FloatingSide
  sideOffset?: number
  align?: FloatingAlign
  alignOffset?: number
  collisionPadding?: number
}

export function resolveStyle<State>(
  style: StateStyle<State> | undefined,
  state: State
): StyleDesc | undefined {
  return typeof style === "function" ? style(state) : style
}

export function mergeStyles(
  base: StyleDesc | undefined,
  override: StyleDesc | undefined
): StyleDesc | undefined {
  if (!base) return override
  if (!override) return base
  return { ...base, ...override }
}

export function floatingRootStyle(style?: StyleDesc): StyleDesc {
  return {
    display: "flex",
    position: "relative",
    alignItems: "start",
    ...style,
  }
}

type InteractiveStyle = Omit<StyleDesc, "hover" | "active">

function floatingSurfaceStateStyle(style?: InteractiveStyle): InteractiveStyle | undefined {
  if (!style) return undefined
  const surface: InteractiveStyle = {}
  if (style.visibility !== undefined) surface.visibility = style.visibility
  if (style.opacity !== undefined) surface.opacity = style.opacity
  if (style.borderRadius !== undefined) surface.borderRadius = style.borderRadius
  if (style.borderTopLeftRadius !== undefined) {
    surface.borderTopLeftRadius = style.borderTopLeftRadius
  }
  if (style.borderTopRightRadius !== undefined) {
    surface.borderTopRightRadius = style.borderTopRightRadius
  }
  if (style.borderBottomRightRadius !== undefined) {
    surface.borderBottomRightRadius = style.borderBottomRightRadius
  }
  if (style.borderBottomLeftRadius !== undefined) {
    surface.borderBottomLeftRadius = style.borderBottomLeftRadius
  }
  return Object.keys(surface).length > 0 ? surface : undefined
}

function floatingSurfaceStyle(style?: StyleDesc): StyleDesc {
  const surface: StyleDesc = floatingSurfaceStateStyle(style) ?? {}
  const hover = floatingSurfaceStateStyle(style?.hover)
  const active = floatingSurfaceStateStyle(style?.active)
  if (hover) surface.hover = hover
  if (active) surface.active = active
  return surface
}

function withoutOpacity(style?: InteractiveStyle): InteractiveStyle | undefined {
  if (!style) return undefined
  const { opacity: _opacity, ...rest } = style
  return rest
}

function floatingContentStyle(style?: StyleDesc): StyleDesc | undefined {
  if (!style) return undefined
  const { opacity: _opacity, hover, active, ...rest } = style
  return {
    ...rest,
    hover: withoutOpacity(hover),
    active: withoutOpacity(active),
  }
}

export function useControllableState<Value>({
  value,
  defaultValue,
  onChange,
}: {
  value: Value | undefined
  defaultValue: Value
  onChange?: (value: Value) => void
}): [Value, (value: Value) => void] {
  const [internalValue, setInternalValue] = useState(defaultValue)
  const controlled = value !== undefined
  const currentValue = controlled ? value : internalValue
  const setValue = useCallback(
    (nextValue: Value) => {
      if (!controlled) setInternalValue(nextValue)
      if (!Object.is(currentValue, nextValue)) onChange?.(nextValue)
    },
    [controlled, currentValue, onChange]
  )
  return [currentValue, setValue]
}

const DismissLayerContext = createContext<DismissLayer | undefined>(undefined)

export interface DismissableLayerProps {
  children?: ReactNode
  /** Escape reached the window, this layer is on top, and nothing prevented it. */
  onEscapeKeyDown: (event: KeyEvent) => void
  /** Element to focus when the layer opens on top. Null leaves focus. */
  initialFocus?: () => number | null
  /** Element to focus on close. `previous` is the focus from before it opened. */
  finalFocus?: (previous: number | null) => number | null
}

/**
 * Put an open overlay on the window's layer stack while mounted. Escape closes
 * only the top layer. Layers inside this one, such as a Select in a Dialog,
 * are always above it. The stack also moves focus in and out, in stack order.
 */
export function DismissableLayer({ children, ...handlers }: DismissableLayerProps): ReactElement {
  const { renderer } = useGpuix()
  const parent = useContext(DismissLayerContext)
  // Read during render, before this layer's elements are committed. After the
  // commit an autoFocus inside the layer may already hold focus.
  const [previousFocus] = useState(() => renderer?.getFocusedElementId?.() ?? null)
  const latest = useRef(handlers)
  latest.current = handlers
  const layer = useMemo<DismissLayer>(
    () => ({
      parent,
      onEscapeKeyDown: (event) => latest.current.onEscapeKeyDown(event),
      initialFocus: () => latest.current.initialFocus?.() ?? null,
      finalFocus: (previous) => latest.current.finalFocus?.(previous) ?? null,
    }),
    [parent]
  )
  useLayoutEffect(() => {
    if (!renderer) return
    return pushDismissLayer(renderer, layer, { previousFocus })
  }, [renderer, layer, previousFocus])
  return <DismissLayerContext.Provider value={layer}>{children}</DismissLayerContext.Provider>
}

export function setRefs<T>(value: T, ...refs: Array<Ref<T> | undefined>): void {
  for (const ref of refs) {
    if (typeof ref === "function") {
      ref(value)
    } else if (ref) {
      ref.current = value
    }
  }
}

function mergeRefs<T>(...refs: Array<Ref<T> | undefined>): (value: T) => void {
  return (value) => {
    for (const ref of refs) {
      if (typeof ref === "function") {
        ref(value)
      } else if (ref) {
        ref.current = value
      }
    }
  }
}

function getElementRef(element: ReactElement<Props>): Ref<PublicInstance> | undefined {
  if (element.props.ref) return element.props.ref
  const descriptor = Object.getOwnPropertyDescriptor(element, "ref")
  return descriptor?.value
}

function composeHandlers<Event extends EventPayload>(
  first?: (event: Event) => void,
  second?: (event: Event) => void
): ((event: Event) => void) | undefined {
  if (!first) return second
  if (!second) return first
  return (event) => {
    first(event)
    second(event)
  }
}

/**
 * Render `props` on a `div`, or merge them into the single `asChild` element.
 *
 * `defaultTabIndex` makes the part a tab stop, like the `<button>` Base UI
 * renders. An explicit `tabIndex` on the part, then on the child, wins.
 */
export function renderSlot({
  asChild,
  children,
  props,
  ref,
  defaultTabIndex,
}: {
  asChild?: boolean
  children: ReactNode
  props: Props
  ref?: Ref<PublicInstance>
  defaultTabIndex?: number
}): ReactElement {
  if (!asChild) {
    return (
      <div {...props} tabIndex={props.tabIndex ?? defaultTabIndex} ref={ref}>
        {children}
      </div>
    )
  }
  if (!isValidElement<Props>(children)) {
    throw new Error("asChild requires exactly one React element")
  }

  const child = children
  const childProps = child.props
  const merged: Props = {
    ...childProps,
    ...props,
    style: mergeStyles(childProps.style, props.style),
    onClick: composeHandlers(childProps.onClick, props.onClick),
    onMouseDown: composeHandlers(childProps.onMouseDown, props.onMouseDown),
    onMouseUp: composeHandlers(childProps.onMouseUp, props.onMouseUp),
    onMouseEnter: composeHandlers(childProps.onMouseEnter, props.onMouseEnter),
    onMouseLeave: composeHandlers(childProps.onMouseLeave, props.onMouseLeave),
    onMouseMove: composeHandlers(childProps.onMouseMove, props.onMouseMove),
    onMouseDownOutside: composeHandlers(
      childProps.onMouseDownOutside,
      props.onMouseDownOutside
    ),
    onKeyDown: composeHandlers(childProps.onKeyDown, props.onKeyDown),
    onKeyUp: composeHandlers(childProps.onKeyUp, props.onKeyUp),
    onFocus: composeHandlers(childProps.onFocus, props.onFocus),
    onBlur: composeHandlers(childProps.onBlur, props.onBlur),
    onScroll: composeHandlers(childProps.onScroll, props.onScroll),
    onChange: composeHandlers(childProps.onChange, props.onChange),
    onSubmit: composeHandlers(childProps.onSubmit, props.onSubmit),
  }
  merged.tabIndex = props.tabIndex ?? childProps.tabIndex ?? defaultTabIndex
  const childRef = getElementRef(child)
  if (childRef || ref) merged.ref = mergeRefs(childRef, ref)
  return cloneElement(child, merged)
}

export const FloatingLayer = forwardRef<PublicInstance, FloatingContentProps>(
  function FloatingLayer(
    {
      side = "bottom",
      sideOffset = 0,
      align = "start",
      alignOffset = 0,
      collisionPadding = 8,
      children,
      ...props
    },
    ref
  ) {
    const offset =
      side === "top" || side === "bottom"
        ? { x: alignOffset, y: 0 }
        : { x: 0, y: alignOffset }

    return (
      <anchored
        style={floatingSurfaceStyle(props.style)}
        side={side}
        align={align}
        gap={sideOffset}
        offset={offset}
        fit="snap"
        snapMargin={collisionPadding}
        deferred
        priority={1}
        occlude={props.style?.pointerEvents !== "none"}
      >
        <div
          {...props}
          ref={ref}
          style={mergeStyles(
            { backgroundColor: "#1A1A1A" },
            floatingContentStyle(props.style)
          )}
        >
          {children}
        </div>
      </anchored>
    )
  }
)
