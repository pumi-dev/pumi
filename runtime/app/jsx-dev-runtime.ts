// Bun compiles JSX to `jsxDEV` unless NODE_ENV was `production` when it started, while React picks
// its build from NODE_ENV when first imported; production React's jsx-dev-runtime has no
// `jsxDEV`. So `jsxDEV` is React's own when there is one, else the production `jsx`.
import * as dev from "react/jsx-dev-runtime"
import { jsx as prodJsx, Fragment } from "react/jsx-runtime"

type Jsx = typeof prodJsx
const own = (dev as unknown as { jsxDEV?: Jsx }).jsxDEV
export const jsxDEV: Jsx = own ?? ((type, props, key) => prodJsx(type, props, key))
export const jsx = jsxDEV
export const jsxs = jsxDEV
export { Fragment }
export type { JSX } from "./jsx-runtime.ts"
