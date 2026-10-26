---
title: Themes
group: Build
order: 5
description: Redraw the whole window: colours, layout, and any part in React.
---

A theme is a plugin with a `themes` entry in its manifest. It sets the window's colours, the terminal palette, metrics and fonts, and it can replace any part of the window, from one button to the whole layout, with a React component.

The fastest start is a theme plugin made from the default theme:

```sh
pumi plugin new mytheme --theme default   # a plugin whose theme.tsx starts from the default's look
pumi plugin dev mytheme             # link it, and reload it as you edit it
```

```text
mytheme/
  package.json          "themes": ["themes/mytheme"] in its "pumi" key
  themes/mytheme/
    theme.tsx           export default defineTheme({ … })
```

## defineTheme

`theme.tsx` exports one `defineTheme`. Everything in it is optional; what you leave out comes from the default theme.

```tsx
import { defineTheme, useTheme } from "pumi";

function View({ index, active }: { view: string; active: boolean; index: number }) {
  const t = useTheme();
  return <text style={{ height: 22, paddingLeft: 10, paddingRight: 10, color: active ? t.accent : t.faint }}>{`${index + 1}`}</text>;
}

export default defineTheme({
  name: "My Theme",
  colors: { dark: { accent: "#ff6b6b" }, light: { accent: "#c92a2a" } },
  terminal: { dark: { background: "#101014" } },
  metrics: { radius: 0, fontSize: 13 },
  fonts: [`${import.meta.dir}/fonts/pixel.ttf`],
  components: { View },
});
```

