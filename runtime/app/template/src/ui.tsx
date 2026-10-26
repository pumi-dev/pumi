// What the plugin draws, in the window's React tree. Hooks read the server's state; logic is in
// main.ts, whose exports are async calls to the worker.
import { useState } from "react";
import { ChoiceGrid, OptionRow, Row, Section, useOption, usePlugin, useTheme } from "pumi";
import { click } from "./main.ts";
import { qr } from "./qr.ts";

/** The `panel` slot's component (package.json's `contributes.slots`): a section of the sidebar. */
export function Panel() {
  const theme = useTheme();
  const [clicks, setClicks] = useState(0);
  return (
    <div style={{ display: "flex", flexDirection: "column", gap: 8, padding: 8 }}>
      <text style={{ color: theme.muted }}>{`${clicks} clicks`}</text>
      <div
        style={{ paddingLeft: 8, paddingRight: 8, borderRadius: 6, backgroundColor: theme.surface, cursor: "pointer", hover: { backgroundColor: theme.hover } }}
        onClick={async () => setClicks(await click())}
      >
        <text>Click</text>
      </div>
    </div>
  );
}

const POSITIONS = ["top-left", "top", "top-right", "bottom-left", "bottom", "bottom-right"];

/** A QR code as a grid of `div`s (or `<img src={qrSvg(text)}>`): no package needed. */
function QrCode({ text, size = 140 }: { text: string; size?: number }) {
  const modules = qr(text);
  const cell = Math.floor(size / (modules.length + 4));
  return (
    <div style={{ display: "flex", flexDirection: "column", padding: cell * 2, backgroundColor: "#ffffff", borderRadius: 6, alignSelf: "flex-start" }}>
      {modules.map((row, y) => (
        <div key={y} style={{ display: "flex", flexDirection: "row" }}>
          {row.map((dark, x) => (
            <div key={x} style={{ width: cell, height: cell, backgroundColor: dark ? "#000000" : "#ffffff" }} />
          ))}
        </div>
      ))}
    </div>
  );
}

/**
 * Its section in Settings → General (package.json's `contributes.settings`), drawn with the
 * settings kit: a position picked on a grid, a link as a QR code, and an option's own row.
 */
export function Settings() {
  const prefix = `${usePlugin()!.replace("/", ".").replaceAll("-", "_")}.`;
  const [position, setPosition] = useOption<string>(`${prefix}position`);
  const [url] = useOption<string>(`${prefix}url`);
  return (
    <Section id={`${prefix}example`} title="{{name}}" description="An example section: change it in src/ui.tsx.">
      <Row
        title="Position"
        description="Where its notice shows."
        link={`${prefix}position`}
        control={
          <ChoiceGrid
            columns={3}
            choices={POSITIONS.map((value) => ({ value, label: value }))}
            value={position ?? "bottom-right"}
            onChange={(value) => setPosition(value)}
          />
        }
      />
      <OptionRow name={`${prefix}url`} />
      <Row title="Scan it" description="Open the link on your phone.">
        <QrCode text={url || "https://pumi.dev/"} />
      </Row>
    </Section>
  );
}
