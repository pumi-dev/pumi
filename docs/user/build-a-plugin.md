---
title: Build a plugin
group: Build
order: 3
description: From an empty folder to a plugin others can install, with the SDK for types and tests.
---

This page walks through making a plugin, running it while you edit it, type-checking and testing it with the SDK, and sharing it. [Plugins](plugins.md) is the reference for everything a plugin can add.

You need pumi installed, and [Bun](https://bun.sh) for type-checking and tests. pumi runs the plugin on its own Bun, so the plugin itself needs nothing installed.

## 1. Make it

```sh
pumi plugin new hello
```

This writes a starter plugin in `hello/`:

| File | |
| --- | --- |
| `package.json` | the manifest, under `"pumi"`: a command, two options, a sidebar panel, a tab menu item and a section in Settings |
| `src/main.ts` | its logic, in a worker: registers the `hello` command |
| `src/ui.tsx` | what it draws, in React: the panel and the Settings section |
| `tsconfig.json` | TypeScript settings for `tsc`, with the SDK (step 3) |
| `AGENTS.md` | a summary of the manifest and the API, for a coding agent working on it |

Its id is `owner/name`: `name` is the folder's name and `owner` is your GitHub user (`git config github.user`), else your login name. Both are in `package.json` and can be changed there.

## 2. Run it

```sh
pumi plugin dev hello
```

This links the folder, loads the plugin, and reloads it each time you save a file in it. Its log prints in the same terminal. Open the palette and run "Say hello", or open the sidebar panel.

If it fails to load, `pumi plugin logs hello` shows its stderr and why it last stopped.

## 3. Type-check it with the SDK

At run time, the app gives the plugin `import … from "pumi"`. Outside the app, `tsc` and `bun test` need the SDK, the npm package [`@pumi-dev/sdk`](https://www.npmjs.com/package/@pumi-dev/sdk). `pumi plugin new` already lists it, under the name `pumi` so imports are the same in both places, and sets `"jsxImportSource": "pumi"` in `tsconfig.json` so JSX knows pumi's elements (`div`, `text`, `terminal`, …):

```json
"devDependencies": {
  "pumi": "npm:@pumi-dev/sdk@0.2.0",
  "@types/bun": "1.3.14",
  "typescript": "5.9.3"
}
```

```sh
cd hello
bun install
bun run check     # tsc --noEmit
```

A plugin made before the SDK existed gets it the same way: add that `pumi` line to its `devDependencies` and `jsxImportSource` to its `tsconfig.json`, and drop `@types/react`, which the SDK brings.

The SDK brings React and its types as peers, at the versions the app runs. Its version is its own: it moves when what a plugin can import changes, and its README says which pumi version it was built from. A plugin that uses an export the installed app does not have yet fails when it runs, not in `tsc`.

| Import | |
| --- | --- |
| `pumi` | the API: `main.ts`'s objects and `ui.tsx`'s hooks and components |
| `pumi/theme` | what a UI theme is drawn with (see [Themes](themes.md)) |
| `pumi/jsx-runtime` | JSX with the renderer's elements |
| `pumi/testing` | `createTestRoot` and `TestRenderer`, for component tests |

## 4. Test it

`pumi` and `pumi/theme` have types but no code in the SDK. In a test, stand in for the parts the code under test imports with `mock.module`, then import it:

```tsx
// test/panel.test.tsx
import { describe, expect, mock, test } from "bun:test";
mock.module("pumi", () => ({
  fn: { set() {} }, request: {}, self: {},                       // what src/main.ts imports
  useTheme: () => ({ muted: "#888", surface: "#222", hover: "#333" }),
  useOption: () => [], usePlugin: () => "me/hello",
  ChoiceGrid() {}, OptionRow() {}, Row() {}, Section() {},          // the rest of what src/ui.tsx imports
}));
const { createTestRoot, hasNativeTestRenderer } = await import("pumi/testing");
const { Panel } = await import("../src/ui.tsx");

(hasNativeTestRenderer ? describe : describe.skip)("the panel", () => {
  test("counts clicks", () => {
    const { render, renderer } = createTestRoot({ width: 300, height: 100 });
    render(<Panel />);
    expect(renderer.getAllText()).toContain("0 clicks");
  });
});
```

The mock replaces the whole module, so it must have every name the tested files import; a missing one is a `SyntaxError: Export named … not found`. Logic that draws nothing is easiest to test as plain functions, without mocks.

Component tests need the app's native test renderer, which release builds of pumi leave out, so with the installed app those tests skip. [Type-check and test](plugins.md#type-check-and-test) says how to run them against a debug build.

## 5. Share it

Push the folder to a GitHub repository. Anyone can then install it:

```sh
pumi plugin add owner/repo
```

pumi installs its `dependencies` (not its `devDependencies`, so the SDK stays out). Tag a release to let people pin it with `--version`. A plugin runs with the user's permissions and no sandbox, so the README should say what it does. [Share](plugins.md#share) has the rest.
