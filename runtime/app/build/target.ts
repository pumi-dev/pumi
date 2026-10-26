// The target a release build is for: this machine's, unless `--target <triple>` (or
// `PUMI_TARGET`) names another, so one machine can build another OS's app.
//
// Triples are Rust's (`x86_64-pc-windows-msvc`). Bun's `process.platform`/`process.arch`, napi's
// file names and Bun's own `--target` each spell the same target differently; they are derived
// here, in one place, because the addon is renamed from one spelling to another after it builds.

/** The `--target` argument, else `PUMI_TARGET`, else this machine's triple. */
export function targetTriple(argv: string[] = process.argv): string {
  const at = argv.indexOf("--target")
  const named = at >= 0 ? argv[at + 1] : argv.find((arg) => arg.startsWith("--target="))?.slice(9)
  return named || process.env.PUMI_TARGET || hostTriple()
}

/** This machine's triple. */
export function hostTriple(): string {
  const arch = process.arch === "arm64" ? "aarch64" : "x86_64"
  if (process.platform === "darwin") return `${arch}-apple-darwin`
  if (process.platform === "linux") return `${arch}-unknown-linux-gnu`
  if (process.platform === "win32") return `${arch}-pc-windows-msvc`
  throw new Error(`no target triple for ${process.platform}`)
}

/** What `process.platform` and `process.arch` say for a triple: `win32-x64`. */
export function nodeTarget(triple: string): { platform: string; arch: string; abi?: string } {
  const [arch, , sys, abi] = triple.split("-")
  const platform = sys === "darwin" ? "darwin" : sys === "linux" ? "linux" : sys === "windows" ? "win32" : undefined
  if (!platform) throw new Error(`no platform for ${triple}`)
  return { platform, arch: arch === "aarch64" ? "arm64" : "x64", abi }
}

/** The addon's file name: `pumi.<platform>-<arch>.node`, the name the app dlopens it by. */
export function addonName(triple: string): string {
  const { platform, arch } = nodeTarget(triple)
  return `pumi.${platform}-${arch}.node`
}

/** What napi writes the addon as: the same, with the ABI when the target has one. */
export function napiAddonName(triple: string): string {
  const { platform, arch, abi } = nodeTarget(triple)
  return `pumi.${platform}-${arch}${abi ? `-${abi}` : ""}.node`
}

/** The app executable's name: `pumi.exe` on Windows. */
export function appName(triple: string): string {
  return nodeTarget(triple).platform === "win32" ? "pumi.exe" : "pumi"
}

/** Bun's `--target`, or null when the host is the target (Bun then builds for the host). */
export function bunTarget(triple: string): string | null {
  if (triple === hostTriple()) return null
  const { platform, arch } = nodeTarget(triple)
  return `bun-${platform === "win32" ? "windows" : platform}-${arch}`
}

/** Whether napi has to cross-compile the addon (`cargo-xwin`): a Windows target off Windows. */
export function crossCompiling(triple: string): boolean {
  return nodeTarget(triple).platform === "win32" && process.platform !== "win32"
}
