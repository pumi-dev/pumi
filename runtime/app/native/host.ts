import type {
  DebugFrameOverlayStats,
  EdgeInsets,
  ElementBounds,
  EventPayload,
  GpuixRenderer,
  HighlightMatch,
  PathPromptOptions,
  WindowInsets as NativeWindowInsets,
} from "./addon"
import type { StyleDesc as WireStyle } from "./protocol.gen.js"

export type {
  DebugFrameOverlayStats,
  EdgeInsets,
  ElementBounds,
  HighlightMatch,
  NativeWindowInsets,
  PathPromptOptions,
}

export * from "./host-runtime.js"
export * from "./mutations.js"
export * from "./renderer-state.js"

export type DimensionValue = number | string

/** A value a track moves to, or keyframes it passes through (`null`: what is shown when it starts). */
export type MotionValue<T> = T | (T | null)[]

export interface MotionStyle {
  width?: MotionValue<number>
  height?: MotionValue<number>
  opacity?: MotionValue<number>
  top?: MotionValue<number>
  right?: MotionValue<number>
  bottom?: MotionValue<number>
  left?: MotionValue<number>
  borderRadius?: MotionValue<number>
  /** Moves the box from where layout put it, in px (an in-flow box becomes `relative`). */
  x?: MotionValue<number>
  y?: MotionValue<number>
  /** An `<svg>`'s turn about its centre, in degrees. */
  rotate?: MotionValue<number>
  backgroundColor?: MotionValue<string>
  color?: MotionValue<string>
  borderColor?: MotionValue<string>
}

export type MotionEase =
  | "linear"
  | "ease"
  | "easeIn"
  | "easeOut"
  | "easeInOut"
  | "circIn"
  | "circOut"
  | "circInOut"
  | "backIn"
  | "backOut"
  | "backInOut"
  | "stepStart"
  | "stepEnd"
  | [number, number, number, number]
  /** Jumps in `steps` equal steps (a blink, a sprite-like tick). */
  | { steps: number; position?: "start" | "end" }

export interface MotionTransition {
  /** Duration in seconds (a spring works out its own). */
  duration?: number
  /** Delay in seconds. */
  delay?: number
  /** For a tween; each keyframe segment is eased by it. */
  ease?: MotionEase
  /** `"spring"`: a physical spring (`stiffness`, `damping`, `mass`), which may overshoot. */
  type?: "tween" | "spring"
  stiffness?: number
  damping?: number
  mass?: number
  /** Plays again this many times after the first, or forever (`Infinity` / `"infinite"`). */
  repeat?: number | "infinite"
  /** `"loop"` starts over; `"reverse"` and `"mirror"` play back. */
  repeatType?: "loop" | "reverse" | "mirror"
  /** Seconds between repeats. */
  repeatDelay?: number
  /** Where each keyframe falls, 0 to 1, in order. Even by default. */
  times?: number[]
  /** Samples a second. A track that repeats forever is capped at 30 unless this says. */
  fps?: number
}

export interface MotionProps {
  initial?: MotionStyle | false
  animate: MotionStyle
  /** Target applied while this node is leaving the tree. */
  exit?: MotionStyle
  transition?: MotionTransition
}

/**
 * CSS `cursor` keywords GPUI can paint. An unlisted keyword is ignored, like
 * every other invalid style value.
 */
export type CursorValue =
  | "default"
  | "auto"
  | "pointer"
  | "text"
  | "vertical-text"
  | "crosshair"
  | "grab"
  | "grabbing"
  | "move"
  | "all-scroll"
  | "col-resize"
  | "row-resize"
  | "ew-resize"
  | "ns-resize"
  | "nwse-resize"
  | "nesw-resize"
  | "n-resize"
  | "e-resize"
  | "s-resize"
  | "w-resize"
  | "ne-resize"
  | "nw-resize"
  | "se-resize"
  | "sw-resize"
  | "not-allowed"
  | "no-drop"
  | "alias"
  | "copy"
  | "context-menu"

