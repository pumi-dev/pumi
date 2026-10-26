// The settings kit's controls: what Settings' own pages are drawn with, and what a plugin's page
// or section uses to look the same. Each takes its colours from the theme (`useTheme`).
import { Children, isValidElement, memo, useEffect, useRef, useState, type ReactNode } from "react"
import type { EventPayload } from "../../native/addon"
import type { KeyEvent, PressEvent, StyleDesc } from "../../native/host.ts"
import { motion, Select as HeadlessSelect, SelectContent, SelectItem, SelectTrigger, SelectValue } from "../../react/index.ts"
import { assets } from "../../app/assets.ts"
import { chordOf } from "../../app/keymap.ts"
import { useHoldKeys } from "../../app/commands.ts"
import { keyGlyphs } from "../actions.ts"
import { Icon } from "../elements.tsx"
import { haptic } from "../haptic.ts"
import { useMetrics, useTheme } from "../theme.ts"
import { formatNumber, snap } from "./options.ts"

/** A size of text at the theme's scale: `n` at the default 13 pt. */
export function useTextSize(): (n: number) => number {
  const base = useMetrics().fontSize
  return (n) => Math.round((n * base) / 13)
}

const pressed = (e: KeyEvent) => e.key === "enter" || e.key === "space"

// ── Switch ───────────────────────────────────────────────────────────

export interface SwitchProps {
  checked: boolean
  onChange(checked: boolean): void
  disabled?: boolean
  "aria-label"?: string
  testId?: string
}

/** A switch: on or off. It taps the trackpad as it turns. */
export function Switch({ checked, onChange, disabled, testId, ...aria }: SwitchProps) {
  const t = useTheme()
  const toggle = () => {
    if (disabled) return
    haptic("generic")
    onChange(!checked)
  }
  return (
    <div
      testId={testId}
      role="switch"
      aria-label={aria["aria-label"]}
      aria-selected={checked}
      tabIndex={disabled ? -1 : 0}
      onClick={toggle}
      onKeyDown={(e) => {
        if (!pressed(e)) return
        e.preventDefault()
        toggle()
      }}
      style={{
        display: "flex",
        flexDirection: "row",
        justifyContent: checked ? "flex-end" : "flex-start",
        alignItems: "center",
        flexShrink: 0,
        width: 34,
        height: 20,
        padding: 2,
        borderRadius: 10,
        backgroundColor: checked ? t.accent : t.active,
        opacity: disabled ? 0.5 : 1,
        cursor: disabled ? "default" : "pointer",
        keyboardFocusDim: false,
        focusVisible: { outlineWidth: 2, outlineColor: t.accent, outlineOffset: 2 },
      }}
    >
      <div style={{ pointerEvents: "none", width: 16, height: 16, borderRadius: 8, backgroundColor: "#ffffff", boxShadow: { offsetX: 0, offsetY: 1, blurRadius: 2, spreadRadius: 0, color: "#00000040" } }} />
    </div>
  )
}

// ── Spinner ──────────────────────────────────────────────────────────

/** Something is under way: an arc turning round, in the theme's faint colour. */
export function Spinner({ size = 14, testId }: { size?: number; testId?: string }) {
  const t = useTheme()
  const src = assets.icon("status_spinner")
  if (!src) return null
  return (
    <motion.svg
      testId={testId}
      src={src}
      animate={{ rotate: [0, 360] }}
      // A turn a second at 24 frames a second, not the display's rate.
      transition={{ duration: 1, repeat: Infinity, ease: "linear", fps: 24 }}
      style={{ width: size, height: size, flexShrink: 0, color: t.muted }}
    />
  )
}

// ── Slider ───────────────────────────────────────────────────────────

export interface SliderProps {
  value: number
  min: number
  max: number
  step?: number
  /** Each step while it is dragged, and each arrow key. */
  onChange(value: number): void
  /** Where a drag ended. */
  onCommit?(value: number): void
  width?: number
  disabled?: boolean
  /** The value as it is written beside the track; none without. */
  format?: (value: number) => string
  testId?: string
}

