// `OptionRow`: the row Settings makes for a declared option — its title, what it means, the
// control its type calls for, and a reset once it has a value of its own. Inside Settings it reads
// and sets through Settings (kept in settings.json, found by its search); anywhere else it reads
// the registry and sets the option itself.
import { useMemo, useState } from "react"
import { call } from "../../app/engine.ts"
import { useApp } from "../../app/state.ts"
import { useRegistry } from "../state.ts"
import { SettingsContext, useSettings, type SettingsKit } from "./context.ts"
import { Button, ColorPicker, SelectField, Slider, Stepper, Switch, TextField } from "./controls.tsx"
import { Row } from "./layout.tsx"
import { choicesOf, formatNumber, parse, settable, textOf, titleOf, type Decl, type OptionValue } from "./options.ts"

export interface OptionRowProps {
  /** The option's full name (`me.remote.position`). */
  name: string
  /** Its title, over the declared label. */
  title?: string
  /** What it means, over the declared `desc`; `""` for none. */
  description?: string
  /** The control drawn instead of the one its type calls for. */
  control?: (value: OptionValue, set: (value: OptionValue) => void) => React.ReactNode
}

/** The row of a declared option, its control chosen by its type. */
export function OptionRow(props: OptionRowProps) {
  const settings = useSettings()
  return settings ? <OptionRowIn settings={settings} {...props} /> : <StandaloneOptionRow {...props} />
}

/** Outside Settings: the registry's declaration and the window's values, set with `option.set`. */
function StandaloneOptionRow(props: OptionRowProps) {
  const decls = useRegistry<Decl>("option", { name: props.name })
  const options = useApp((s) => s.options)
  const kit = useMemo<SettingsKit>(() => {
    const decl = (name: string) => decls.find((d) => d.name === name)
    return {
      decls,
      decl,
      value: (name) => (options[name] ?? decl(name)?.default ?? null) as OptionValue,
      isSet: (name) => options[name] !== undefined,
      set: (name, value) => void call("option.set", name, value, { scope: "global" }).catch((err) => console.error(`pumi: setting ${name}:`, err?.message ?? err)),
      open: () => {},
      report: (message) => message && console.error(`pumi: ${message}`),
    }
  }, [decls, options])
  return (
    <SettingsContext.Provider value={kit}>
      <OptionRowIn settings={kit} {...props} />
    </SettingsContext.Provider>
  )
}

function OptionRowIn({ settings, name, title, description, control }: OptionRowProps & { settings: SettingsKit }) {
  const d = settings.decl(name)
  if (!d || !settable(d)) return null
  const value = settings.value(name)
  const set = (next: OptionValue) => settings.set(name, next)
  const own = settings.isSet(name) && JSON.stringify(value) !== JSON.stringify(d.default ?? null)
  const reset = own ? <Button ghost icon="rotate_ccw" title="Reset to default" onClick={() => settings.set(name, null)} testId={`reset-${name}`} /> : null
  return (
    <Row
      link={name}
      testId={`option-${name}`}
      title={title ?? titleOf(d)}
      description={description ?? d.desc ?? ""}
      control={
        <>
          {reset}
          {control ? control(value, set) : <OptionControl decl={d} value={value} set={set} report={settings.report} />}
        </>
      }
    />
  )
}

/** The control an option's type calls for: a switch, a menu of its choices, a slider or a stepper, a colour, a field. */
export function OptionControl({ decl: d, value, set, report }: { decl: Decl; value: OptionValue; set(value: OptionValue): void; report(message: string | undefined): void }) {
  const [, redraw] = useState(0)
  if (d.type === "boolean" || d.control === "toggle") {
    const on = typeof value === "boolean" ? value : typeof value === "number" ? value !== 0 : false
    return <Switch checked={on} aria-label={titleOf(d)} testId={`switch-${d.name}`} onChange={(next) => set(d.type === "number" ? (next ? 1 : 0) : next)} />
  }
  // A colour: its placeholder is the one an empty value stands for (the theme's own).
  if (d.control === "color") return <ColorPicker value={textOf(value)} empty={d.placeholder ?? undefined} onChange={set} onInvalid={(message) => report(message && `${titleOf(d)}: ${message}`)} testId={`color-${d.name}`} />
  const choices = choicesOf(d)
  if (choices.length) return <SelectField options={choices} value={textOf(value)} onChange={(picked) => set(picked)} testId={`select-${d.name}`} />
  if (d.type === "number" && typeof d.min === "number" && typeof d.max === "number") {
    const step = d.step && d.step > 0 ? d.step : 1
    const unit = d.unit ?? ""
    const n = typeof value === "number" ? value : typeof d.default === "number" ? d.default : d.min
    if (d.control === "slider") return <Slider value={n} min={d.min} max={d.max} step={step} onChange={set} format={(v) => formatNumber(v, step, unit)} testId={`slider-${d.name}`} />
    return <Stepper value={n} min={d.min} max={d.max} step={step} unit={unit} onChange={set} testId={`stepper-${d.name}`} />
  }
  const wide = d.type === "table" || d.type === "any" || d.control === "json"
  return (
    <TextField
      value={textOf(value)}
      placeholder={d.placeholder ?? ""}
      width={wide ? 320 : 240}
      code={wide}
      testId={`field-${d.name}`}
      onCommit={(text) => {
        const parsed = parse(d, text)
        if ("error" in parsed) {
          report(`${titleOf(d)}: ${parsed.error}`)
          return redraw((n) => n + 1)
        }
        report(undefined)
        set(parsed.value)
      }}
    />
  )
}
