# @pumi-dev/sdk

Types and test tools for [pumi](https://pumi.dev) plugins, so a plugin type-checks and tests
outside the pumi repository. At run time the app gives a plugin its own `pumi`; this package is for
`tsc` and `bun test`.

Install it under the name `pumi`:

```json
"devDependencies": { "pumi": "npm:@pumi-dev/sdk@{{version}}", "@types/bun": "1.3.14", "typescript": "5.9.3" }
```

This version, {{version}}, was built from pumi {{app}}. The SDK's version is its own and moves
when what a plugin can import changes; a plugin that uses something newer than the app it runs
in gets an error from the app at run time, not from `tsc`.

```json
// tsconfig.json
{
  "compilerOptions": {
    "target": "ESNext",
    "module": "Preserve",
    "moduleResolution": "bundler",
    "lib": ["ESNext"],
    "types": ["bun"],
    "jsx": "react-jsx",
    "jsxImportSource": "pumi",
    "strict": true,
    "noEmit": true,
    "allowImportingTsExtensions": true,
    "verbatimModuleSyntax": true,
    "skipLibCheck": true
  },
  "exclude": ["node_modules"]
}
```

| Import | What it is |
| --- | --- |
| `pumi` | the SDK's types: `main.ts`'s and `ui.tsx`'s |
| `pumi/theme` | a UI theme's kit |
| `pumi/jsx-runtime`, `pumi/jsx-dev-runtime` | React's JSX with the renderer's elements (`div`, `text`, `terminal`, …) |
| `pumi/testing` | `createTestRoot` and `TestRenderer`: components drawn by the native test renderer |

`pumi` and `pumi/theme` hold no code. In a test each of their values is a stand-in: code that
imports one loads, a call gives a rejected promise, and using what it returned throws. A test
stands in for the app with `mock.module("pumi", () => ({ … }))` before importing what uses it.

`react` is a peer dependency at the app's version, so the plugin's code, the JSX and the test
renderer share one React.

## Testing components

```tsx
import { mock, test, expect, describe } from "bun:test"
mock.module("pumi", () => ({ useTheme: () => ({ text: "#fff" }) }))
const { createTestRoot, hasNativeTestRenderer } = await import("pumi/testing")
const { Panel } = await import("../src/ui.tsx")

;(hasNativeTestRenderer ? describe : describe.skip)("the panel", () => {
  test("says hello", () => {
    const { render, renderer } = createTestRoot({ width: 400, height: 300 })
    render(<Panel />)
    expect(renderer.getAllText()).toContain("hello")
  })
})
```

The renderer is the app's native addon, which this package does not ship. `pumi/testing` loads
the file `PUMI_ADDON` names, else the installed app's:

| | |
| --- | --- |
| macOS | `/Applications/pumi.app/Contents/MacOS/pumi.darwin-<arch>.node` (or `~/Applications/…`) |
| Linux | `~/.local/pumi.app/bin/pumi.linux-<arch>.node` |
| Windows | `%LOCALAPPDATA%\Programs\pumi\pumi.app\bin\pumi.win32-x64.node` |

A release build of the app has no test renderer, so with the installed app `hasNativeTestRenderer`
is false and such tests skip. A debug build has it: `bun run addon` in a pumi checkout writes
`runtime/app/native/pumi.<platform>-<arch>.node`; point `PUMI_ADDON` at it. Linux has no test
renderer in any build.

Docs: https://pumi.dev/docs/