export interface BoxShadow {
  offsetX: number
  offsetY: number
  blurRadius: number
  spreadRadius: number
  color: string
  /** Draw inside the border box, like CSS `inset`. */
  inset?: boolean
}

export interface LinearGradientStop {
  color: string
  /** Position along the gradient from 0 to 1. Left out, stops spread evenly, as in CSS. */
  position?: number
}

/** Two stops paint natively; more are rasterised (smooth, up to 512 px a side). */
export interface LinearGradientBackground {
  type: "linear-gradient"
  /** CSS angle in degrees. 0 points up and values increase clockwise. 180 by default. */
  angle?: number
  stops: LinearGradientStop[]
  /** Only for two stops. */
  colorSpace?: "srgb" | "oklab"
}

export interface RadialGradientBackground {
  type: "radial-gradient"
  /** `"ellipse"` (default) keeps the box's proportions; `"circle"` is round. */
  shape?: "circle" | "ellipse"
  /** The centre as fractions of the box. `[0.5, 0.5]` by default. */
  center?: [number, number]
  /** In px: one radius, or `[x, y]`. The farthest corner by default. */
  radius?: number | [number, number]
  stops: LinearGradientStop[]
}

export interface ConicGradientBackground {
  type: "conic-gradient"
  /** Where the sweep starts, in degrees clockwise from the top. */
  angle?: number
  center?: [number, number]
  stops: LinearGradientStop[]
}

export type GradientBackground = LinearGradientBackground | RadialGradientBackground | ConicGradientBackground

/** A length in px, a percentage (`"50%"`) or a keyword. */
export type LengthValue = number | string

export type BackgroundSize = "auto" | "cover" | "contain" | number | [LengthValue, LengthValue]

/** `"center"`, `"top left"`, `"bottom 8px"`… or `[x, y]`. A percentage puts that point of the image on that point of the box. */
export type BackgroundPosition = string | [LengthValue, LengthValue]

export type BackgroundRepeat = "repeat" | "repeat-x" | "repeat-y" | "no-repeat"

/** One image layer: what it leaves out comes from the element's `background*` props. */
export interface BackgroundImageLayer {
  /** A path (an imported asset: `import floor from "./floor.png"`), a data URI or an http(s) URL. */
  src: string
  size?: BackgroundSize
  repeat?: BackgroundRepeat
  position?: BackgroundPosition
  opacity?: number
}

export interface StyleDesc {
  display?: string
  visibility?: string
  flexDirection?: string
  flexWrap?: string
  flexGrow?: number
  flexShrink?: number
  flexBasis?: number
  alignItems?: string
  alignSelf?: string
  alignContent?: string
  justifyContent?: string
  gap?: number
  rowGap?: number
  columnGap?: number

  width?: DimensionValue
  height?: DimensionValue
  minWidth?: DimensionValue
  minHeight?: DimensionValue
  maxWidth?: DimensionValue
  maxHeight?: DimensionValue

  padding?: number
  paddingTop?: number
  paddingRight?: number
  paddingBottom?: number
  paddingLeft?: number

  margin?: number
  marginTop?: number
  marginRight?: number
  marginBottom?: number
  marginLeft?: number

  position?: string
  /** An offset from the box's parent: px, or a share of it (`"25%"`), as a size is. */
  top?: DimensionValue
  right?: DimensionValue
  bottom?: DimensionValue
  left?: DimensionValue
  /** Paint (and hit-test) order among siblings: higher is on top. Ties keep tree order. */
  zIndex?: number

  /** A colour, a gradient, or layers of them (the first on top, as in CSS). */
  background?: string | GradientBackground | (string | GradientBackground)[]
  backgroundColor?: string
  /** Images over the background: a src, a layer, or layers (the first on top). Clipped square. */
  backgroundImage?: string | BackgroundImageLayer | (string | BackgroundImageLayer)[]
  backgroundSize?: BackgroundSize
  /** `"repeat"` by default. */
  backgroundRepeat?: BackgroundRepeat
  /** `"pixelated"` draws this element's images (background, frame, `<img>`) by nearest neighbour: crisp pixel art. */
  imageRendering?: "auto" | "pixelated"
  color?: string
  opacity?: number
  /** An `<svg>`'s turn about its centre, in degrees, clockwise; it moves neither layout nor hit area. */
  rotate?: number