/** A value along a track, dragged or stepped with the arrow keys. A step taps the trackpad, and an end taps it harder. */
export function Slider({ value, min, max, step = 1, onChange, onCommit, width = 160, disabled, format, testId }: SliderProps) {
  const t = useTheme()
  const ts = useTextSize()
  const track = useRef<{ x: number; width: number }>({ x: 0, width })
  const [dragged, setDragged] = useState<number>()
  const dragging = useRef(false)
  const last = useRef(value)
  const shown = dragged ?? value
  const span = max - min || 1
  const fraction = Math.min(1, Math.max(0, (shown - min) / span))

  const move = (next: number) => {
    if (next === last.current) return
    last.current = next
    haptic(next === min || next === max ? "alignment" : "levelChange")
    onChange(next)
  }
  const at = (x: number | undefined) => snap(min + ((x ?? 0) - track.current.x) / (track.current.width || 1) * span, min, max, step)
  const end = () => {
    if (!dragging.current) return
    dragging.current = false
    const done = last.current
    setDragged(undefined)
    onCommit?.(done)
  }
  return (
    <div style={{ display: "flex", flexDirection: "row", alignItems: "center", gap: 10, flexShrink: 0 }}>
      {format ? <text style={{ fontSize: ts(13), color: t.text, minWidth: 48, textAlign: "right", whiteSpace: "nowrap" }}>{format(shown)}</text> : null}
      <div
        testId={testId}
        role="slider"
        aria-valuetext={format ? format(shown) : String(shown)}
        tabIndex={disabled ? -1 : 0}
        onLayout={(e: EventPayload & { width?: number }) => (track.current = { x: e.x ?? 0, width: e.width ?? width })}
        onMouseDown={(e: PressEvent) => {
          if (disabled || (e.button ?? 0) !== 0) return
          dragging.current = true
          last.current = value
          const next = at(e.x)
          setDragged(next)
          move(next)
        }}
        onMouseMove={(e: EventPayload) => {
          if (!dragging.current) return
          // The release happened where it was not heard.
          if (e.pressedButton !== 0) return end()
          const next = at(e.x)
          setDragged(next)
          move(next)
        }}
        onMouseUp={end}
        onKeyDown={(e) => {
          if (disabled) return
          const by = e.key === "left" || e.key === "down" ? -step : e.key === "right" || e.key === "up" ? step : 0
          if (!by) return
          e.preventDefault()
          last.current = value
          const next = snap(value + by, min, max, step)
          move(next)
          onCommit?.(next)
        }}
        style={{ position: "relative", width, height: 20, flexShrink: 0, display: "flex", flexDirection: "row", alignItems: "center", opacity: disabled ? 0.5 : 1, cursor: "pointer", keyboardFocusDim: false, focusVisible: { outlineWidth: 2, outlineColor: t.accent, outlineOffset: 2 } }}
      >
        <div style={{ pointerEvents: "none", position: "absolute", left: 0, top: 8, width, height: 4, borderRadius: 2, backgroundColor: t.active }} />
        <div style={{ pointerEvents: "none", position: "absolute", left: 0, top: 8, width: Math.round(fraction * width), height: 4, borderRadius: 2, backgroundColor: t.accent }} />
        <div style={{ pointerEvents: "none", position: "absolute", left: Math.round(fraction * (width - 16)), top: 2, width: 16, height: 16, borderRadius: 8, backgroundColor: "#ffffff", borderWidth: 1, borderColor: t.border, boxShadow: { offsetX: 0, offsetY: 1, blurRadius: 3, spreadRadius: 0, color: "#00000040" } }} />
      </div>
    </div>
  )
}

// ── Stepper ──────────────────────────────────────────────────────────

export interface StepperProps {
  value: number
  onChange(value: number): void
  min?: number
  max?: number
  step?: number
  unit?: string
  /** How the value is written; its step's decimals and its unit by default. */
  format?: (value: number) => string
  testId?: string
}

/** A number one step down or up at a time. */
export function Stepper({ value, onChange, min = -Infinity, max = Infinity, step = 1, unit = "", format, testId }: StepperProps) {
  const t = useTheme()
  const ts = useTextSize()
  const m = useMetrics()
  const go = (by: number) => {
    const next = Math.round(Math.min(max, Math.max(min, value + by)) * 1e9) / 1e9
    if (next !== value) onChange(next)
  }
  const button = (label: string, by: number, enabled: boolean) => (
    <div
      testId={testId ? `${testId}-${by < 0 ? "down" : "up"}` : undefined}
      role="button"
      aria-label={by < 0 ? "Less" : "More"}
      tabIndex={enabled ? 0 : -1}
      onClick={() => enabled && go(by)}
      onKeyDown={(e) => {
        if (!pressed(e)) return
        e.preventDefault()
        if (enabled) go(by)
      }}
      style={{ width: 26, height: 26, display: "flex", alignItems: "center", justifyContent: "center", opacity: enabled ? 1 : 0.4, cursor: enabled ? "pointer" : "default", hover: enabled ? { backgroundColor: t.hover } : {} }}
    >
      <text style={{ fontSize: ts(14), color: t.text }}>{label}</text>
    </div>
  )
  return (
    <div testId={testId} style={{ display: "flex", flexDirection: "row", alignItems: "center", flexShrink: 0, borderWidth: 1, borderColor: t.border, borderRadius: m.radiusSmall, backgroundColor: t.editor, overflow: "hidden" }}>
      {button("−", -step, value > min)}
      <text style={{ fontSize: ts(13), color: t.text, minWidth: 56, textAlign: "center", whiteSpace: "nowrap" }}>{format ? format(value) : formatNumber(value, step, unit)}</text>
      {button("+", step, value < max)}
    </div>
  )
}

// ── TextField ────────────────────────────────────────────────────────

export interface TextFieldProps {
  value: string
  /** Enter, or the field losing the focus with something changed. */
  onCommit(value: string): void
  /** Each edit. */
  onChange?(value: string): void
  placeholder?: string
  /** Its width; `"100%"` fills the space it is in. */
  width?: number | "100%"
  /** Monospace, for code and JSON. */
  code?: boolean
  /** A search field: a glass before it, and Escape empties it. */
  search?: boolean
  autoFocus?: boolean
  testId?: string
}

