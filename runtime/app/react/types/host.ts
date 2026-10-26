import type { Key, ReactNode, Ref } from "react"
import type { EventPayload } from "../../native/addon"
import type {
  ElementIdAllocator,
  ElementType,
  EventHandlerMap,
  HostProps,
  ImagePixelsOptions,
  InputProps as NativeInputProps,
  TextareaProps as NativeTextareaProps,
  ImgProps as NativeImgProps,
  SvgProps as NativeSvgProps,
  AnchoredProps as NativeAnchoredProps,
  MutationRenderer,
  NativeRenderer,
  RootEventHandlers,
  WindowKeyEventHandlers,
} from "../../native/host"

export * from "../../native/host"

export interface Props extends HostProps {
  key?: Key | null
  children?: ReactNode
  ref?: Ref<PublicInstance>
}

export interface InputProps extends NativeInputProps {
  key?: Key | null
  children?: ReactNode
  ref?: Ref<PublicInstance>
}

export interface TextareaProps extends NativeTextareaProps {
  key?: Key | null
  children?: ReactNode
  ref?: Ref<PublicInstance>
}

export interface ImgProps extends NativeImgProps {
  key?: Key | null
  children?: ReactNode
  ref?: Ref<ImgInstance>
}

export interface SvgProps extends NativeSvgProps {
  key?: Key | null
  children?: ReactNode
  ref?: Ref<PublicInstance>
}

export interface AnchoredProps extends NativeAnchoredProps {
  key?: Key | null
  children?: ReactNode
  ref?: Ref<PublicInstance>
}

export type VirtualListProps = import("../../native/host").VirtualListProps & {
  key?: Key | null
  children?: ReactNode
  ref?: Ref<PublicInstance>
}

export interface Container {
  nativeRenderer: NativeRenderer
  renderer: MutationRenderer
  ids: ElementIdAllocator
  eventHandlers: EventHandlerMap
  windowKeyEventHandlers: WindowKeyEventHandlers
  windowKeyEventId: number
  windowSelectionEventId: number
  onEvent?: (event: EventPayload) => void
  /**
   * Set on a portal's container. Its children mount under this element instead
   * of becoming the window's root. The container inherits everything else from
   * the root container it was made from, so both share one mutation queue and
   * one event map.
   */
  portalTarget?: Instance
}

export interface Instance {
  id: number
  type: ElementType
  props: Props
  scrollIntoView?(): void
  /** A scrolling box's: scroll its content to `x`, `y` px from its start, as the DOM's
   *  `scrollTo`. Before the box is first laid out it does nothing; `onLayout` says when it is. */
  scrollTo?(x: number, y: number): void
  /** A `<virtual-list>`'s: scroll its logical row `index` to the top, less `offsetInItem` px
   *  (negative anchors the viewport above it: `-(viewport - rowHeight)` puts it at the bottom). */
  scrollToItem?(index: number, offsetInItem?: number): void
  /** A `<virtual-list>`'s: `[itemIndex, offsetInItemPx, viewportHeightPx]`, or null. */
  listScrollTop?(): number[] | null
}

export interface ImgInstance extends Instance {
  type: "img"
  setImage(bytes: Buffer | Uint8Array): void
  setImagePixels(
    width: number,
    height: number,
    pixels: Buffer | Uint8Array,
    options?: ImagePixelsOptions,
  ): void
}

export interface TextInstance {
  id: number
  text: string
  parentId: number | null
}

export type PublicInstance = Instance

export interface HostContext {
  isInsideText: boolean
}

export type { ElementIdAllocator, EventHandlerMap, NativeRenderer, RootEventHandlers }