  borderWidth?: number
  borderTopWidth?: number
  borderRightWidth?: number
  borderBottomWidth?: number
  borderLeftWidth?: number
  borderColor?: string
  /** One side's colour, over `borderColor`: bevels, inset and outset frames. Square corners are mitred. */
  borderTopColor?: string
  borderRightColor?: string
  borderBottomColor?: string
  borderLeftColor?: string
  borderRadius?: number
  borderTopLeftRadius?: number
  borderTopRightRadius?: number
  borderBottomLeftRadius?: number
  borderBottomRightRadius?: number
  /** One shadow, or a list painted in the order given. */
  boxShadow?: BoxShadow | BoxShadow[]
  /** Applies to every side. */
  borderStyle?: "solid" | "dashed"
  /** CSS `outline`: a line outside the border box. Takes no layout space, so it
   *  can appear on focus without moving anything. Width defaults to 1. */
  outlineWidth?: number
  outlineColor?: string
  /** Gap between the border box and the outline. Negative draws it inside. */
  outlineOffset?: number

  fontSize?: number
  fontFamily?: string
  fontWeight?: string | number
  fontStyle?: "normal" | "italic"
  textAlign?: string
  lineHeight?: number
  whiteSpace?: "normal" | "nowrap"
  textOverflow?: "ellipsis" | "ellipsis-start"
  lineClamp?: number
  textDecoration?: "underline" | "line-through" | "none"

  overflow?: string
  overflowX?: string
  overflowY?: string
  /** Colour of the scrollbar thumb. Setting it paints a native thumb over a container
   *  that scrolls (`overflow: "scroll"`), on each axis that has more to show. The
   *  thumb follows the scroll offset in the frame it paints, and takes no pointer input. */
  scrollbarColor?: string
  /** Thickness of that thumb in px. Defaults to 6. */
  scrollbarWidth?: number

  cursor?: CursorValue
  /**
   * `"drag"`: the window moves by this element, pressed and moved (a title bar of the theme's
   * own); a double press zooms the window (or minimizes it, as the system's setting says).
   * `"no-drag"` keeps a part of one from it; what takes presses itself (`onClick`,
   * `onMouseDown`) already is.
   */
  appRegion?: "drag" | "no-drag"
  /** `"auto"` blocks hits behind this element **and its wheel**. `"none"` never
   *  blocks, nor does anything inside it until a `"auto"`, as CSS inherits it. Unset
   *  blocks clicks when the element paints a fill or is positioned, but lets the
   *  wheel reach the ancestor scroller, like HTML. */
  pointerEvents?: "auto" | "none"

  /** "none" opts this element and its subtree out of text selection.
   *  Inherited like the CSS property, so a toolbar can disable it once. */
  userSelect?: "text" | "none" | "auto"
  /** Selection wash colour for this subtree. Defaults to the theme accent at 35%. */
  selectionColor?: string

  // Pseudo-selector styles — applied by GPUI natively (no JS round-trip).
  // Nesting is one level deep: a state style cannot contain another.
  hover?: StateStyleDesc
  active?: StateStyleDesc
  /** While focused after keyboard input (Tab), like CSS `:focus-visible`.
   *  A mouse press never shows it. Needs a focusable element. */
  focusVisible?: StateStyleDesc
  /** `false` keeps this focusable element at full opacity while another
   *  control has keyboard focus. Default dims it. Not inherited. */
  keyboardFocusDim?: boolean
}

/** Fails to compile when `StyleDesc` has a field the renderer does not read: one that is not in
 *  the UI protocol's (`protocol.gen.ts`). */