/** A line of text, committed on Enter or as the focus leaves it. */
export function TextField({ value, onCommit, onChange, placeholder, width = 240, code, search, autoFocus, testId }: TextFieldProps) {
  const t = useTheme()
  const ts = useTextSize()
  const m = useMetrics()
  const [text, setText] = useState(value)
  const [focused, setFocused] = useState(false)
  // What the option is now, while the field is not being typed in.
  useEffect(() => {
    if (!focused) setText(value)
  }, [value, focused])
  const commit = () => {
    if (text !== value) onCommit(text)
  }
  return (
    <div style={{ display: "flex", flexDirection: "row", alignItems: "center", gap: 6, width, maxWidth: "100%", flexShrink: 1, minWidth: 80, height: 28, paddingLeft: 8, paddingRight: 8, borderRadius: m.radiusSmall, borderWidth: 1, borderColor: focused ? t.accent : t.border, backgroundColor: t.editor }}>
      {search ? <Icon name="search" size={13} color={t.faint} /> : null}
      <input
        testId={testId}
        autoFocus={autoFocus}
        value={text}
        placeholder={placeholder}
        onChange={(e) => {
          const next = String((e as { value?: string }).value ?? "")
          setText(next)
          onChange?.(next)
        }}
        onSubmit={() => onCommit(text)}
        onFocus={() => setFocused(true)}
        onBlur={() => {
          setFocused(false)
          commit()
        }}
        onKeyDown={(e) => {
          if (!search || e.key !== "escape" || !text) return
          e.preventDefault()
          e.stopPropagation()
          setText("")
          onChange?.("")
          onCommit("")
        }}
        style={{ flexGrow: 1, minWidth: 0, height: 26, fontSize: ts(13), color: t.text, ...(code ? { fontFamily: "Menlo" } : {}) }}
      />
    </div>
  )
}

// ── Button ───────────────────────────────────────────────────────────

export interface ButtonProps {
  children?: ReactNode
  onClick(): void
  /** The thing to do here: in the accent. */
  primary?: boolean
  /** It ends or removes something. */
  danger?: boolean
  /** No background until hovered. */
  ghost?: boolean
  disabled?: boolean
  /** Busy: drawn dimmed, and it does nothing. */
  loading?: boolean
  /** One of the app's icons, before the label (or alone). */
  icon?: string
  /** Its tooltip; its accessible label when it has only an icon. */
  title?: string
  testId?: string
}

export function Button({ children, onClick, primary, danger, ghost, disabled, loading, icon, title, testId }: ButtonProps) {
  const t = useTheme()
  const ts = useTextSize()
  const m = useMetrics()
  const off = disabled || loading
  const fill = danger ? t.error : primary ? t.accent : ghost ? "transparent" : t.active
  const ink = danger ? t.window : primary ? t.accentText : t.text
  const only = children === undefined || children === null
  return (
    <div
      testId={testId}
      role="button"
      title={title}
      aria-label={typeof children === "string" ? children : title}
      tabIndex={off ? -1 : 0}
      onClick={(e) => {
        e.stopPropagation()
        if (!off) onClick()
      }}
      onKeyDown={(e) => {
        if (!pressed(e)) return
        e.preventDefault()
        if (!off) onClick()
      }}
      style={{
        display: "flex",
        flexDirection: "row",
        alignItems: "center",
        justifyContent: "center",
        gap: 6,
        flexShrink: 0,
        height: 26,
        paddingLeft: only ? 5 : 12,
        paddingRight: only ? 5 : 12,
        borderRadius: m.radiusSmall,
        backgroundColor: fill,
        opacity: off ? 0.55 : 1,
        cursor: off ? "default" : "pointer",
        keyboardFocusDim: false,
        hover: off ? {} : primary || danger ? { opacity: 0.9 } : { backgroundColor: t.hover },
        focusVisible: { outlineWidth: 2, outlineColor: t.accent, outlineOffset: 1 },
      }}
    >
      {icon ? <Icon name={icon} size={13} color={ghost && !primary ? t.muted : ink} /> : null}
      {only ? null : typeof children === "string" ? <text style={{ fontSize: ts(12), fontWeight: 500, color: ink, whiteSpace: "nowrap" }}>{loading ? `${children}…` : children}</text> : children}
    </div>
  )
}

// ── Select ───────────────────────────────────────────────────────────

export interface SelectOption {
  value: string
  label?: string
}

export interface SelectFieldProps {
  options: SelectOption[]
  value: string | undefined
  onChange(value: string): void
  placeholder?: string
  width?: number
  disabled?: boolean
  testId?: string
}

