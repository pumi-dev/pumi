// Generated from api/ui-protocol.json by `bun runtime/app/gen/index.ts` (`cargo xtask api` runs it); do not edit.

/** The protocol's level: what `pumi_ui_proto::LEVEL` is. */
export const LEVEL = 1;

/** The element types the renderer always has; every other type is a custom element. */
export const BUILT_IN_TYPES = [
  "div",
  "text",
] as const;

/** The events `setEventListener` can listen for on an element. */
export const ELEMENT_EVENTS = [
  "click",
  "doubleClick",
  "auxClick",
  "contextMenu",
  "mouseDown",
  "mouseUp",
  "mouseMove",
  "mouseEnter",
  "mouseLeave",
  "mouseDownOutside",
  "scroll",
  "fileDrop",
  "keyDown",
  "keyUp",
  "focus",
  "blur",
  "change",
  "select",
  "submit",
  "layout",
  "scrollChange",
  "visibleRange",
  "highlight",
  "motionComplete",
  "motionThrottled",
] as const;
export type ElementEvent = (typeof ELEMENT_EVENTS)[number];

/** The custom props a built-in element reads. */
export const UNIVERSAL_PROPS = [
  "autoFocus",
  "tabIndex",
  "motion",
  "cache",
  "compartment",
  "testId",
  "title",
  "highlight",
  "textFlow",
  "role",
  "aria-label",
  "aria-description",
  "aria-id",
  "aria-expanded",
  "aria-selected",
  "aria-valuetext",
  "aria-level",
] as const;

export type BackgroundImageItem = string | BackgroundImageLayer;

/**
 * One image layer of `backgroundImage`. What a layer leaves out comes from the element's
 * `backgroundSize`, `backgroundRepeat` and `backgroundPosition`.
 */
export type BackgroundImageLayer = {
  src: string;
  size?: BackgroundSizeValue | null;
  repeat?: string | null;
  position?: BackgroundPositionValue | null;
  opacity?: number | null;
};

/** `backgroundImage`: a src, a layer, or layers (the first on top). */
export type BackgroundImageValue = BackgroundImageItem | BackgroundImageItem[];

/** One layer of a layered `background`: a colour or a gradient. */
export type BackgroundLayerValue = string | GradientValue;

/** `backgroundPosition`: a keyword (`"center"`, `"top left"`) or `[x, y]`. */
export type BackgroundPositionValue = string | LengthValue[];

/** `backgroundSize`: `"auto"`, `"cover"`, `"contain"`, a width, or `[width, height]`. */
export type BackgroundSizeValue = number | string | LengthValue[];

export type BackgroundValue = string | GradientValue | BackgroundLayerValue[];

/**
 * A nine-slice image drawn as the element's frame (CSS `border-image`). `slice` is in the
 * image's pixels, `width` in the window's (the slice by default). The middle is drawn too
 * unless `fill` is false.
 */
export type BorderImageValue = {
  src: string;
  slice: SidesValue;
  width?: SidesValue | null;
  /** `"stretch"` (default), `"repeat"` or `"round"`, for the edges and the middle. */
  repeat?: string | null;
  fill?: boolean | null;
  /** Draws the frame this far outside the border box. */
  outset?: number | null;
};

export type BoxShadowValue = {
  offsetX: number;
  offsetY: number;
  blurRadius: number;
  spreadRadius: number;
  color: string;
  /** Draws inside the border box instead of around it, like CSS `inset`. */
  inset?: boolean;
};

/** `boxShadow` is one shadow or a list of them, painted in the order given. */
export type BoxShadowValues = BoxShadowValue[] | BoxShadowValue;

/** Pixels as a number, or a string: `"auto"`, a percentage (`"50%"`) or pixels (`"12px"`, `"12"`). */
export type DimensionValue = number | string;

/** An element's id: unique within its window, picked by the UI. 0 is the window. */
export type ElementId = number;

/**
 * One event, on `target`: an element that has a listener for it (`setEventListener`), or the
 * window. Window events have target 0, except the window's key and selection events, which have
 * the id the UI gave when it asked for them.
 */
