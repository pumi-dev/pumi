/** Headless shadcn-shaped Select components rendered with GPUIX host elements. */

import React, {
  createContext,
  forwardRef,
  useCallback,
  useContext,
  useLayoutEffect,
  useMemo,
  useRef,
  useState,
} from "react"
import type { ReactElement, ReactNode } from "react"
import type { EventPayload } from "../../native/addon"
import type { KeyEvent, PressEvent, Props, PublicInstance } from "../types/host.js"
import { useGpuix } from "../hooks/use-gpuix.js"
import {
  FloatingLayer,
  floatingRootStyle,
  renderSlot,
  resolveStyle,
  setRefs,
  useControllableState,
  DismissableLayer,
} from "./floating.js"
import type { FloatingContentProps, StateStyle } from "./floating.js"

export interface SelectItemData {
  value: string
  label?: ReactNode
  textValue?: string
}

interface SelectItemRecord {
  value: string
  disabled: boolean
}

interface SelectContextValue {
  open: boolean
  value: string | undefined
  disabled: boolean
  labels: Map<string, ReactNode>
  activeValue: string | null
  triggerPressedWhileOpen: React.MutableRefObject<boolean>
  dismissedByOutsidePress: React.MutableRefObject<boolean>
  triggerRef: React.MutableRefObject<PublicInstance | null>
  /** `restoreFocus: false` when the user pressed elsewhere, so the press keeps
   *  the focus it just gave. */
  setOpen: (open: boolean, restoreFocus?: boolean) => void
  setActiveValue: (value: string | null) => void
  moveActive: (delta: number) => void
  selectValue: (value: string) => void
  registerItem: (item: SelectItemRecord & { mounted: boolean }) => void
}

const SelectContext = createContext<SelectContextValue | null>(null)

function useSelectContext(name: string): SelectContextValue {
  const context = useContext(SelectContext)
  if (!context) throw new Error(`${name} must be used inside Select`)
  return context
}

export interface SelectProps extends Omit<Props, "children" | "onChange"> {
  children?: ReactNode
  items?: readonly SelectItemData[]
  value?: string
  defaultValue?: string
  onValueChange?: (value: string) => void
  open?: boolean
  defaultOpen?: boolean
  onOpenChange?: (open: boolean) => void
  disabled?: boolean
}

export function Select({
  children,
  items: itemsProp,
  value: valueProp,
  defaultValue,
  onValueChange,
  open: openProp,
  defaultOpen = false,
  onOpenChange,
  disabled = false,
  style,
  ...props
}: SelectProps): ReactElement {
  const { renderer } = useGpuix()
  const [value, setValue] = useControllableState<string | undefined>({
    value: valueProp,
    defaultValue,
    onChange: (nextValue) => {
      if (nextValue !== undefined) onValueChange?.(nextValue)
    },
  })
  const [open, setOpenState] = useControllableState({
    value: openProp,
    defaultValue: defaultOpen,
    onChange: onOpenChange,
  })
  const [activeValue, setActiveValue] = useState<string | null>(null)
  const [itemsVersion, setItemsVersion] = useState(0)
  const triggerPressedWhileOpen = useRef(false)
  const dismissedByOutsidePress = useRef(false)
  const triggerRef = useRef<PublicInstance | null>(null)
  const registeredItems = useRef<SelectItemRecord[]>([])
  const labels = useMemo(() => {
    const next = new Map<string, ReactNode>()
    for (const item of itemsProp ?? []) {
      next.set(item.value, item.label ?? item.textValue ?? item.value)
    }
    return next
  }, [itemsProp])

  const setOpen = (nextOpen: boolean, restoreFocus = true) => {
    setOpenState(nextOpen)
    if (!nextOpen && restoreFocus && triggerRef.current) {
      renderer?.focusElement?.(triggerRef.current.id)
    }
  }

  const registerItem = useCallback(
    ({ value: itemValue, disabled: itemDisabled, mounted }: SelectItemRecord & { mounted: boolean }) => {
      const existing = registeredItems.current.findIndex((item) => item.value === itemValue)
      if (!mounted) {
        if (existing < 0) return
        registeredItems.current = registeredItems.current.filter((item) => item.value !== itemValue)
        setItemsVersion((version) => version + 1)
        return
      }
      if (existing >= 0) {
        registeredItems.current[existing] = { value: itemValue, disabled: itemDisabled }
        return
      }
      registeredItems.current = [
        ...registeredItems.current,
        { value: itemValue, disabled: itemDisabled },
      ]
      setItemsVersion((version) => version + 1)
    },
    []
  )

  useLayoutEffect(() => {
    if (!open) return
    const selected = registeredItems.current.find(
      (item) => item.value === value && !item.disabled
    )
    setActiveValue(selected?.value ?? null)
  }, [open, value, itemsVersion])

  const moveActive = (delta: number) => {
    if (disabled) return
    const enabled = registeredItems.current.filter((item) => !item.disabled)
    if (enabled.length === 0) return
    const currentIndex = enabled.findIndex((item) => item.value === activeValue)
    const start = currentIndex < 0 ? (delta > 0 ? -1 : 0) : currentIndex
    const nextIndex = (start + delta + enabled.length) % enabled.length
    setActiveValue(enabled[nextIndex].value)
  }

  const selectValue = (nextValue: string) => {
    if (disabled) return
    const item = registeredItems.current.find((candidate) => candidate.value === nextValue)
    if (!item || item.disabled) return
    setValue(nextValue)
    setOpen(false)
  }

  const context = useMemo<SelectContextValue>(
    () => ({
      open,
      value,
      disabled,
      labels,
      activeValue,
      triggerPressedWhileOpen,
      dismissedByOutsidePress,
      triggerRef,
      setOpen,
      setActiveValue,
      moveActive,
      selectValue,
      registerItem,
    }),
    [open, value, disabled, labels, activeValue, registerItem]
  )

  return (
    <SelectContext.Provider value={context}>
      <div {...props} style={floatingRootStyle(style)}>{children}</div>
    </SelectContext.Provider>
  )
}