/** One of a list, picked from a menu: the headless `Select`, drawn as Settings draws one. */
export function SelectField({ options, value, onChange, placeholder, width = 220, disabled, testId }: SelectFieldProps) {
  const t = useTheme()
  const ts = useTextSize()
  const m = useMetrics()
  const items = options.map((o) => ({ value: o.value, label: o.label ?? o.value }))
  return (
    <HeadlessSelect items={items} value={value} onValueChange={onChange} disabled={disabled}>
      <SelectTrigger
        testId={testId}
        style={({ open }) => ({ display: "flex", flexDirection: "row", alignItems: "center", justifyContent: "space-between", gap: 6, width, height: 28, paddingLeft: 10, paddingRight: 8, borderRadius: m.radiusSmall, borderWidth: 1, borderColor: open ? t.accent : t.border, backgroundColor: t.editor, cursor: "pointer", opacity: disabled ? 0.5 : 1 })}
      >
        <SelectValue placeholder={placeholder ?? ""} style={{ flexGrow: 1, minWidth: 0, fontSize: ts(13), color: t.text, whiteSpace: "nowrap", textOverflow: "ellipsis", overflow: "hidden" }} />
        <Icon name="chevron_up_down" size={12} color={t.muted} />
      </SelectTrigger>
      <SelectContent
        sideOffset={4}
        style={{ display: "flex", flexDirection: "column", width, maxHeight: 320, overflowY: "scroll", padding: 4, borderRadius: m.radius, borderWidth: 1, borderColor: t.border, backgroundColor: t.surface, scrollbarColor: t.faint, boxShadow: { offsetX: 0, offsetY: 8, blurRadius: 24, spreadRadius: 0, color: "#00000060" } }}
      >
        {items.map((item) => (
          <SelectItem
            key={item.value}
            value={item.value}
            style={({ highlighted, selected }) => ({ display: "flex", flexDirection: "row", alignItems: "center", justifyContent: "space-between", gap: 8, height: 26, flexShrink: 0, paddingLeft: 8, paddingRight: 8, borderRadius: m.radiusSmall, backgroundColor: highlighted ? t.hover : selected ? t.selected : "transparent", cursor: "pointer" })}
          >
            {({ selected }) => (
              <>
                <text style={{ fontSize: ts(13), color: t.text, whiteSpace: "nowrap", textOverflow: "ellipsis", overflow: "hidden" }}>{item.label}</text>
                {selected ? <Icon name="check" size={12} color={t.accent} /> : null}
              </>
            )}
          </SelectItem>
        ))}
      </SelectContent>
    </HeadlessSelect>
  )
}

// ── KeyRecorder ──────────────────────────────────────────────────────

/** How long a pause ends a sequence (`ctrl+x ctrl+f`), and how many chords one has at most. */
export const RECORD_PAUSE = 900
export const MAX_SEQUENCE = 3
const MODIFIER_KEYS = ["shift", "control", "ctrl", "alt", "option", "platform", "cmd", "function", "fn"]

export interface KeyRecorderProps {
  /** The keys pressed, as the keymap writes them (`cmd+k`, `ctrl+x ctrl+f`). */
  onRecord(keys: string): void
  /** Escape, or the focus leaving it. */
  onCancel(): void
  testId?: string
}

/**
 * Takes the keys pressed while it is shown (the app's own bindings let them through): a chord, or
 * up to three in a row, ended by a pause. Escape cancels.
 */
export function KeyRecorder({ onRecord, onCancel, testId }: KeyRecorderProps) {
  const t = useTheme()
  const ts = useTextSize()
  const m = useMetrics()
  useHoldKeys()
  const [chords, setChords] = useState<string[]>([])
  const timer = useRef<ReturnType<typeof setTimeout>>(undefined)
  const latest = useRef({ onRecord, chords })
  latest.current = { onRecord, chords }
  useEffect(() => () => clearTimeout(timer.current), [])
  const done = (list: string[]) => {
    clearTimeout(timer.current)
    if (list.length) latest.current.onRecord(list.join(" "))
  }
  return (
    <div
      testId={testId}
      autoFocus
      tabIndex={0}
      aria-label="Press keys"
      onBlur={onCancel}
      onKeyDown={(e) => {
        e.preventDefault()
        e.stopPropagation()
        const chord = chordOf(e as { key?: string; modifiers?: any })
        if (chord === "escape") {
          clearTimeout(timer.current)
          return onCancel()
        }
        if (MODIFIER_KEYS.includes(chord.split("+").at(-1)!)) return
        const list = [...latest.current.chords, chord]
        setChords(list)
        clearTimeout(timer.current)
        if (list.length >= MAX_SEQUENCE) return done(list)
        timer.current = setTimeout(() => done(latest.current.chords), RECORD_PAUSE)
      }}
      style={{ display: "flex", flexDirection: "row", alignItems: "center", height: 24, paddingLeft: 8, paddingRight: 8, borderRadius: m.radiusSmall, borderWidth: 1, borderColor: t.accent, keyboardFocusDim: false }}
    >
      <text style={{ fontSize: ts(12), color: t.text, whiteSpace: "nowrap" }}>{chords.length ? `${chords.map(keyGlyphs).join("  ")} …` : "Press keys…"}</text>
    </div>
  )
}

/** A key's chip: its glyphs, a warning border when another command has it too, and a remove button. */
export function KeyChip({ keys, conflicts = [], onRemove }: { keys: string; conflicts?: string[]; onRemove?(): void }) {
  const t = useTheme()
  const ts = useTextSize()
  const m = useMetrics()
  return (
    <div
      title={conflicts.length ? `Also bound to ${conflicts.join(", ")}` : undefined}
      style={{ display: "flex", flexDirection: "row", alignItems: "center", gap: 2, height: 22, paddingLeft: 8, paddingRight: onRemove ? 2 : 8, flexShrink: 0, borderRadius: m.radiusSmall, borderWidth: 1, borderColor: conflicts.length ? t.warning : t.border, backgroundColor: t.editor }}
    >
      <text style={{ fontSize: ts(12), color: t.text, whiteSpace: "nowrap" }}>{keyGlyphs(keys)}</text>
      {onRemove ? (
        <div role="button" aria-label="Remove" title="Remove" onClick={onRemove} style={{ width: 16, height: 16, display: "flex", alignItems: "center", justifyContent: "center", borderRadius: 3, cursor: "pointer", hover: { backgroundColor: t.hover } }}>
          <Icon name="close" size={9} color={t.muted} />
        </div>
      ) : null}
    </div>
  )
}

