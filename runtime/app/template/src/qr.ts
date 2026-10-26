// A QR code, made here: no package to install. Byte mode, error correction L, versions 1–5 (up to
// 106 bytes: a URL), mask 0 — what any phone's camera reads. `qr(text)` is its modules, row by
// row (`true` dark); draw them as a grid of `div`s, or as an SVG (`qrSvg`) in an `<img>`.

const DATA = [0, 19, 34, 55, 80, 108] // data codewords, by version (EC level L)
const ECC = [0, 7, 10, 15, 20, 26] // error-correction codewords
const ALIGN = [0, 0, 18, 22, 26, 30] // the alignment pattern's centre (versions 2–5)

/** GF(256) product, modulo x⁸ + x⁴ + x³ + x² + 1. */
function mul(x: number, y: number): number {
  let z = 0
  for (let i = 7; i >= 0; i--) {
    z = (z << 1) ^ ((z >>> 7) * 0x11d)
    z ^= ((y >>> i) & 1) * x
  }
  return z & 0xff
}

/** The Reed–Solomon codewords of `data`, `degree` of them. */
function ecc(data: number[], degree: number): number[] {
  const divisor = new Array<number>(degree).fill(0)
  divisor[degree - 1] = 1
  let root = 1
  for (let i = 0; i < degree; i++) {
    for (let j = 0; j < degree; j++) {
      divisor[j] = mul(divisor[j]!, root)
      if (j + 1 < degree) divisor[j]! ^= divisor[j + 1]!
    }
    root = mul(root, 2)
  }
  const out = new Array<number>(degree).fill(0)
  for (const b of data) {
    const factor = b ^ out.shift()!
    out.push(0)
    divisor.forEach((coef, i) => (out[i]! ^= mul(coef, factor)))
  }
  return out
}

/** The modules of `text`'s QR code, row by row. Throws when it is too long (over 106 bytes). */
export function qr(text: string): boolean[][] {
  const bytes = [...new TextEncoder().encode(text)]
  const version = [1, 2, 3, 4, 5].find((v) => 4 + 8 + bytes.length * 8 <= DATA[v]! * 8)
  if (!version) throw new Error(`a QR code here holds 106 bytes; this is ${bytes.length}`)
  // The bits: byte mode, the count, the bytes, a terminator, then padding.
  const bits: number[] = []
  const put = (value: number, n: number) => {
    for (let i = n - 1; i >= 0; i--) bits.push((value >>> i) & 1)
  }
  put(0b0100, 4)
  put(bytes.length, 8)
  for (const b of bytes) put(b, 8)
  const capacity = DATA[version]! * 8
  put(0, Math.min(4, capacity - bits.length))
  put(0, (8 - (bits.length % 8)) % 8)
  const data: number[] = []
  for (let i = 0; i < bits.length; i += 8) data.push(bits.slice(i, i + 8).reduce((a, b) => (a << 1) | b, 0))
  for (let pad = 0xec; data.length < DATA[version]!; pad ^= 0xec ^ 0x11) data.push(pad)
  const codewords = [...data, ...ecc(data, ECC[version]!)]

  const size = 17 + 4 * version
  const dark = Array.from({ length: size }, () => new Array<boolean>(size).fill(false))
  const fixed = Array.from({ length: size }, () => new Array<boolean>(size).fill(false))
  const set = (x: number, y: number, on: boolean) => {
    dark[y]![x] = on
    fixed[y]![x] = true
  }
  for (let i = 0; i < size; i++) {
    set(6, i, i % 2 === 0)
    set(i, 6, i % 2 === 0)
  }
  for (const [cx, cy] of [[3, 3], [size - 4, 3], [3, size - 4]] as const) {
    for (let dy = -4; dy <= 4; dy++) {
      for (let dx = -4; dx <= 4; dx++) {
        const x = cx + dx
        const y = cy + dy
        const d = Math.max(Math.abs(dx), Math.abs(dy))
        if (x >= 0 && x < size && y >= 0 && y < size) set(x, y, d !== 2 && d !== 4)
      }
    }
  }
  const a = ALIGN[version]!
  if (a) for (let dy = -2; dy <= 2; dy++) for (let dx = -2; dx <= 2; dx++) set(a + dx, a + dy, Math.max(Math.abs(dx), Math.abs(dy)) !== 1)
  // The format: level L (01) and mask 0, with its BCH bits.
  let rem = 0b01000
  for (let i = 0; i < 10; i++) rem = (rem << 1) ^ ((rem >>> 9) * 0x537)
  const format = ((0b01000 << 10) | rem) ^ 0x5412
  const bit = (i: number) => ((format >>> i) & 1) === 1
  for (let i = 0; i <= 5; i++) set(8, i, bit(i))
  set(8, 7, bit(6))
  set(8, 8, bit(7))
  set(7, 8, bit(8))
  for (let i = 9; i < 15; i++) set(14 - i, 8, bit(i))
  for (let i = 0; i < 8; i++) set(size - 1 - i, 8, bit(i))
  for (let i = 8; i < 15; i++) set(8, size - 15 + i, bit(i))
  set(8, size - 8, true)
  // The codewords, in two-column strips from the bottom right, up and down in turn; then mask 0.
  let at = 0
  for (let right = size - 1; right >= 1; right -= 2) {
    if (right === 6) right = 5
    for (let vert = 0; vert < size; vert++) {
      for (let j = 0; j < 2; j++) {
        const x = right - j
        const y = ((right + 1) & 2) === 0 ? size - 1 - vert : vert
        if (fixed[y]![x]) continue
        const on = at < codewords.length * 8 && ((codewords[at >>> 3]! >>> (7 - (at & 7))) & 1) === 1
        at++
        dark[y]![x] = on !== ((x + y) % 2 === 0)
      }
    }
  }
  return dark
}

/** The code as an SVG, with the quiet zone round it: for `<img src={qrSvg(url)}>` (as a data URL). */
export function qrSvg(text: string, color = "#000000", background = "#ffffff"): string {
  const m = qr(text)
  const n = m.length + 8
  let path = ""
  m.forEach((row, y) => row.forEach((on, x) => on && (path += `M${x + 4} ${y + 4}h1v1h-1z`)))
  const svg = `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 ${n} ${n}" shape-rendering="crispEdges"><rect width="${n}" height="${n}" fill="${background}"/><path d="${path}" fill="${color}"/></svg>`
  return `data:image/svg+xml;base64,${btoa(svg)}`
}
