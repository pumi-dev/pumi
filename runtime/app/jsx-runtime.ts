// The JSX runtime: React's, with the renderer's elements as the intrinsics. Activated by
// "jsxImportSource": "pumi".
///
/// `key` is declared on `Props`, not on `IntrinsicAttributes` below.
/// TypeScript 5 ignores `IntrinsicAttributes` for intrinsic elements.

import type * as React from "react"
import type {
  AnchoredProps,
  ImgProps,
  InputProps,
  Props,
  SvgProps,
  TextareaProps,
  VirtualListProps,
} from "./react/types/host.ts"

export { jsx, jsxs, Fragment } from "react/jsx-runtime"

export namespace JSX {
  export type ElementType = React.JSX.ElementType
  export type Element = React.JSX.Element
  export type ElementClass = React.JSX.ElementClass
  export type ElementAttributesProperty = React.JSX.ElementAttributesProperty
  export type ElementChildrenAttribute = React.JSX.ElementChildrenAttribute
  export type IntrinsicAttributes = React.JSX.IntrinsicAttributes
  export type IntrinsicClassAttributes<T> = React.JSX.IntrinsicClassAttributes<T>

  export interface IntrinsicElements {
    div: Props
    text: Props
    img: ImgProps
    svg: SvgProps
    canvas: Props
    input: InputProps
    textarea: TextareaProps
    anchored: AnchoredProps
    "virtual-list": VirtualListProps
    /** A tab's live terminal. */
    terminal: Props & { tab: string | number | { id: number; server?: string }; autoFocus?: boolean }
    /** A tab's screen, scaled into the box. */
    preview: Props & { tab: string | number | { id: number; server?: string } }
    /** A web page (sdk/webview.tsx's `WebView` fills in a plugin's own). */
    webview: Props & { handle: string; src: string; plugin?: string; dir?: string; args?: unknown; theme?: unknown; themeCss?: string; background?: string; keepKeys?: string[]; zoom?: number; autoFocus?: boolean }
  }
}