export type StyleFieldsAreTheRenderers<Extra extends never = Exclude<keyof StyleDesc, keyof WireStyle>> = Extra

export type StateStyleDesc = Omit<
  StyleDesc,
  "hover" | "active" | "focusVisible" | "keyboardFocusDim"
>

// Element types supported by GPUIX
export type ElementType =
  | "div"
  | "text"
  | "img"
  | "svg"
  | "canvas"
  | "input"
  | "textarea"
  | "anchored"
  | "virtual-list"

// ── Theme ────────────────────────────────────────────────────────────

/**
 * Theme tokens for the native text components. Every field is optional and
 * layers on top of the built-in dark theme (or light, via `appearance`).
 */
export interface GpuixTheme {
  appearance?: "dark" | "light"
  bg?: string
  border?: string
  text?: string
  textMuted?: string
  textFaint?: string
  textDim?: string
  accent?: string
  caret?: string
  codeText?: string
  codeWash?: string
  diffAdd?: string
  diffDel?: string
  diffHunkBg?: string
  fontSans?: string
  fontMono?: string
}

/** One `highlight` entry. See `HostProps.highlight`. */
export interface HighlightSpec {
  /**
   * Substring to match. Case-insensitive unless `caseSensitive` is set.
   *
   * A match never crosses a line, exactly like browser find. It DOES cross the
   * several host nodes a JSX renderer makes for one interpolated line, so
   * `<text>Hello {name}!</text>` matches `Hello Tommy`.
   */
  query?: string
  caseSensitive?: boolean
  /** Only match when neither neighbour is Unicode Alphabetic, a number, a mark, or `_`. */
  wholeWord?: boolean
  /**
   * Explicit `[start, end)` pairs in UTF-16 code units, the units `indexOf` and
   * `RegExp.exec` return. They index the declaring subtree's text, with a
   * newline between lines.
   *
   * A pair that splits a surrogate pair is rejected, not snapped. Native text
   * (`<code>`, `<markdown>`, `<diff>`) is not part of that text; use `query`.
   */
  ranges?: Array<[number, number]>
  /** Any CSS colour. Defaults to the theme accent at 30% alpha. */
  color?: string
  /** Colour for the match at `activeIndex`. Defaults to accent at 65%. */
  activeColor?: string
  /** Index of the match to highlight differently, for a find-bar cursor. */
  activeIndex?: number
  /**
   * How many MATCHES come before this subtree in your document, so `activeIndex`
   * is compared against `matchIndexOffset + n` for the nth match here.
   *
   * It is a match count, not a row index. Rows hold different numbers of
   * matches, so a row index cannot stand in for it.
   *
   * Only needed for virtualized content: a `<virtual-list>` mounts a window of
   * its rows, so native can only number what that window contains. Sum
   * this many rows before `windowStart`. Defaults to 0.
   *
   * A negative or fractional value is refused and the whole spec is dropped,
   * because a bad offset silently marks the wrong match.
   */
  matchIndexOffset?: number
  /** Corner radius of the wash. Defaults to 2. */
  radius?: number
}

// Props passed to elements.
// Element IDs are auto-generated numeric IDs (not user-settable).
// Framework adapters add their own children, key, and ref fields.
export interface HostProps {
  style?: StyleDesc

  /** A tooltip: GPUI's own, in the theme's tooltip style. `div` and `text`. */
  title?: string

  // ── Mouse events ───────────────────────────────────────────────
  /** Primary button only, like the DOM. Use `onAuxClick` for the others. */
  onClick?: (event: PressEvent) => void
  /** The second primary click of a quick pair. `div` and `text`. */
  onDoubleClick?: (event: PressEvent) => void
  /** A right press, or a control-click. `div` and `text`. */
  onContextMenu?: (event: PressEvent) => void
  /** Non-primary click, like the DOM `auxclick`. `isRightClick` says which. */
  onAuxClick?: (event: PressEvent) => void
  onMouseDown?: (event: PressEvent) => void
  onMouseUp?: (event: PressEvent) => void
  onMouseEnter?: (event: EventPayload) => void
  onMouseLeave?: (event: EventPayload) => void
  onMouseMove?: (event: EventPayload) => void
  /** Fires when user clicks OUTSIDE this element. Use for "click outside to close". */
  onMouseDownOutside?: (event: EventPayload) => void