export interface SelectTriggerState {
  open: boolean
  disabled: boolean
  placeholder: boolean
}

export interface SelectTriggerProps extends Omit<Props, "style"> {
  asChild?: boolean
  disabled?: boolean
  style?: StateStyle<SelectTriggerState>
}

export const SelectTrigger = forwardRef<PublicInstance, SelectTriggerProps>(
  function SelectTrigger(
    { asChild, disabled: disabledProp, style, children, onMouseDown, onClick, onKeyDown, ...props },
    forwardedRef
  ) {
    const context = useSelectContext("SelectTrigger")
    const disabled = disabledProp ?? context.disabled
    const state = {
      open: context.open,
      disabled,
      placeholder: context.value === undefined,
    }
    const ref = (value: PublicInstance | null) => {
      context.triggerRef.current = value
      setRefs(value, forwardedRef)
    }
    const triggerProps: Props = {
      ...props,
      tabIndex: disabled ? -1 : props.tabIndex,
      style: resolveStyle(style, state),
      onMouseDown: (event) => {
        onMouseDown?.(event)
        context.triggerPressedWhileOpen.current = context.open
      },
      onClick: (event) => {
        onClick?.(event)
        if (disabled) return
        if (context.dismissedByOutsidePress.current) {
          context.dismissedByOutsidePress.current = false
          return
        }
        if (context.triggerPressedWhileOpen.current) {
          context.triggerPressedWhileOpen.current = false
          context.setOpen(false)
          return
        }
        context.setOpen(!context.open)
      },
      onKeyDown: (event: KeyEvent) => {
        onKeyDown?.(event)
        if (disabled) return
        if (event.key === "down" || (event.key === "n" && event.modifiers?.ctrl)) {
          if (!context.open) context.setOpen(true)
          else context.moveActive(1)
        } else if (event.key === "up" || (event.key === "p" && event.modifiers?.ctrl)) {
          if (!context.open) context.setOpen(true)
          else context.moveActive(-1)
        } else if (event.key === "enter" || event.key === "space") {
          context.setOpen(!context.open)
        }
      },
    }
    return renderSlot({ asChild, children, props: triggerProps, ref, defaultTabIndex: 0 })
  }
)

export interface SelectValueProps extends Props {
  placeholder?: ReactNode
}

export const SelectValue = forwardRef<PublicInstance, SelectValueProps>(
  function SelectValue({ placeholder, children, ...props }, ref) {
    const context = useSelectContext("SelectValue")
    const label = context.value === undefined ? undefined : context.labels.get(context.value)
    return <div {...props} ref={ref}>{children ?? label ?? context.value ?? placeholder}</div>
  }
)

export interface SelectContentProps extends FloatingContentProps {
  onEscapeKeyDown?: (event: KeyEvent) => void
}

