# pumi/terminal

Bundled with pumi. It draws `terminal` blocks: pumid runs every terminal, and this plugin's
`Terminal` renderer puts its screen in the tab with `<terminal tab autoFocus>`.

It is listed with your plugins and can be turned off, in Settings ▸ Plugins or with
`pumi plugin update pumi/terminal --no-enabled`, not removed. To draw terminals yourself, give a
plugin `"renderers": { "terminal": "MyTerminal" }` and either turn this one off, set the
`renderers` option to `{ "terminal": "you/your-plugin" }`, or set a tab's `@tab_renderer` to
your plugin's id for that tab alone.
