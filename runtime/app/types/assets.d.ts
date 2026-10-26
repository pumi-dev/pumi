// An asset a plugin imports (`import floor from "./art/floor.png"`) is its absolute path, as Bun
// gives it: what `<img src>`, `backgroundImage`, `borderImage` and a theme's `fonts` take.
declare module "*.png" {
  const path: string
  export default path
}
declare module "*.jpg" {
  const path: string
  export default path
}
declare module "*.jpeg" {
  const path: string
  export default path
}
declare module "*.gif" {
  const path: string
  export default path
}
declare module "*.webp" {
  const path: string
  export default path
}
declare module "*.bmp" {
  const path: string
  export default path
}
declare module "*.svg" {
  const path: string
  export default path
}
declare module "*.ttf" {
  const path: string
  export default path
}
declare module "*.otf" {
  const path: string
  export default path
}