  // ── Keyboard events (need focus: autoFocus, or a click on the element) ──
  /** Fires on the focused element, then on ancestors that declare it. */
  onKeyDown?: (event: KeyEvent) => void
  onKeyUp?: (event: KeyEvent) => void

  // ── Focus events ───────────────────────────────────────────────
  onFocus?: (event: EventPayload) => void
  onBlur?: (event: EventPayload) => void

  // ── Scroll events ──────────────────────────────────────────────
  onScroll?: (event: EventPayload) => void

  /**
   * The scroll offset of a `overflow: "scroll"` element changed, however it
   * moved: wheel, scrollbar drag, `scrollTo`, `scrollIntoView`. `scrollX` and
   * `scrollY` are the pair `getScrollOffset` returns: 0 at the start and
   * negative as the content scrolls. The position at the first paint is not
   * reported, only movement after it. Not for `<virtual-list>`; use
   * `onVisibleRange`.
   */
  onScrollChange?: (event: EventPayload) => void

  // ── Layout events ──────────────────────────────────────────────
  /**
   * The element's border box, reported after the first paint and each time its
   * size changes: `width` and `height`, plus `x` and `y` in window coordinates.
   * Moving without resizing reports nothing. Supported on `div` and `text`.
   */
  onLayout?: (event: EventPayload) => void

  // ── File drop (Finder / OS paths) ───────────────────────────────
  onFileDrop?: (event: EventPayload) => void

  // ── Text editor events ─────────────────────────────────────────
  onChange?: (event: EventPayload) => void
  /** Enter on `<input>`, or Enter on `<textarea>` when this listener is set. */
  onSubmit?: (event: EventPayload) => void

  // ── Native component events ─────────────────────────────────────
  onVisibleRange?: (event: EventPayload) => void
  /** Match count changed for this element's `highlight`. See `matchCount`. */
  onHighlight?: (event: EventPayload) => void
  /** A native `motion` track reached its current target. */
  onMotionComplete?: (event: EventPayload) => void
  /**
   * Internal: an endless `motion` is sampled less often for what its frames cost; `value` is the
   * compartment it is drawn in. The motion components report it (`onSlowLoop`).
   */
  onMotionThrottled?: (event: EventPayload) => void

  // ── Highlight ──────────────────────────────────────────────────
  /**
   * Paint a background wash behind matched or explicitly given text ranges.
   *
   * Scoped by position: on the root it searches the window, on a container it
   * searches that container. The nearest declaration wins, so a nested
   * `highlight` replaces an ancestor's for its own subtree.
   */
  highlight?: HighlightSpec | HighlightSpec[] | null

  /**
   * Its text is one line, in however many elements it is drawn: a paragraph whose words are each
   * a box of their own in a wrapping row. A selection copies its runs with nothing between them
   * (not a line each), and a search finds a match across them. `div` and `text`.
   */
  textFlow?: boolean

  /**
   * On a `<text>`: its `<text>` children are runs of one paragraph, drawn as one text that wraps
   * at its box's end and selects as one, each run styled over what the paragraph inherits:
   * `color`, `fontWeight`, `fontStyle`, `fontFamily`, `textDecoration`, and a rounded
   * `backgroundColor` (`borderRadius`, `paddingLeft` up to 4) behind it, as inline code has. A run
   * with `onClick` is pressed by a click on its text (a link). Nothing else of a run's style or
   * events applies, and children that are not `<text>` are not drawn.
   */
  inline?: boolean