export type Event = {
  target: ElementId;
} & (Press & {
  type: "click";
} | Press & {
  type: "doubleClick";
} | Press & {
  type: "auxClick";
} | Press & {
  type: "contextMenu";
} | Press & {
  type: "mouseDown";
} | Press & {
  type: "mouseUp";
} | {
  type: "mouseMove";
  position: Point;
  modifiers: Modifiers;
  /** The button held down, if one is: 0 left, 1 middle, 2 right, 3 back/forward. */
  pressedButton?: number | null;
} | {
  type: "mouseEnter";
} | {
  type: "mouseLeave";
} | {
  type: "mouseDownOutside";
  position: Point;
  button: number;
  modifiers: Modifiers;
} | {
  type: "scroll";
  position: Point;
  modifiers: Modifiers;
  delta: Point;
  /** Pixel-precise (a trackpad), or by lines (a mouse wheel). */
  precise: boolean;
  touchPhase: TouchPhase;
} | {
  type: "fileDrop";
  position: Point;
  paths: string[];
} | Key & {
  type: "keyDown";
} | Key & {
  type: "keyUp";
} | {
  type: "focus";
} | {
  type: "blur";
} | {
  type: "change";
  value: string;
  selection: Selection;
} | {
  type: "select";
  selection: Selection;
} | {
  type: "submit";
  value: string;
} | {
  type: "layout";
  x: number;
  y: number;
  width: number;
  height: number;
} | {
  type: "scrollChange";
  x: number;
  y: number;
} | {
  type: "visibleRange";
  start: number;
  end: number;
} | {
  type: "highlight";
  matchCount: number;
} | {
  type: "motionComplete";
  generation: number;
} | {
  type: "motionThrottled";
  compartment?: string | null;
} | {
  type: "selectionChange";
  text?: string | null;
} | {
  type: "windowActivated";
  active: boolean;
} | {
  type: "windowClosed";
} | {
  type: "windowHidden";
} | {
  type: "windowFullscreen";
  fullscreen: boolean;
} | Key & {
  type: "windowKeyDown";
} | Key & {
  type: "windowKeyUp";
} | {
  type: "windowModifiersChanged";
  modifiers: Modifiers;
});

/** Font weight value: a CSS keyword or numeric string (`"bold"`, `"700"`) or a number (`700`). */
export type FontWeightValue = number | string;

export type GradientValue = {
  type: "linear-gradient";
  angle?: number;
  stops: LinearGradientStopValue[];
  colorSpace?: string | null;
} | {
  type: "radial-gradient";
  shape?: string | null;
  center?: number[] | null;
  radius?: RadiusValue | null;
  stops: LinearGradientStopValue[];
} | {
  type: "conic-gradient";
  angle?: number;
  center?: number[] | null;
  stops: LinearGradientStopValue[];
};

/** A key going down or up. */
export type Key = {
  /** Its name: `"a"`, `"enter"`, `"escape"`, `"down"`, `"f1"`. */
  key: string;
  /** The character it types, if one: `"ß"` for ⌥s. */
  keyChar?: string | null;
  /** Held down and repeating (key down only). */
  isHeld?: boolean | null;
  modifiers: Modifiers;
  /**
   * Shared by every element's event of one keystroke and the window's, so a UI can apply
   * `preventDefault` and `stopPropagation` across them.
   */
  keystrokeId: number;
};

/** A length that is pixels (a number), a percentage (`"50%"`) or a keyword. */
export type LengthValue = number | string;

/** One colour stop. A stop without a position sits evenly between its neighbours, like CSS. */
export type LinearGradientStopValue = {
  color: string;
  position?: number | null;
};

/** The modifier keys held. `cmd` is ⌘ on macOS and the Windows key elsewhere. */
export type Modifiers = {
  shift: boolean;
  ctrl: boolean;
  alt: boolean;
  cmd: boolean;
};

/** One change to the window's tree, as a tuple with its name first. */
export type Op = ["createElement", ElementId, string] | ["destroyElement", ElementId] | ["appendChild", ElementId, ElementId] | ["insertBefore", ElementId, ElementId, ElementId] | ["setStyle", ElementId, StyleDesc] | ["setText", ElementId, string] | ["setEventListener", ElementId, string, boolean] | ["setRoot", ElementId] | ["setKeyboardFocusDim", boolean] | ["setCustomProp", ElementId, string, unknown];

/** A point in window coordinates, in pixels; or a distance, for a scroll's `delta`. */
export type Point = {
  x: number;
  y: number;
};

/**
 * A mouse press or release. What a press does not know is `None`: an assistive technology's
 * click has no position.
 */
export type Press = {
  position?: Point | null;
  /** 0 left, 1 middle, 2 right, 3 back/forward. */
  button?: number | null;
  /** 1, 2 or 3 for a single, double or triple click. */
  clickCount?: number | null;
  isRightClick?: boolean | null;
  modifiers?: Modifiers | null;
  /**
   * Shared by every element's event of one press, innermost first, so a UI can stop it
   * propagating across them.
   */
  pressId?: number | null;
};

/**
 * A radial gradient's size: one radius (a circle), `[x, y]` radii, or omitted for the
 * farthest corner.
 */
export type RadiusValue = number | number[];

/**
 * A text field's selection as UTF-16 offsets into its value (the units JS strings use):
 * equal when it is only the caret. `end` is the caret unless it was selected backwards.
 */
export type Selection = {
  start: number;
  end: number;
};

/** Four sides in CSS order: one value, `[vertical, horizontal]`, or `[top, right, bottom, left]`. */
export type SidesValue = number | number[];

