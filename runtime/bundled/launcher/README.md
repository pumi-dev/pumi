# pumi/launcher

Bundled with pumi. It declares the `pumi.launcher.new` block: the tab a pane split from the pane
bar's split button starts with. The default theme draws it as a list of what can be opened there
(Terminal, File…, and what the enabled plugins add to the `tab.new` slot), each with a key to press;
what is picked replaces the tab (`tab.open(…, { kind: "tiled", replace })`), in the same pane.

It has no code of its own. Turn it off (Settings ▸ Plugins, or
`pumi plugin update pumi/launcher --no-enabled`) and a split opens a terminal.
