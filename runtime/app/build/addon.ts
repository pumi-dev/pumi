// Builds the addon: `bun run addon [--release] [--target <triple>]`.
//
// On macOS it also mends the dylib: Xcode 27's ld leaves the LINKEDIT string pool 4-byte
// aligned when the indirect symbol table has an odd count, and dyld then refuses the library
// ("mis-aligned LINKEDIT string pool"). The pool is moved into the padding before the code
// signature, or, in a library ld did not sign (x86_64), where the pool ends the file, the file
// grows by the padding. Then the library is signed again.
import { existsSync, readdirSync, readFileSync, renameSync, writeFileSync } from "node:fs"
import { join } from "node:path"
import { addonName, crossCompiling, napiAddonName, nodeTarget, targetTriple } from "./target.ts"

const release = process.argv.includes("--release")
const triple = targetTriple()
const app = join(import.meta.dir, "..")
const crate = join(app, "..", "..", "Cargo.toml")

// napi-rs builds the crate, writes native/addon.d.ts from its #[napi] items, and copies the
// library to native/pumi.<platform>-<arch>.node. It gathers the types while compiling, so a build
// that compiles only some of its crates writes only their types: the last whole one is kept then.
// A debug build has the GPU test renderer (`test-support`, what `bun test` needs); a release
// doesn't, and keeps the test renderer's types from the last debug build, so the file is the same
// whichever ran last.
const dts = join(app, "native", "addon.d.ts")
const before = await Bun.file(dts).text().catch(() => "")
// `bun x`, not `bunx`: on Windows `bunx` is `bunx.cmd`, which only a shell finds.
const napi = Bun.spawnSync(
  [process.execPath, "x", "napi", "build", "--manifest-path", crate, "--package-json-path", "package.json",
    "--output-dir", "native", "--dts", "addon.d.ts", "--no-js", "--platform", "--target", triple,
    ...(crossCompiling(triple) ? ["--cross-compile"] : []),
    ...(release ? ["--release"] : ["--features", "test-support"])],
  { cwd: app, stdio: ["inherit", "inherit", "inherit"] },
)
if (napi.exitCode !== 0) process.exit(napi.exitCode ?? 1)
renameAddon()
const whole = (text: string) => ["class Engine", "class GpuixRenderer", "function setCapturedKeys"].every((n) => text.includes(n))
const after = await Bun.file(dts).text()
if (whole(before) && !whole(after)) await Bun.write(dts, before)
else if (release && whole(after)) await Bun.write(dts, keepDeclarations(after, before))

if (nodeTarget(triple).platform === "darwin") {
  const path = join(app, "native", addonName(triple))
  if (alignStringPool(path)) {
    const sign = Bun.spawnSync(["codesign", "-f", "-s", "-", path], { stdio: ["inherit", "ignore", "inherit"] })
    if (sign.exitCode !== 0) process.exit(sign.exitCode ?? 1)
  }
}

/**
 * Renames what napi wrote to the name the app loads the addon by: napi spells a named target
 * `pumi.win32-x64-msvc.node`, while the app asks `process` for `pumi.win32-x64.node`.
 */
function renameAddon(): void {
  const dir = join(app, "native")
  const built = napiAddonName(triple)
  const to = addonName(triple)
  if (built !== to && existsSync(join(dir, built))) {
    renameSync(join(dir, built), join(dir, to))
    return
  }
  if (existsSync(join(dir, to))) return
  // A spelling this does not know: take the one file it left, if there is only one.
  const left = readdirSync(dir).filter((name) => /^pumi\..+\.node$/.test(name))
  if (left.length === 1) renameSync(join(dir, left[0]), join(dir, to))
}

/**
 * `next` with the declarations only `previous` has (a release build's file lacks the test
 * renderer's), each placed after the one it followed in `previous`. A declaration is a block
 * between blank lines, named by its `export` line.
 */
export function keepDeclarations(next: string, previous: string): string {
  const blocks = (text: string) => text.split("\n\n")
  const name = (block: string) => block.split("\n").find((line) => line.startsWith("export ")) ?? block
  const merged = blocks(next)
  let after = -1
  for (const block of blocks(previous)) {
    const at = merged.findIndex((b) => name(b) === name(block))
    if (at >= 0) after = at
    else merged.splice(++after, 0, block)
  }
  return merged.join("\n\n")
}

/** Moves a 64-bit Mach-O's string pool to an 8-byte offset. True when the file changed. */
export function alignStringPool(path: string): boolean {
  const aligned = alignedStringPool(readFileSync(path), path)
  if (aligned) writeFileSync(path, aligned)
  return aligned !== null
}

/** `file` with its string pool at an 8-byte offset, or null when it is there already. */
export function alignedStringPool(file: Buffer, path = "the library"): Buffer | null {
  const read = (f: Buffer) => new DataView(f.buffer, f.byteOffset, f.byteLength)
  let view = read(file)
  if (view.getUint32(0, true) !== 0xfeedfacf) throw new Error(`${path}: not a 64-bit Mach-O`)
  const ncmds = view.getUint32(16, true)
  let at = 32
  let symtab = -1
  let linkedit = -1
  let signed = false
  let limit = file.byteLength
  for (let i = 0; i < ncmds; i++) {
    const cmd = view.getUint32(at, true)
    if (cmd === 0x2) symtab = at
    if (cmd === 0x19 && file.toString("latin1", at + 8, at + 24).replace(/\0+$/, "") === "__LINKEDIT") linkedit = at
    if (cmd === 0x1d) {
      signed = true
      limit = view.getUint32(at + 8, true) // LC_CODE_SIGNATURE dataoff
    }
    at += view.getUint32(at + 4, true)
  }
  if (symtab < 0) return null
  const stroff = view.getUint32(symtab + 16, true)
  const strsize = view.getUint32(symtab + 20, true)
  const pad = (8 - (stroff % 8)) % 8
  if (pad === 0) return null
  if (stroff + strsize + pad > limit) {
    // Unsigned, with the pool last in the file and in __LINKEDIT: the file and the segment grow.
    const end = linkedit < 0 ? -1 : Number(view.getBigUint64(linkedit + 40, true) + view.getBigUint64(linkedit + 48, true))
    if (signed || stroff + strsize !== file.byteLength || end !== file.byteLength) {
      throw new Error(`${path}: no room to align the string pool`)
    }
    const grown = Buffer.alloc(file.byteLength + pad)
    file.copy(grown)
    file = grown
    view = read(file)
    const filesize = view.getBigUint64(linkedit + 48, true) + BigInt(pad)
    view.setBigUint64(linkedit + 48, filesize, true)
    const page = 0x4000n
    if (filesize > view.getBigUint64(linkedit + 32, true)) {
      view.setBigUint64(linkedit + 32, ((filesize + page - 1n) / page) * page, true)
    }
  }
  file.copyWithin(stroff + pad, stroff, stroff + strsize)
  file.fill(0, stroff, stroff + pad)
  view.setUint32(symtab + 16, stroff + pad, true)
  return file
}
