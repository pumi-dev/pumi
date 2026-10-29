# pumi/launcher

Bundled with pumi. It declares the `pumi.launcher.new` block: the tab a pane split from the pane
bar's split button starts with, and what any new tab holds while the `new_tab` option names
nothing. The default theme draws it as a list of what can be opened there (File…, and what the
enabled plugins add to the `tab.new` slot, the Terminal plugin's shell among them), each with a key
to press;
what is picked replaces the tab (`tab.open(…, { kind: "tiled", replace })`), in the same pane.

It has no code of its own. Turn it off (Settings ▸ Plugins, or
`pumi plugin update pumi/launcher --no-enabled`) and a split opens what the `new_tab` option
names (the Terminal plugin sets it to a shell).