  // ── Focus props ────────────────────────────────────────────────
  /** Take keyboard focus when the element first mounts. Required for `<input>`:
   *  without it, or a click, the field never receives key events. */
  autoFocus?: boolean
  /** Native GPUI tab order. Use 0 for normal keyboard focus. */
  tabIndex?: number
  /**
   * AccessKit role, as an ARIA token (`"button"`, `"heading"`).
   * A node is in the accessibility tree only with both an id (always set)
   * and a role. `"none"` / `"presentation"` produce no node.
   */
  role?: string
  /** Accessible name. Maps to GPUI `aria_label`. */
  "aria-label"?: string
  /** Extra description announced after name, role, and value. */
  "aria-description"?: string
  /** Author id exposed as `AXIdentifier` / UIA AutomationId. */
  "aria-id"?: string
  "aria-expanded"?: boolean
  "aria-selected"?: boolean
  /** String value reported to assistive technology. */
  "aria-valuetext"?: string
  /** Heading level, 1-based. */
  "aria-level"?: number
  /** Stable locator id for automation. */
  testId?: string
  /** Internal native animation description used by motion components. */
  motion?: MotionProps
  /**
   * Keep what this subtree drew, and draw it again as it was, until something
   * in it changes. A window is otherwise rebuilt and laid out whole on every
   * frame: a terminal's output, a spinner, a hover anywhere. Put it on the big,
   * mostly still regions (a sidebar, a tab bar, a pane). The element's size
   * must come from its style and its parent (`flex: 1`, a width, `100%`), never
   * from its content: it is laid out without looking inside.
   */
  cache?: boolean
  /**
   * A compartment: draws nothing of its own, and its one child is kept and drawn
   * again as it was until something in it changes, like `cache`, but sized as it
   * would be without it (by its contents too). What one plugin draws costs the
   * window's frames only when it changes, whatever else moves. The app puts one
   * around every contributed component (`inCompartment`). A name (`"owner/name"`)
   * says whose it is in the performance overlay.
   */
  compartment?: boolean | string
}

// Props for native text editor elements.
export interface InputProps extends HostProps {
  /** External editor value. Native edits apply immediately and report through onChange. */
  value?: string
  placeholder?: string
  readOnly?: boolean
  theme?: GpuixTheme
  /**
   * Keys the editor binds (`"up"`, `"down"`, `"enter"`) that go to `onKeyDown` instead, with
   * no modifier, while listed: for a completion list drawn over the field. `"enter"` then sends
   * no `onSubmit`. Keys the editor does not bind (`"tab"`, `"escape"`) reach `onKeyDown` anyway.
   */
  interceptKeys?: string[]
  /**
   * Where the caret goes when `value` is set from outside (not echoing an edit of the field's own), as
   * a UTF-16 offset: its end without. A field the app types into keeps its caret where it was typed.
   */
  caret?: number
  /**
   * Mentions drawn as chips in the line: each `text` that stands whole in the value (at its start or
   * after a space, at its end or before one) is drawn as `label` on a rounded wash, after `icon` (an
   * SVG's markup or file). A chip is one character to the caret: it moves over it, and Backspace or
   * Delete takes it whole. The value keeps the text; copying a chip copies it.
   */
  chips?: TextChip[]
  /** The caret or selection moved. `selectionStart`, `selectionEnd` (UTF-16) say where; `change` carries them too. */
  onSelect?: (event: EventPayload) => void
}

/** One of a text field's `chips`. */
export interface TextChip {
  text: string
  label: string
  icon?: string
}

export interface TextareaProps extends InputProps {
  minRows?: number
  maxRows?: number
}

export type VirtualListShared = {
  /** No `hover` or `active`: gpui's `List` has no pressed or hovered style
   *  state. Put those on a wrapping `<div>` instead. */
  style?: Omit<StyleDesc, "hover" | "active">
  alignment?: "top" | "bottom"
  followTail?: boolean
  overdraw?: number
  onVisibleRange?: (event: EventPayload) => void
  role?: string
  "aria-label"?: string
  "aria-description"?: string
  "aria-id"?: string
  "aria-expanded"?: boolean
  "aria-selected"?: boolean
  "aria-valuetext"?: string
  "aria-level"?: number
  testId?: string
}

