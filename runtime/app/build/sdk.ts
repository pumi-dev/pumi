// `bun run sdk`: the npm package `@pumi-dev/sdk`, at target/sdk/package, packed into
// target/sdk/pumi-dev-sdk-<version>.tgz. A plugin outside this repository installs it as
// `"pumi": "npm:@pumi-dev/sdk@<version>"`, so `import … from "pumi"` and `"jsxImportSource": "pumi"`
// resolve to it. docs/releasing.md says how it is published.
//
//   pumi, pumi/theme              types: types/index.ts and types/theme.ts as bundled .d.ts. Their
//                                 JS only stands in for the app's (npm/stand-in.ts), which gives a
//                                 plugin both at run time: a test mocks the module.
//   pumi/jsx-runtime,             React's JSX with the renderer's intrinsics, types and JS
//   pumi/jsx-dev-runtime
//   pumi/testing                  react/testing.ts as JS (`createTestRoot`, `TestRenderer`) with
//                                 npm/addon.ts loading the addon: PUMI_ADDON, else the installed
//                                 app's. No binary is in the package.
//
// The SDK has its own version, SDK_VERSION below: bump it when what a plugin imports changes (a new
// export is a minor, a removed or changed one too while it is 0.x). The README says which app
// version it was built from (the root Cargo.toml's). React is a peer at the app's version: the
// plugin's code, the JSX and the test renderer share it.
import { cpSync, existsSync, mkdirSync, readFileSync, rmSync, statSync, symlinkSync, writeFileSync } from "node:fs"
import { dirname, join, relative } from "node:path"
import { rollup } from "rollup"
import { dts } from "rollup-plugin-dts"
import ts from "typescript"
import own from "../package.json"

const app = join(import.meta.dir, "..")
const repo = join(app, "..", "..")
const sdk = join(repo, "target", "sdk")
const out = join(sdk, "package")
/**
 * The package's version on npm. 0.1.0-alpha.0 was the first; 0.2.0 added webviewRun, webviewSnapshot,
 * the tab icon parts (TabIcon, useTabIcon, …), self.caller and a function's runsOn.
 */
const SDK_VERSION = "0.2.0"
const appVersion = /^version = "(.+)"$/m.exec(readFileSync(join(repo, "Cargo.toml"), "utf8"))?.[1]
if (!appVersion) throw new Error("no version in Cargo.toml")

/** The package's entry points: their types' source, and what their JS is. */
const entries = {
  index: { types: "types/index.ts", js: "stub" },
  theme: { types: "types/theme.ts", js: "stub" },
  "jsx-runtime": { types: "types/jsx-runtime.ts", js: "jsx-runtime.ts" },
  "jsx-dev-runtime": { types: "types/jsx-dev-runtime.ts", js: "jsx-dev-runtime.ts" },
  testing: { types: "react/testing.ts", js: "react/testing.ts" },
} as const
type Entry = keyof typeof entries
const names = Object.keys(entries) as Entry[]
/** The entries a plugin's `.tsx` reads, and so the ones an imported image's types come with. */
const withAssets: Entry[] = ["index", "theme", "jsx-runtime", "jsx-dev-runtime"]

const started = performance.now()
rmSync(out, { recursive: true, force: true })
mkdirSync(out, { recursive: true })

// Types: one .d.ts per entry, and what two of them share in `internal/`, which `exports` keeps
// out of reach. Only an entry's exports are public; a type one of them names but no entry
// exports is declared in `internal/` and can't be imported by name.
const unexported: string[] = []
const bundle = await rollup({
  input: Object.fromEntries(names.map((name) => [name, join(app, entries[name].types)])),
  plugins: [dts({ tsconfig: join(app, "tsconfig.json"), compilerOptions: { noEmit: false } })],
  external: [/^react($|\/)/],
  onwarn: (warning) => {
    // The types an entry names but no entry exports (`Props`, `Root`, …): private, and listed.
    const named = /Entry "(.+)\.d\.ts" still references private shared type exports with no public re-export: ([^.]+)\./.exec(warning.message)
    if (named) return void unexported.push(`${named[1]}: ${named[2]}`)
    throw new Error(warning.message)
  },
})
await bundle.write({ dir: out, format: "es", entryFileNames: "[name].d.ts", chunkFileNames: "internal/[name].d.ts" })
await bundle.close()
cpSync(join(app, "types", "assets.d.ts"), join(out, "assets.d.ts"))
for (const name of withAssets) {
  const file = join(out, `${name}.d.ts`)
  writeFileSync(file, `/// <reference path="./assets.d.ts" />\n${readFileSync(file, "utf8")}`)
}

// The JSX runtimes and the test renderer, with React (and the reconciler) left to the plugin's
// node_modules, so there is one React. The addon is npm/addon.ts's, not native/addon.js's.
async function buildJs(source: string, to: string): Promise<void> {
  const built = await Bun.build({
    entrypoints: [join(app, source)],
    target: "bun",
    format: "esm",
    metafile: true,
    external: ["react", "react/*", "react-reconciler", "react-reconciler/*"],
    plugins: [
      {
        name: "sdk-addon",
        setup(build) {
          build.onResolve({ filter: /(^|\/)addon(\.js)?$/ }, (args) =>
            join(dirname(args.importer), args.path).replace(/\.js$/, "") === join(app, "native", "addon") ? { path: join(app, "npm", "addon.ts") } : undefined,
          )
        },
      },
    ],
  })
  if (!built.success) throw new AggregateError(built.logs, `${source} did not build`)
  if (Object.keys(built.metafile!.inputs).some((input) => input.endsWith("native/addon.js"))) throw new Error(`${source}: bundled the app's addon loader`)
  writeFileSync(join(out, to), await built.outputs[0]!.text())
}
for (const name of names) {
  const source = entries[name].js
  if (source !== "stub") await buildJs(source, `${name}.js`)
}
await buildJs("npm/stand-in.ts", "internal/stand-in.js")

