// Launchpad's block (`pumi.launchpad.new`): a list of what to open in this tab, the same as the `+`
// menu's (the most used first), each with its key, the same letter as there. A press on a row, its
// key, or Enter on the one the arrows chose opens it in the list's place (`replace`, the same tab);
// Escape closes the tab. After the list comes the way to more of it: a last row that opens the
// plugins in Settings. The keys are drawn only while the tab has the focus: out of focus they would
// name what a press cannot pick.
import { Fragment, useMemo, useRef, useState } from "react";
import { IconImage, iconSource, openSettings, useMetrics, useTextSize, useTheme, type RendererProps } from "pumi";
import * as kit from "pumi/theme";
import { call, useSlot, via } from "pumi/theme";
import { byLetter, fromSlot, kinds, type Kind, type NewTab, type SlotAction } from "./kinds.ts";

const swallow = (err: unknown) => console.error(`launchpad: ${String((err as Error)?.message ?? err)}`);

/**
 * A row's icon: one of the app's by name, or the mark a plugin brings for it (an agent's own,
 * `data:image/svg+xml,…`). What is neither is the app's box, as a row that names no icon had.
 */
function RowIcon({ icon, color }: { icon: string; color: string }) {
  const source = useMemo(() => iconSource(icon) ?? iconSource("box"), [icon]);
  return source ? <IconImage icon={source} size={16} color={color} /> : null;
}

/** The `+` menu's list; a pumi before `useNewTabs` has the `tab.new` slot's actions instead. */
const useNewTabs = (kit as { useNewTabs?: () => NewTab[] }).useNewTabs;
const useList: () => Kind[] = useNewTabs
  ? () => {
      const list = useNewTabs();
      return useMemo(() => kinds(list), [list]);
    }
  : () => {
      const { actions } = useSlot("tab.new");
      return useMemo(() => fromSlot(actions as SlotAction[]), [actions]);
    };

/** The plugins in Settings: where the rows this list has not got are. */
const explorePlugins = () => openSettings("store");

export function New({ tab, focused }: RendererProps) {
  const id = tab.id;
  const ts = useTextSize();
  const t = useTheme();
  const m = useMetrics();
  const list = useList();
  // The list, then the one row that is not a tab to open.
  const count = list.length + 1;
  const [chosen, setChosen] = useState(0);
  const opening = useRef(false);
  const pick = (k: Kind | undefined) => {
    if (!k || opening.current) return;
    opening.current = true;
    Promise.resolve(k.open(id, { kind: "tiled", replace: id }))
      .catch(swallow)
      .finally(() => (opening.current = false));
  };
  const chosenRow = (i: number) => i === chosen;
  const rowStyle = (i: number) => ({
    display: "flex",
    flexDirection: "row" as const,
    alignItems: "center",
    gap: 12,
    height: 34,
    flexShrink: 0,
    paddingLeft: 10,
    paddingRight: 8,
    borderRadius: m.radius,
    cursor: "pointer" as const,
    backgroundColor: chosenRow(i) ? t.hover : "transparent",
  });
  return (
    <div
      // Mounted again as the tab takes the focus, so the keys come here and not to the last one.
      key={focused ? "focused" : "unfocused"}
      autoFocus={focused}
      tabIndex={0}
      onMouseDown={() => {
        if (!focused) call(via(id, "tab.focus"), id).catch(swallow);
      }}
      onKeyDown={(e: any) => {
        const mods = e.modifiers ?? {};
        if (mods.cmd || mods.ctrl || mods.alt) return;
        const key = String(e.key ?? "").toLowerCase();
        const last = Math.max(count, 1);
        let handled = true;
        if (key === "up") setChosen((i) => (i + last - 1) % last);
        else if (key === "down") setChosen((i) => (i + 1) % last);
        else if (key === "enter") {
          if (chosen === list.length) explorePlugins();
          else pick(list[chosen]);
        } else if (key === "escape") call(via(id, "tab.close"), id).catch(swallow);
        else {
          const k = byLetter(list, key);
          if (k) pick(k);
          else handled = false;
        }
        if (handled) {
          e.preventDefault?.();
          e.stopPropagation?.();
        }
      }}
      style={{ display: "flex", flexDirection: "column", flexGrow: 1, minWidth: 0, minHeight: 0, alignItems: "center", justifyContent: "center", padding: 24, overflowY: "scroll", keyboardFocusDim: false }}
    >
      <div style={{ display: "flex", flexDirection: "column", width: "100%", maxWidth: 360, gap: 2 }}>
        {list.map((k, i) => (
          <div
            key={k.id}
            role="button"
            aria-label={k.title}
            onClick={() => pick(k)}
            onMouseEnter={() => setChosen(i)}
            style={rowStyle(i)}
          >
            <RowIcon icon={k.icon} color={t.text} />
            <text style={{ fontSize: ts(14), color: t.text, whiteSpace: "nowrap", textOverflow: "ellipsis", flexShrink: 1, minWidth: 0 }}>{k.title}</text>
            <div style={{ flexGrow: 1, minWidth: 16 }} />
            {focused && k.keys ? <text style={{ fontSize: ts(12), color: t.faint, whiteSpace: "nowrap", flexShrink: 0 }}>{k.keys}</text> : null}
            {focused && k.letter ? (
              <div style={{ display: "flex", width: 22, height: 22, flexShrink: 0, alignItems: "center", justifyContent: "center", borderRadius: m.radiusSmall, backgroundColor: t.active }}>
                <text style={{ fontSize: ts(12), color: t.muted }}>{k.letter}</text>
              </div>
            ) : null}
          </div>
        ))}
        <Fragment>
          <div style={{ height: 1, flexShrink: 0, marginTop: 6, marginBottom: 6, marginLeft: 10, marginRight: 10, backgroundColor: t.border }} />
          <div
            role="button"
            aria-label="Explore plugins"
            onClick={explorePlugins}
            onMouseEnter={() => setChosen(list.length)}
            style={rowStyle(list.length)}
          >
            <RowIcon icon="puzzle" color={t.muted} />
            <text style={{ fontSize: ts(14), color: t.muted, whiteSpace: "nowrap" }}>Explore plugins</text>
          </div>
        </Fragment>
      </div>
    </div>
  );
}