/** A variable-height list that builds only rows near its viewport. */
export type VirtualListProps =
  | (VirtualListShared & {
      estimatedItemHeight?: number
      itemCount?: never
      windowStart?: never
    })
  | (VirtualListShared & {
      itemCount: number
      estimatedItemHeight: number
      windowStart?: number
    })

// Props for native <img> rendering.
export interface ImgProps extends HostProps {
  /** Filesystem path (an imported asset), data URL, or http(s) URL. */
  src?: string
  objectFit?: "fill" | "contain" | "cover" | "scaleDown" | "none"
  alt?: string
  /** The image is a sheet of equal cells, one drawn at a time; with `fps`, played. The box
   *  is one cell unless it has a size. `style.imageRendering: "pixelated"` keeps pixel art crisp. */
  sprite?: SpriteSheet
}

export interface SpriteSheet {
  /** Cells across. 1 by default. */
  columns?: number
  /** Cells down. As many as `count` needs, or 1. */
  rows?: number
  /** Cells in use, when the last row is not full. */
  count?: number
  /** Frames a second. 0 (the default) shows `frame` and stays. */
  fps?: number
  /** The cell shown, or the first one played, from 0. */
  frame?: number
  /** Back to the first after the last. True by default. */
  loop?: boolean
  /** False holds the current cell. */
  playing?: boolean
}

/** Byte order of a `setImagePixels` buffer. Alpha is straight in both. */
export type ImagePixelFormat = "rgba" | "bgra"

export interface ImagePixelsOptions {
  /** Default `"rgba"`. `"bgra"` is GPUI's native order and skips a per-pixel pass. */
  format?: ImagePixelFormat
}

// Props for monochrome SVGs tinted by style.color.
export interface SvgProps extends HostProps {
  /** Desktop local path. Use source for portable browser rendering. */
  src?: string
  /** Raw SVG markup rendered directly by GPUI. */
  source?: string
}

// Props for the <anchored> custom element.
export interface AnchoredProps extends HostProps {
  position?: { x: number; y: number }
  side?: "top" | "right" | "bottom" | "left"
  align?: "start" | "center" | "end"
  gap?: number
  anchor?:
    | "topLeft"
    | "topCenter"
    | "topRight"
    | "rightCenter"
    | "bottomRight"
    | "bottomCenter"
    | "bottomLeft"
    | "leftCenter"
  offset?: { x: number; y: number }
  fit?: "switch" | "snap"
  snapMargin?: number
  deferred?: boolean
  priority?: number
  occlude?: boolean
  /** `"window"` covers the whole viewport and tracks resizes natively. Ignores
   *  position, side, align, anchor, offset and fit. */
  fill?: "window"
}

export type MutationHost = Pick<GpuixRenderer, "applyBatch">
export type WindowSizeHost = Pick<GpuixRenderer, "getWindowSize">
export type WindowInsetsHost = Pick<GpuixRenderer, "getWindowInsets">
export type SelectionHost = Pick<GpuixRenderer, "getSelectedText">
/** `applyBatch` plus the live renderer methods a host may also expose. */
export type NativeRenderer = MutationHost &
  Partial<
    Pick<
      GpuixRenderer,
      | "focusElement"
      | "focusNext"
      | "focusPrevious"
      | "getFocusedElementId"
      | "focusNextWithin"
      | "focusPreviousWithin"
      | "blur"
      | "setWindowKeyEvents"
      | "setWindowSelectionChange"
      | "getElementBounds"
      | "getElementsAt"
      | "scrollTo"
      | "scrollToItem"
      | "scrollIntoView"
      | "setImage"
      | "setImagePixels"
      | "getScrollOffset"
      | "getListScrollTop"
      | "getSelectedText"
      | "clearSelection"
      | "getPaintedHighlights"
      | "getWindowSize"
      | "getWindowInsets"
      | "setWindowTitle"
      | "registerFont"
      | "setWindowBackground"
      | "setWindowBlur"
      | "setWindowGlass"
      | "isFullscreen"
      | "setTooltipStyle"
      | "promptForPaths"
      | "activateWindow"
      | "minimizeWindow"
      | "zoomWindow"
      | "toggleFullscreen"
      | "setDebugFrameOverlay"
      | "getDebugFrameOverlay"
      | "cycleDebugFrameOverlay"
      | "resetDebugFrameOverlayStats"
      | "getDebugFrameOverlayStats"
    >
  >