/** An element's style: what `setStyle` sets. Every field is optional; one left out is unset. */
export type StyleDesc = {
  display?: string | null;
  visibility?: string | null;
  flexDirection?: string | null;
  flexWrap?: string | null;
  flexGrow?: number | null;
  flexShrink?: number | null;
  flexBasis?: number | null;
  alignItems?: string | null;
  alignSelf?: string | null;
  alignContent?: string | null;
  justifyContent?: string | null;
  gap?: number | null;
  rowGap?: number | null;
  columnGap?: number | null;
  gridTemplateColumns?: number | null;
  gridTemplateRows?: number | null;
  gridColumnMin?: string | null;
  gridRowMin?: string | null;
  width?: DimensionValue | null;
  height?: DimensionValue | null;
  minWidth?: DimensionValue | null;
  minHeight?: DimensionValue | null;
  maxWidth?: DimensionValue | null;
  maxHeight?: DimensionValue | null;
  padding?: number | null;
  paddingTop?: number | null;
  paddingRight?: number | null;
  paddingBottom?: number | null;
  paddingLeft?: number | null;
  margin?: number | null;
  marginTop?: number | null;
  marginRight?: number | null;
  marginBottom?: number | null;
  marginLeft?: number | null;
  position?: string | null;
  top?: DimensionValue | null;
  right?: DimensionValue | null;
  bottom?: DimensionValue | null;
  left?: DimensionValue | null;
  /** Paint order among siblings: higher paints (and hit-tests) later. Ties keep tree order. */
  zIndex?: number | null;
  background?: BackgroundValue | null;
  backgroundColor?: string | null;
  backgroundImage?: BackgroundImageValue | null;
  backgroundSize?: BackgroundSizeValue | null;
  /** `"repeat"` (default), `"repeat-x"`, `"repeat-y"` or `"no-repeat"`. */
  backgroundRepeat?: string | null;
  backgroundPosition?: BackgroundPositionValue | null;
  /**
   * `"pixelated"` samples this element's images (background, frame, `<img>`) by nearest
   * neighbour, for pixel art. `"auto"` smooths them.
   */
  imageRendering?: string | null;
  color?: string | null;
  opacity?: number | null;
  /**
   * An `<svg>`'s turn about its centre, in degrees, clockwise: it moves neither its layout nor
   * where it is hit.
   */
  rotate?: number | null;
  borderWidth?: number | null;
  borderTopWidth?: number | null;
  borderRightWidth?: number | null;
  borderBottomWidth?: number | null;
  borderLeftWidth?: number | null;
  borderColor?: string | null;
  /** One side's colour, over `borderColor`: bevels, inset and outset frames. */
  borderTopColor?: string | null;
  borderRightColor?: string | null;
  borderBottomColor?: string | null;
  borderLeftColor?: string | null;
  borderImage?: BorderImageValue | null;
  borderRadius?: number | null;
  borderTopLeftRadius?: number | null;
  borderTopRightRadius?: number | null;
  borderBottomLeftRadius?: number | null;
  borderBottomRightRadius?: number | null;
  boxShadow?: BoxShadowValues | null;
  /** `"solid"` or `"dashed"`, for every side. */
  borderStyle?: string | null;
  /** Width divided by height, applied to whichever axis is not definite. */
  aspectRatio?: number | null;
  /**
   * CSS `outline`: a line outside the border box that takes no layout
   * space, so it can appear on focus without moving anything.
   */
  outlineWidth?: number | null;
  outlineColor?: string | null;
  outlineOffset?: number | null;
  fontSize?: number | null;
  fontFamily?: string | null;
  fontWeight?: FontWeightValue | null;
  /** `"italic"` or `"normal"`. */
  fontStyle?: string | null;
  textAlign?: string | null;
  lineHeight?: number | null;
  whiteSpace?: string | null;
  textOverflow?: string | null;
  lineClamp?: number | null;
  textDecoration?: string | null;
  overflow?: string | null;
  overflowX?: string | null;
  overflowY?: string | null;
  /**
   * Thumb colour for a container that scrolls (`overflow: "scroll"`). Setting
   * it is what turns the native scrollbar on.
   */
  scrollbarColor?: string | null;
  /** Thickness of that thumb in px. Defaults to 6. */
  scrollbarWidth?: number | null;
  cursor?: string | null;
  /**
   * `"drag"`: the window moves by this element (a title bar of the app's own); `"no-drag"`:
   * not by this part of one.
   */
  appRegion?: string | null;
  /**
   * `"auto"` blocks mouse hits behind this element. `"none"` never does.
   * Unset: block when this element paints a fill or is absolutely positioned.
   */
  pointerEvents?: string | null;
  userSelect?: string | null;
  /**
   * Selection wash colour for this subtree. Defaults to the theme accent at
   * 35% opacity, the same tone Comet uses.
   */
  selectionColor?: string | null;
  hover?: StyleDesc | null;
  active?: StyleDesc | null;
  /** While focused after keyboard input, like CSS `:focus-visible`. */
  focusVisible?: StyleDesc | null;
  /**
   * `false` keeps this element at full opacity while another control has
   * keyboard focus. Not inherited.
   */
  keyboardFocusDim?: boolean | null;
};

/** Where a trackpad scroll is. */
export type TouchPhase = "started" | "moved" | "ended" | "cancelled";