// The bundle checked as a plugin reads it (no DOM, Bun's globals), with the app's React types: a
// declaration the bundling broke is an error here. Its value exports are what the stubs stand in for.
const modules = join(sdk, "node_modules")
if (!existsSync(modules)) symlinkSync(relative(sdk, join(app, "node_modules")), modules, "junction")
const program = ts.createProgram({
  rootNames: names.map((name) => join(out, `${name}.d.ts`)),
  options: {
    target: ts.ScriptTarget.ESNext,
    module: ts.ModuleKind.Preserve,
    moduleResolution: ts.ModuleResolutionKind.Bundler,
    lib: ["lib.esnext.d.ts"],
    types: ["bun"],
    typeRoots: [join(modules, "@types")],
    strict: true,
    noEmit: true,
    skipLibCheck: false,
  },
})
const diagnostics = ts.getPreEmitDiagnostics(program).filter((d) => d.file?.fileName.startsWith(out))
if (diagnostics.length > 0) {
  const host = { getCanonicalFileName: (f: string) => f, getCurrentDirectory: () => out, getNewLine: () => "\n" }
  throw new Error(`the bundled types do not check:\n${ts.formatDiagnostics(diagnostics, host)}`)
}
const checker = program.getTypeChecker()
function valueExports(file: string): string[] {
  const source = program.getSourceFile(file)
  const module = source && checker.getSymbolAtLocation(source)
  if (!module) throw new Error(`${file}: not a module`)
  return checker
    .getExportsOfModule(module)
    .filter((symbol) => {
      const target = symbol.flags & ts.SymbolFlags.Alias ? checker.getAliasedSymbol(symbol) : symbol
      return (target.flags & ts.SymbolFlags.Value) !== 0
    })
    .map((symbol) => symbol.name)
    .sort()
}
for (const name of names) {
  if (entries[name].js !== "stub") continue
  const values = valueExports(join(out, `${name}.d.ts`))
  writeFileSync(join(out, `${name}.js`), stub(name === "index" ? "pumi" : `pumi/${name}`, values))
}

const exports: Record<string, unknown> = {}
for (const name of names) {
  exports[name === "index" ? "." : `./${name}`] = { types: `./${name}.d.ts`, default: `./${name}.js` }
}
exports["./package.json"] = "./package.json"
const pkg = {
  name: "@pumi-dev/sdk",
  version: SDK_VERSION,
  description: "Types and test tools for pumi plugins: `import … from \"pumi\"` outside the pumi repository.",
  license: own.license,
  homepage: "https://pumi.dev/docs/",
  repository: { type: "git", url: "git+https://github.com/pumi-dev/pumi.git", directory: "runtime/app" },
  keywords: ["pumi", "plugin", "sdk", "types"],
  type: "module",
  types: "./index.d.ts",
  exports,
  sideEffects: false,
  // React at the app's version, which the app gives a plugin at run time: one React for the
  // plugin's code, the JSX and the test renderer.
  peerDependencies: { react: own.dependencies.react, "@types/react": `^${own.devDependencies["@types/react"]}` },
  dependencies: { "react-reconciler": own.dependencies["react-reconciler"] },
  publishConfig: { access: "public" },
}
writeFileSync(join(out, "package.json"), `${JSON.stringify(pkg, null, 2)}\n`)
writeFileSync(join(out, "README.md"), readFileSync(join(app, "npm", "README.md"), "utf8").replaceAll("{{version}}", SDK_VERSION).replaceAll("{{app}}", appVersion))
cpSync(join(repo, "LICENSE"), join(out, "LICENSE"))

const pack = Bun.spawnSync(["npm", "pack", "--pack-destination", sdk, "--json"], { cwd: out, stderr: "inherit" })
if (pack.exitCode !== 0) process.exit(pack.exitCode ?? 1)
type Packed = { filename: string; size: number; unpackedSize: number; entryCount: number }
let packed: Packed
try {
  packed = (JSON.parse(pack.stdout.toString()) as Packed[])[0]!
} catch (err) {
  throw new Error(`npm pack printed no package list: ${pack.stdout.toString()}`, { cause: err })
}
const kb = (bytes: number) => `${(bytes / 1024).toFixed(0)} KB`
console.log(`Named by the public types but not exported (internal/):\n  ${unexported.join("\n  ")}`)
console.log(
  `target/sdk/${packed.filename}: ${kb(statSync(join(sdk, packed.filename)).size)}, ${kb(packed.unpackedSize)} unpacked in ` +
    `${packed.entryCount} files, in ${((performance.now() - started) / 1000).toFixed(1)} s`,
)

/** `module`'s JS: a stand-in (npm/stand-in.ts) for each of `values`, for a test to mock. */
function stub(module: string, values: string[]): string {
  const head = `// ${module} outside the app, which gives a plugin the real one at run time. A test mocks it:
//   mock.module(${JSON.stringify(module)}, () => ({ useTheme: () => tokens, call: async () => null }))
import { standIn } from "./internal/stand-in.js"
`
  return head + values.map((name) => `export const ${name} = standIn(${JSON.stringify(module)}, ${JSON.stringify(name)})\n`).join("")
}