// ── ChoiceGrid ───────────────────────────────────────────────────────

export interface Choice {
  value: string
  label?: string
  /** One of the app's icons in the cell. */
  icon?: string
}

export interface ChoiceGridProps {
  choices: Choice[]
  value: string | undefined
  onChange(value: string): void
  /** Cells across; the rows are as many as the choices need. */
  columns: number
  /** A cell's size in px. */
  cell?: { width: number; height: number }
  testId?: string
}

/** Choices laid out in rows and columns: where on the screen a notification shows. */
export function ChoiceGrid({ choices, value, onChange, columns, cell = { width: 36, height: 24 }, testId }: ChoiceGridProps) {
  const t = useTheme()
  const ts = useTextSize()
  const m = useMetrics()
  const across = Math.max(1, Math.floor(columns))
  const rows: Choice[][] = []
  for (let at = 0; at < choices.length; at += across) rows.push(choices.slice(at, at + across))
  return (
    <div testId={testId} role="radiogroup" style={{ display: "flex", flexDirection: "column", gap: 4, padding: 4, flexShrink: 0, borderRadius: m.radius, borderWidth: 1, borderColor: t.border, backgroundColor: t.editor }}>
      {rows.map((row, r) => (
        <div key={r} style={{ display: "flex", flexDirection: "row", gap: 4 }}>
          {row.map((choice) => {
            const on = choice.value === value
            return (
              <div
                key={choice.value}
                testId={testId ? `${testId}-${choice.value}` : undefined}
                role="radio"
                aria-selected={on}
                aria-label={choice.label ?? choice.value}
                title={choice.label ?? choice.value}
                tabIndex={0}
                onClick={() => onChange(choice.value)}
                onKeyDown={(e) => {
                  if (!pressed(e)) return
                  e.preventDefault()
                  onChange(choice.value)
                }}
                style={{ display: "flex", alignItems: "center", justifyContent: "center", width: cell.width, height: cell.height, borderRadius: m.radiusSmall, backgroundColor: on ? t.accent : t.active, cursor: "pointer", keyboardFocusDim: false, hover: on ? {} : { backgroundColor: t.hover }, focusVisible: { outlineWidth: 2, outlineColor: t.accent, outlineOffset: 1 } }}
              >
                {choice.icon ? <Icon name={choice.icon} size={12} color={on ? t.accentText : t.muted} /> : null}
                {!choice.icon && choice.label && cell.width >= 48 ? <text style={{ fontSize: ts(11), color: on ? t.accentText : t.muted }}>{choice.label}</text> : null}
              </div>
            )
          })}
        </div>
      ))}
    </div>
  )
}

// ── ColorPicker ──────────────────────────────────────────────────────

export interface ColorSwatch {
  /** A `#rrggbb`. */
  color: string
  label: string
}

/** The colours a picker offers when it is given none. */
export const COLOR_SWATCHES: ColorSwatch[] = [
  { color: "#3474f0", label: "Blue" },
  { color: "#8b5cf6", label: "Purple" },
  { color: "#e5508f", label: "Pink" },
  { color: "#ef8a35", label: "Orange" },
  { color: "#3fae5c", label: "Green" },
]

