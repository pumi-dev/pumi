// `Select` from "pumi": the renderer's headless one (`SelectTrigger`, `SelectContent`, … inside
// it, styled by the caller), or, given `options`, the settings kit's, drawn as Settings draws one.
import { Select as Headless, type SelectProps } from "../react/index.ts"
import { SelectField, type SelectFieldProps } from "./settings/controls.tsx"

export type { SelectFieldProps, SelectProps }

export function Select(props: SelectProps | SelectFieldProps) {
  if ("options" in props && Array.isArray(props.options)) return <SelectField {...props} />
  return <Headless {...(props as SelectProps)} />
}