export const SelectContent = forwardRef<PublicInstance, SelectContentProps>(
  function SelectContent(
    { children, onMouseDownOutside, onKeyDown, onEscapeKeyDown, tabIndex = -1, style, ...props },
    forwardedRef
  ) {
    const context = useSelectContext("SelectContent")
    if (!context.open) return null
    return (
      <DismissableLayer
        onEscapeKeyDown={(event) => {
          onEscapeKeyDown?.(event)
          context.setOpen(false)
        }}
      >
        <FloatingLayer
          {...props}
          ref={forwardedRef}
          // Focused only to receive keys; the highlighted item shows where
          // the user is, so the popup gets no focus look of its own.
          style={{ keyboardFocusDim: false, ...style }}
          tabIndex={tabIndex}
          autoFocus
          onMouseDownOutside={(event) => {
            onMouseDownOutside?.(event)
            context.dismissedByOutsidePress.current = true
            queueMicrotask(() => {
              context.dismissedByOutsidePress.current = false
            })
            context.setOpen(false, false)
          }}
          onKeyDown={(event: KeyEvent) => {
            onKeyDown?.(event)
            // The popup is modal, like Base UI: Tab stays inside until it closes.
            if (event.key === "tab") {
              event.preventDefault()
              return
            }
            if (context.disabled) return
            if (event.key === "down" || (event.key === "n" && event.modifiers?.ctrl)) {
              context.moveActive(1)
            } else if (event.key === "up" || (event.key === "p" && event.modifiers?.ctrl)) {
              context.moveActive(-1)
            } else if ((event.key === "enter" || event.key === "space") && context.activeValue) {
              context.selectValue(context.activeValue)
            }
          }}
        >
          {children}
        </FloatingLayer>
      </DismissableLayer>
    )
  }
)

export interface SelectItemState {
  selected: boolean
  highlighted: boolean
  disabled: boolean
}

export interface SelectItemProps extends Omit<Props, "children" | "style"> {
  value: string
  disabled?: boolean
  asChild?: boolean
  children?: ReactNode | ((state: SelectItemState) => ReactNode)
  style?: StateStyle<SelectItemState>
}

export const SelectItem = forwardRef<PublicInstance, SelectItemProps>(
  function SelectItem(
    { value, disabled = false, asChild, children, style, onClick, onMouseEnter, ...props },
    ref
  ) {
    const context = useSelectContext("SelectItem")
    const state = {
      selected: context.value === value,
      highlighted: context.activeValue === value,
      disabled,
    }
    useLayoutEffect(() => {
      context.registerItem({ value, disabled, mounted: true })
      return () => context.registerItem({ value, disabled, mounted: false })
    }, [context.registerItem, value, disabled])
    const onItemMouseEnter = (event: EventPayload) => {
      onMouseEnter?.(event)
      if (!disabled && !context.disabled) context.setActiveValue(value)
    }
    const onItemClick = (event: PressEvent) => {
      onClick?.(event)
      if (!disabled && !context.disabled) context.selectValue(value)
    }
    return renderSlot({
      asChild,
      children: typeof children === "function" ? children(state) : children,
      props: {
        ...props,
        style: resolveStyle(style, state),
        onMouseEnter: onItemMouseEnter,
        onClick: onItemClick,
      },
      ref,
    })
  }
)

export const SelectGroup = forwardRef<PublicInstance, Props>(function SelectGroup(props, ref) {
  return <div {...props} ref={ref} />
})

export const SelectLabel = forwardRef<PublicInstance, Props>(function SelectLabel(props, ref) {
  return <div {...props} ref={ref} />
})

export const SelectSeparator = forwardRef<PublicInstance, Props>(
  function SelectSeparator(props, ref) {
    return <div {...props} ref={ref} />
  }
)

export const SelectScrollUpButton = forwardRef<PublicInstance, Props>(
  function SelectScrollUpButton(props, ref) {
    return <div {...props} ref={ref} />
  }
)

export const SelectScrollDownButton = forwardRef<PublicInstance, Props>(
  function SelectScrollDownButton(props, ref) {
    return <div {...props} ref={ref} />
  }
)

export {
  Select as Root,
  SelectContent as Content,
  SelectGroup as Group,
  SelectItem as Item,
  SelectLabel as Label,
  SelectScrollDownButton as ScrollDownButton,
  SelectScrollUpButton as ScrollUpButton,
  SelectSeparator as Separator,
  SelectTrigger as Trigger,
  SelectValue as Value,
}