/** `text` as a `#rrggbb` (`#abc`, `ABCDEF` and `#AbCdEf` too), lower case; `""` when empty; nothing when it is not a colour. */
export function parseHex(text: string): string | undefined {
  const s = text.trim().replace(/^#/, "").toLowerCase()
  if (!s) return ""
  if (/^[0-9a-f]{3}$/.test(s)) return `#${[...s].map((c) => c + c).join("")}`
  if (/^[0-9a-f]{6}$/.test(s)) return `#${s}`
  return undefined
}

export interface ColorPickerProps {
  /** A `#rrggbb`, or `""` for `empty`. */
  value: string
  /** A swatch's colour or a typed one; `""` for the swatch that is `empty`. */
  onChange(value: string): void
  /** Text typed that is not a colour; `undefined` once one is. */
  onInvalid?(message: string | undefined): void
  swatches?: ColorSwatch[]
  /** The colour `""` stands for: its swatch is picked while the value is empty, and picking it sets `""`. */
  empty?: string
  testId?: string
}

/** A colour: one of a few swatches, or any typed as hex in the field the last one opens. */
export function ColorPicker({ value, onChange, onInvalid, swatches = COLOR_SWATCHES, empty, testId }: ColorPickerProps) {
  const t = useTheme()
  const ts = useTextSize()
  const fallback = (empty && parseHex(empty)) || ""
  const current = parseHex(value) || fallback
  const preset = swatches.find((s) => s.color.toLowerCase() === current)
  // Custom is picked once pressed, or while the value is none of the swatches.
  const [typing, setTyping] = useState(false)
  const custom = typing || (!!current && !preset)
  const own = custom && !preset && !!current
  const pick = (color: string) => {
    haptic("generic")
    setTyping(false)
    onInvalid?.(undefined)
    onChange(fallback && color.toLowerCase() === fallback ? "" : color)
  }
  const ring = (on: boolean): StyleDesc => ({
    display: "flex",
    alignItems: "center",
    justifyContent: "center",
    width: 26,
    height: 26,
    flexShrink: 0,
    borderRadius: 13,
    borderWidth: 2,
    borderColor: on ? t.text : "transparent",
    cursor: "pointer",
    keyboardFocusDim: false,
    hover: on ? {} : { borderColor: t.faint },
    focusVisible: { outlineWidth: 2, outlineColor: t.accent, outlineOffset: 1 },
  })
  const keys = (go: () => void) => (e: KeyEvent) => {
    if (!pressed(e)) return
    e.preventDefault()
    go()
  }
  return (
    <div testId={testId} role="radiogroup" style={{ display: "flex", flexDirection: "row", alignItems: "center", gap: 4, flexShrink: 0 }}>
      {swatches.map((s) => {
        const on = !custom && s === preset
        return (
          <div
            key={s.color}
            testId={testId ? `${testId}-${s.color.replace("#", "")}` : undefined}
            role="radio"
            aria-selected={on}
            aria-label={s.label}
            title={s.label}
            tabIndex={0}
            onClick={() => pick(s.color)}
            onKeyDown={keys(() => pick(s.color))}
            style={ring(on)}
          >
            <div style={{ pointerEvents: "none", width: 18, height: 18, borderRadius: 9, backgroundColor: s.color, borderWidth: 1, borderColor: "#00000030" }} />
          </div>
        )
      })}
      <div
        testId={testId ? `${testId}-custom` : undefined}
        role="radio"
        aria-selected={custom}
        aria-label="Custom colour"
        title="Custom colour"
        tabIndex={0}
        onClick={() => setTyping(true)}
        onKeyDown={keys(() => setTyping(true))}
        style={ring(custom)}
      >
        <div style={{ pointerEvents: "none", display: "flex", alignItems: "center", justifyContent: "center", width: 18, height: 18, borderRadius: 9, borderWidth: 1, borderColor: own ? "#00000030" : t.border, backgroundColor: own ? current : t.editor }}>
          {own ? null : <text style={{ fontSize: ts(11), fontWeight: 600, color: t.muted }}>#</text>}
        </div>
      </div>
      {custom ? (
        <div style={{ marginLeft: 4 }}>
          <TextField
            testId={testId ? `${testId}-hex` : undefined}
            autoFocus={typing}
            value={own ? current : ""}
            placeholder="#rrggbb"
            width={104}
            code
            onCommit={(text) => {
              const hex = parseHex(text)
              if (hex === undefined) return onInvalid?.(`"${text.trim()}" is not a colour: write it as #rrggbb`)
              onInvalid?.(undefined)
              if (hex) setTyping(false)
              onChange(hex)
            }}
          />
        </div>
      ) : null}
    </div>
  )
}

// ── ImageCards ───────────────────────────────────────────────────────

export interface ImageCard {
  value: string
  title: string
  /** An image (a path, a data URL: SVG bytes are decoded), or `preview`. */
  src?: string
  /** What the card shows instead of an image: a terminal scheme's lines. */
  preview?: ReactNode
}

export interface ImageCardsProps {
  cards: ImageCard[]
  value: string | undefined
  onChange(value: string): void
  /** A card's width; its preview is `height` tall. */
  width?: number
  height?: number
  /** One line that scrolls sideways, building only the cards in view (hundreds of them); else they wrap. */
  strip?: boolean
  /** Rows that fill the space they are in and scroll down, the cards widened to fill each row and
   *  only the rows in view built: a page of every choice. `width` is then the narrowest a card is. */
  grid?: boolean
  testId?: string
}

/** Choices as cards with a picture: an app icon, a UI theme, a terminal scheme. */
export function ImageCards({ cards, value, onChange, width = 168, height = 80, strip, grid, testId }: ImageCardsProps) {
  // One function for every render, whatever `onChange` the caller passes, so a card that has
  // not changed is not drawn again.
  const latest = useRef(onChange)
  latest.current = onChange
  const [onPick] = useState(() => (v: string) => latest.current(v))
  const card = (c: ImageCard, w = width, h = height) => <Card key={c.value} card={c} selected={c.value === value} width={w} height={h} onPick={onPick} testId={testId ? `${testId}-${c.value}` : undefined} />
  if (grid) return <CardGrid cards={cards} value={value} width={width} height={height} draw={card} testId={testId} />
  if (strip) return <CardStrip cards={cards} value={value} width={width} draw={(c) => card(c)} testId={testId} />
  return (
    <div testId={testId} style={{ display: "flex", flexDirection: "row", flexWrap: "wrap", gap: CARD_GAP }}>
      {cards.map((c) => card(c))}
    </div>
  )
}

const CARD_GAP = 12
/** Cards drawn either side of those in view, so a fast swipe shows no gap. */
const OVERSCAN = 3

/** Which of `count` cards `stride` apart a strip `width` wide, scrolled `scrolled`, shows. */
export function visibleCards(scrolled: number, width: number, count: number, stride: number): [number, number] {
  const from = Math.min(count, Math.max(0, Math.floor(Math.max(0, scrolled) / stride) - OVERSCAN))
  const to = Math.min(count, Math.ceil((Math.max(0, scrolled) + Math.max(0, width)) / stride) + OVERSCAN)
  return [from, Math.max(from, to)]
}

function CardStrip({ cards, value, width, draw, testId }: { cards: ImageCard[]; value: string | undefined; width: number; draw: (c: ImageCard) => ReactNode; testId?: string }) {
  const stride = width + CARD_GAP
  const [view, setView] = useState({ scrolled: 0, width: 720 })
  const ref = useRef<{ scrollTo?: (x: number, y: number) => void } | null>(null)
  /** The card to bring into view once the strip can scroll, which is after its first layout. */
  const want = useRef(-1)
  const [from, to] = visibleCards(view.scrolled, view.width, cards.length, stride)
  // A swipe moves the offset every frame, but the cards built change only when one comes into
  // view or leaves it: the strip draws again then, not on every frame of the swipe.
  const scrolled = useRef(0)
  scrolled.current = view.scrolled
  const same = (a: number, b: number, w: number) => {
    const [f1, t1] = visibleCards(a, w, cards.length, stride)
    const [f2, t2] = visibleCards(b, w, cards.length, stride)
    return f1 === f2 && t1 === t2
  }
  const onScroll = (e: EventPayload & { scrollX?: number }) => {
    const x = -(e.scrollX ?? 0)
    want.current = -1
    scrolled.current = x
    setView((v) => (same(v.scrolled, x, v.width) ? v : { ...v, scrolled: x }))
  }
  // The picked card is in view when the strip first shows, or its list changes (a search).
  const picked = cards.findIndex((c) => c.value === value)
  const list = cards.map((c) => c.value).join("\n")
  // Only the offset the strip reports is the one its cards are built for: one assumed before
  // the scroll lands built cards out of view, and the strip showed none.
  const bring = (viewport: number) => {
    if (want.current < 0) return
    ref.current?.scrollTo?.(Math.max(0, want.current * stride - (viewport - width) / 2), 0)
  }
  useEffect(() => {
    want.current = picked
    bring(view.width)
  }, [list])
  return (
    <div
      testId={testId}
      ref={ref as any}
      // Layout is reported whenever the strip moves (the page scrolling): only a new width matters.
      onLayout={(e: EventPayload & { width?: number }) => {
        // Laid out, the strip can scroll: the card is brought once, not each time the page moves.
        bring(e.width ?? view.width)
        want.current = -1
        setView((v) => (e.width == null || e.width === v.width ? v : { scrolled: scrolled.current, width: e.width }))
      }}
      onScrollChange={onScroll}
      style={{ display: "flex", flexDirection: "row", width: "100%", overflowX: "scroll", paddingBottom: 6, scrollbarWidth: 4 }}
    >
      <div style={{ width: from * stride, height: 1, flexShrink: 0 }} />
      {cards.slice(from, to).map((c) => (
        <div key={c.value} style={{ flexShrink: 0, marginRight: CARD_GAP }}>
          {draw(c)}
        </div>
      ))}
      <div style={{ width: (cards.length - to) * stride, height: 1, flexShrink: 0 }} />
    </div>
  )
}

/** A card's caption under its preview, and the most its borders take (a picked one's are 2). */
const CARD_CAPTION = 28
const CARD_FRAME = 1 + 2 * 2
/** Room at a grid's right for its scroll bar. */
const GRID_GUTTER = 8

/** How a grid `width` wide lays out cards at least `min` wide: how many go in a row, and how wide each is. */
export function gridColumns(width: number, min: number): { columns: number; width: number } {
  const columns = Math.max(1, Math.floor((width + CARD_GAP) / (min + CARD_GAP)))
  return { columns, width: Math.max(min, Math.floor((width - CARD_GAP * (columns - 1)) / columns)) }
}

/** A grid `viewWidth` wide of cards at least `min` wide whose previews are `height` tall at that width. */
function gridLayout(viewWidth: number, min: number, height: number) {
  const { columns, width } = gridColumns(viewWidth - GRID_GUTTER, min)
  // Each card keeps its preview's shape as it widens to fill its row.
  const tall = Math.round((height * width) / min)
  const rowHeight = tall + CARD_CAPTION + CARD_FRAME
  return { columns, width, tall, rowHeight, stride: rowHeight + CARD_GAP }
}

function CardGrid({ cards, value, width, height, draw, testId }: { cards: ImageCard[]; value: string | undefined; width: number; height: number; draw: (c: ImageCard, w: number, h: number) => ReactNode; testId?: string }) {
  const [view, setView] = useState({ scrolled: 0, width: 720, height: 480 })
  const ref = useRef<{ scrollTo?: (x: number, y: number) => void } | null>(null)
  /** The card to bring into view once the grid can scroll, which is after its first layout. */
  const want = useRef(-1)
  const layout = gridLayout(view.width, width, height)
  const { stride } = layout
  const rows = Math.ceil(cards.length / layout.columns)
  const [from, to] = visibleCards(view.scrolled, view.height, rows, stride)
  // As the strip does: draw again when a row comes into view or leaves it, not each frame.
  const scrolled = useRef(0)
  scrolled.current = view.scrolled
  const onScroll = (e: EventPayload & { scrollY?: number }) => {
    const y = -(e.scrollY ?? 0)
    want.current = -1
    scrolled.current = y
    setView((v) => {
      const [f1, t1] = visibleCards(v.scrolled, v.height, rows, stride)
      const [f2, t2] = visibleCards(y, v.height, rows, stride)
      return f1 === f2 && t1 === t2 ? v : { ...v, scrolled: y }
    })
  }
  // The picked card's row is in view when the grid first shows, or its list changes (a search).
  const picked = cards.findIndex((c) => c.value === value)
  const list = cards.map((c) => c.value).join("\n")
  // Laid out as the grid is `viewWidth` wide: the size its first layout reports, not the one assumed.
  const bring = (viewWidth: number, viewport: number) => {
    if (want.current < 0) return
    const at = gridLayout(viewWidth, width, height)
    ref.current?.scrollTo?.(0, Math.max(0, Math.floor(want.current / at.columns) * at.stride - (viewport - at.rowHeight) / 2))
  }
  useEffect(() => {
    want.current = picked
    if (picked < 0) ref.current?.scrollTo?.(0, 0)
    bring(view.width, view.height)
  }, [list])
  const slice: ImageCard[][] = []
  for (let r = from; r < to; r++) slice.push(cards.slice(r * layout.columns, (r + 1) * layout.columns))
  return (
    <div
      testId={testId}
      ref={ref as any}
      onLayout={(e: EventPayload & { width?: number; height?: number }) => {
        bring(e.width ?? view.width, e.height ?? view.height)
        want.current = -1
        setView((v) => ((e.width == null || e.width === v.width) && (e.height == null || e.height === v.height) ? v : { scrolled: scrolled.current, width: e.width ?? v.width, height: e.height ?? v.height }))
      }}
      onScrollChange={onScroll}
      style={{ display: "flex", flexDirection: "column", flexGrow: 1, minHeight: 0, width: "100%", overflowY: "scroll", paddingRight: GRID_GUTTER, scrollbarWidth: 4 }}
    >
      <div style={{ height: from * stride, width: 1, flexShrink: 0 }} />
      {slice.map((row, i) => (
        <div key={from + i} style={{ display: "flex", flexDirection: "row", alignItems: "flex-start", gap: CARD_GAP, height: layout.rowHeight, flexShrink: 0, marginBottom: CARD_GAP }}>
          {row.map((c) => draw(c, layout.width, layout.tall))}
        </div>
      ))}
      <div style={{ height: Math.max(0, (rows - to) * stride), width: 1, flexShrink: 0 }} />
    </div>
  )
}

/** Memoized, so a strip scrolling builds the cards coming into view and not the ones already in it. */
const Card = memo(function Card({ card, selected, width, height, onPick, testId }: { card: ImageCard; selected: boolean; width: number; height: number; onPick(value: string): void; testId?: string }) {
  const t = useTheme()
  const ts = useTextSize()
  const m = useMetrics()
  const r = m.radius + 2
  return (
    <div
      testId={testId}
      role="radio"
      aria-selected={selected}
      aria-label={card.title}
      title={card.title}
      tabIndex={0}
      onClick={() => onPick(card.value)}
      onKeyDown={(e) => {
        if (!pressed(e)) return
        e.preventDefault()
        onPick(card.value)
      }}
      style={{ display: "flex", flexDirection: "column", width, flexShrink: 0, borderRadius: r, borderWidth: selected ? 2 : 1, borderColor: selected ? t.accent : t.border, overflow: "hidden", cursor: "pointer", keyboardFocusDim: false, hover: selected ? {} : { borderColor: t.muted } }}
    >
      <div style={{ pointerEvents: "none", display: "flex", width: "100%", height, backgroundColor: t.editor, alignItems: "center", justifyContent: "center", overflow: "hidden" }}>
        {card.preview ?? (card.src ? <img src={card.src} objectFit="cover" style={{ width: "100%", height }} /> : <text style={{ fontSize: ts(16), color: t.faint }}>{card.title}</text>)}
      </div>
      <div style={{ pointerEvents: "none", display: "flex", flexDirection: "row", alignItems: "center", gap: 6, height: 28, paddingLeft: 8, paddingRight: 8, borderTopWidth: 1, borderColor: t.border, backgroundColor: t.surface }}>
        <text style={{ flexGrow: 1, minWidth: 0, fontSize: ts(12), color: selected ? t.text : t.muted, whiteSpace: "nowrap", textOverflow: "ellipsis", overflow: "hidden" }}>{card.title}</text>
        {selected ? <Icon name="check" size={12} color={t.accent} /> : null}
      </div>
    </div>
  )
})

/** `children` without the holes conditionals leave (`false`, `null`). */
export function present(children: ReactNode): ReactNode[] {
  // `toArray` already drops `null`, `undefined` and booleans.
  return Children.toArray(children).filter((c) => isValidElement(c) || typeof c === "string" || typeof c === "number")
}