| Key | |
| --- | --- |
| `colors` | `window` `surface` `editor` `text` `muted` `faint` `border` `hover` `active` `selected` `accent` `accentText` `success` `error` `warning` `overlay` `agent.<status>` |
| `terminal` | `background` `foreground` `cursor` `selection` `ansi`, `palette` (colours 0–255 pinned by index) |
| `metrics` | `radius` `radiusSmall` `fontSize` `paneFade` (how much an unfocused terminal fades, 0–1), `backgroundOpacity` (the default window's backgrounds, 0–1; under 1 they let the desktop through), bar heights |
| `fonts` | font files |
| `window` | `{ background: "opaque" \| "transparent" \| "blurred", blur }` (`blur`, macOS: px of the window server's blur behind a background that is not opaque) |
| `layout` | `gap`, `divider`, `lifted`; or a function `(tokens, metrics, option) => ({ … })`, where `option("pane_style")` reads the theme's own setting, drawn again as it changes |
| `fromPalette` | `(palette, appearance) => tokens`: colours made from the terminals' palette (the picked colour scheme, else the theme's `terminal`), over `colors`, so the window matches whatever scheme is picked; with a scheme picked, the look is the scheme's |
| `docks` | extra sidebars |
| `components` | parts to replace |

Above the sixteen ANSI colours, terminals use xterm's 256-colour cube and grey ramp. With `terminal_palette_generate` on (Settings: "Generate 256 colors from the scheme"), pumi builds them from the sixteen, the background and the foreground instead, as Ghostty's `palette-generate` does, so programs that use 256 colours match the scheme. `terminal_palette_harmonious` keeps a light background's generated colours running from light to dark. Generation never overwrites a colour a theme pins in `palette`.

Pick a theme with `pumi option set ui_theme NAME`, or in `settings.json` (`"options": { "ui_theme": "mytheme" }`). If one throws, the window falls back to the default theme, and the app started with `--safe` (or Settings → Plugins → Safe mode) runs without any plugin.

The default theme has settings of its own, under the theme picker in Settings: `ui_font_size` (the window's text, 10–16pt), `ui_inactive_pane_fade` (0–80%), `ui_pane_gap` (px), `ui_radius` (px), `ui_accent` (`#rrggbb`), `ui_background_opacity` (20–100%, 95 by default; 100 is opaque, lower shows the desktop through the window) and `ui_background_blur` (0–60px, 40 by default; how much what shows through is blurred). A plugin's theme declares its own in its manifest and reads them with `useThemeOption`.

It also has settings for how it behaves, in sections under those: Panes (`ui_pane_titles`, title bars over panes; `ui_pane_outline`, an outline on the focused pane; `ui_pane_equalize`, even splits as tabs open and close; `ui_focus_follows_mouse`), Side bar (`ui_sidebar_swipe`, a swipe left to hide it; `ui_view_icons`, how many tab icons a view shows, 0–3) and Motion and feedback (`animate`, tabs gliding into place; `haptics`, trackpad taps from switches and sliders). They are the default theme's, not the app's: a plugin's theme that wants them declares the same keys, unprefixed, in its manifest (`contributes.options.pane_titles`, `pane_outline`, `pane_equalize`, `focus_follows_mouse`, `sidebar_swipe`, `view_icons`, `animate`, `haptics`), and Settings shows them on its page; one it leaves out keeps its default. The kit reads the drawing theme's own: `usePaneTitles`, `useOutlineFocused`, `useAutoEqualize`, `useFocusOnHover(tab, focused)` (a pane's `onMouseEnter`), `useSidebarSwipe(hide)` (its side bar's `onScroll`), `useViewIcons`, `useAnimate` and `useThemeSetting(key)`, from `pumi/theme`; `toggle_pane_titles` and `toggle_auto_equalize` set the drawing theme's too.

## Parts

Each part of the window is a component. Give one in `components` and it draws that part; the app's own draw the rest, and a part you give draws the ones inside it with `useComponents()`.

`Window` `Starting` `ViewBar` `View` `Layout` `Split` `Pane` `PaneTitle` `PaneTab` `TabBody` `Splitter` `Float` `Panel` `Sessions` `Agent` `AgentControls` `Action` `Menu` `Dialog` `IconButton`

A `Pane` is given its id, its `tabs` and the one in front (`active`). Its `PaneTitle` draws the strip of its tabs, one `PaneTab` each, while panes hold tabs (`pane_tabs`, on by default), and the front tab's title while they don't. `TabBody` draws a tab: only the front tab's is drawn, and the others keep running behind it. A zoomed pane fills its view with its strip still on top.

```tsx
import { useTheme } from "pumi";
import { useComponents } from "pumi/theme";

function Pane({ pane, tabs, active, focused, zoomed }: { pane: string; tabs: string[]; active?: string | null; focused: boolean; zoomed: boolean }) {
  const t = useTheme();
  const C = useComponents();
  const front = active ?? tabs[0];
  return (
    <div style={{ borderWidth: 2, borderColor: t.accent, flexDirection: "column", flexGrow: 1 }}>
      <C.PaneTitle pane={pane} tabs={tabs} active={front} focused={focused} zoomed={zoomed} menu={() => {}} />
      <C.TabBody key={front} tab={front} focused={focused} />
    </div>
  );
}
```

## Docks

A dock is a sidebar that your `Window` places. `docks` declares it; pumi gives it a `toggle_<name>` command, a shortcut, a size the user can drag, and the plugins' panels that match `panels`.

```tsx
import { defineTheme } from "pumi";
import { Dock, renderComponent, useComponents, useDockPanels } from "pumi/theme";

function Surfaces() {
  const { components, shown, show } = useDockPanels("surfaces");
  return shown ? renderComponent(shown) : components.map((c) => <text key={c.id} onClick={() => show(c)}>{c.title}</text>);
}

function Window() {
  const C = useComponents();
  return (
    <div style={{ display: "flex", flexDirection: "row", width: "100%", height: "100%" }}>
      <Dock name="sidebar" edge="left"><C.Sessions /></Dock>
      <div style={{ flexGrow: 1 }}><C.Layout /></div>
      <Dock name="surfaces" edge="right"><Surfaces /></Dock>
    </div>
  );
}

export default defineTheme({
  name: "Two Sides",
  docks: { surfaces: { title: "Surfaces", keys: "cmd+alt+b", size: 320, panels: ["*"] } },
  components: { Window },
});
```

## Styling

Styles are objects, not CSS: flexbox, plus gradients, nine-slice borders, sprite sheets and motion. Take colours from `useTheme()` so they follow light and dark.

```tsx
<div style={{ background: { type: "linear-gradient", angle: 90, stops: [{ color: t.accent }, { color: t.surface }] } }} />
<div style={{ backgroundImage: floor, backgroundSize: 32, imageRendering: "pixelated" }} />
<div style={{ borderImage: { src: frame, slice: 8 } }} />
<img src={walk} sprite={{ columns: 4, fps: 8 }} />
<motion.div animate={{ opacity: [1, 0.4, 1] }} transition={{ duration: 1.2, repeat: Infinity }} />
<motion.svg src={spinner} animate={{ rotate: [0, 360] }} transition={{ duration: 1, repeat: Infinity, ease: "linear" }} />  {/* rotate: svg only, in degrees */}
<div style={{ height: 22, appRegion: "drag" }} />      {/* a title bar */}
```

Files beside the theme, images and fonts, are paths from `` `${import.meta.dir}/…` ``.