/** Commit-phase mutation facade shared by framework adapters. */
export interface MutationRenderer {
  createElement(id: number, elementType: string): void
  destroyElement(id: number): Array<number>
  appendChild(parentId: number, childId: number): void
  insertBefore(parentId: number, childId: number, beforeId: number): void
  setStyle(id: number, style: WireStyle): void
  setText(id: number, content: string): void
  setEventListener(id: number, eventType: string, hasHandler: boolean): void
  setRoot(id: number): void
  setKeyboardFocusDim(enabled: boolean): void
  setCustomProp(id: number, key: string, value: object | string | number | boolean | null): void
  flushMutations(): void
}

export type DebugFrameOverlayMode = "hidden" | "minimal" | "full"

/** Method syntax makes the parameter bivariant, so an `onKeyDown` typed with
 *  `KeyEvent` can share the map. Dispatch builds the `KeyEvent` for key types. */
export type HostEventHandler = { handle(event: EventPayload): void }["handle"]

export type EventHandlerMap = Map<number, Map<string, HostEventHandler>>

/**
 * A `keyDown` / `keyUp` event. Element handlers and the window handler of one
 * keystroke share the flags, like one DOM event bubbling to `window`.
 */
export interface KeyEvent extends EventPayload {
  /** True after any handler of this keystroke called `preventDefault()`. */
  readonly defaultPrevented: boolean
  /** Cancel the default action, for example Tab moving focus. */
  preventDefault(): void
  /** Skip ancestor `onKeyDown` handlers and the window handler. The default
   *  action still runs, like the DOM. */
  stopPropagation(): void
}

/**
 * A click, double click, aux click, context menu, mouse down or up. One press's
 * handlers run innermost first, like one DOM event bubbling.
 */
export interface PressEvent extends EventPayload {
  /** Skip ancestors' handlers of this same event, as the DOM does. */
  stopPropagation(): void
}

export type WindowKeyEventHandler = (
  event: KeyEvent,
  renderer: NativeRenderer
) => void

export type WindowEventHandler = (
  event: EventPayload,
  renderer: NativeRenderer
) => void

export interface WindowKeyEventHandlers {
  /** Window-level GPUI listener. Runs after element `onKeyDown` handlers.
   *  Key actions can consume an event before this runs. */
  onKeyDown?: WindowKeyEventHandler
  /** Window-level GPUI listener. */
  onKeyUp?: WindowKeyEventHandler
  /** Window-level text selection. Fires when the selected ranges change. */
  onSelectionChange?: WindowEventHandler
  /** A modifier pressed or let go on its own (no keystroke carries it): the ones held now. */
  onModifiersChanged?: (modifiers: { ctrl: boolean; alt: boolean; shift: boolean; cmd: boolean }) => void
  /**
   * Tab and Shift+Tab move focus through `tabIndex` elements. Defaults to
   * true. Call `event.preventDefault()` in any `onKeyDown` to keep one Tab.
   */
  tabNavigation?: boolean
  /**
   * While a control has keyboard focus, every other focusable element dims.
   * Defaults to true. Set false to draw your own focus look with
   * `focusVisible`. Per element, use `style.keyboardFocusDim: false`.
   */
  keyboardFocusDim?: boolean
}

export interface RootEventHandlers extends WindowKeyEventHandlers {
  onEvent?: (event: EventPayload) => void
  onUncaughtError?: (
    error: Error,
    errorInfo: { componentStack?: string }
  ) => void
}

export interface ElementIdAllocator {
  nextElementId: number
}
