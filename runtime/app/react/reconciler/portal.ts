import type { ReactNode } from "react"
import type { PublicInstance } from "../types/host.js"
import { portalContainerFor } from "./host-config.js"

const REACT_PORTAL_TYPE = Symbol.for("react.portal")

/**
 * Render `children` as children of `target`, an element that is already
 * mounted, in the order React places them. The content keeps the context of the
 * component that called `createPortal`, as in React DOM.
 *
 * Take `target` from a `ref` and render the portal once it is set: a portal
 * into an element that is not mounted yet has nowhere to attach.
 *
 * ```tsx
 * const [layer, setLayer] = useState<PublicInstance | null>(null)
 * return (
 *   <>
 *     <div ref={setLayer} style={{ position: "absolute", inset: 0 }} />
 *     {layer && createPortal(<Toast />, layer)}
 *   </>
 * )
 * ```
 *
 * `target` must belong to the same root as the caller.
 */
export function createPortal(
  children: ReactNode,
  target: PublicInstance,
  key?: string | null
): ReactNode {
  return {
    $$typeof: REACT_PORTAL_TYPE,
    key: key == null ? null : String(key),
    children,
    containerInfo: portalContainerFor(target),
    implementation: null,
  } as unknown as ReactNode
}
