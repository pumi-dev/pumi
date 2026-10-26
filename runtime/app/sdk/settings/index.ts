// The settings kit, exported from "pumi": what Settings' own pages are drawn with, and what a
// plugin's page or section uses to look native.
export { SettingsContext, useSettings, type SettingsKit } from "./context.ts"
export {
  Button,
  ChoiceGrid,
  COLOR_SWATCHES,
  ColorPicker,
  gridColumns,
  ImageCards,
  KeyChip,
  KeyRecorder,
  MAX_SEQUENCE,
  parseHex,
  RECORD_PAUSE,
  SelectField,
  Slider,
  Spinner,
  Stepper,
  Switch,
  TextField,
  useTextSize,
  visibleCards,
  type ButtonProps,
  type Choice,
  type ChoiceGridProps,
  type ColorPickerProps,
  type ColorSwatch,
  type ImageCard,
  type ImageCardsProps,
  type KeyRecorderProps,
  type SelectFieldProps,
  type SelectOption,
  type SliderProps,
  type StepperProps,
  type SwitchProps,
  type TextFieldProps,
} from "./controls.tsx"
export { Note, Page, Row, Section, StatusDot, useReveal, type PageProps, type RowProps, type SectionProps } from "./layout.tsx"
export { OptionControl, OptionRow, type OptionRowProps } from "./option-row.tsx"
export { choicesOf, formatNumber, groupOf, parse, settable, snap, textOf, titleOf, type Decl, type OptionValue } from "./options.ts"
