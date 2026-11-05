// Generated from api/pumi-api.json by `bun runtime/app/gen/index.ts` (`cargo xtask api` runs it); do not edit.

/** Every function, by name. */
export const FUNCTIONS = ["call","clipboard.get","clipboard.set","event.emit","event.off","event.on","event.wait","fn.set","fs.list","fs.open","fs.pick","fs.trash","menu.set","option.get","option.set","pane.close","pane.focus","pane.get","pane.list","pane.update","plugin.add","plugin.contribute","plugin.remove","plugin.update","process.list","project.close","project.focus","project.get","project.list","registry.list","request.close","request.get","request.list","request.open","tab.call","tab.close","tab.focus","tab.get","tab.list","tab.open","tab.update","view.close","view.focus","view.get","view.update"] as const;

/** The events the core and the host fire. */
export const EVENTS = ["tab.opened","tab.closed","tab.changed","tab.alerted","request.opened","request.closed","view.opened","view.closed","view.changed","pane.opened","pane.closed","pane.changed","project.opened","project.closed","project.changed","option.changed","app.changed","registry.changed"] as const;

/** The methods every tab answers through `tab.call`. */
export const TAB_METHODS = ["read","input","run","wait","selection","report","attach"] as const;

/** Each model, by its kind: its id's prefix, its fields, which of them can be watched, its events. */
export const MODELS = {
  project: {"name":"Project","prefix":"$","fields":["id","path","views","activeView"],"watch":["views","activeView"],"events":["project.opened","project.closed","project.changed"]},
  view: {"name":"View","prefix":"@","fields":["id","project","name","index","layout","activeTab","activePane","zoom"],"watch":["project","name","index","layout","activeTab","activePane","zoom"],"events":["view.opened","view.closed","view.changed"]},
  pane: {"name":"Pane","prefix":":","fields":["id","view","tabs","activeTab","isFocused","isZoomed"],"watch":["tabs","activeTab"],"events":["pane.opened","pane.closed","pane.changed"]},
  tab: {"name":"Tab","prefix":"%","fields":["id","project","view","block","traits","place","title","cwd","foregroundCwd","process","scroll","neighbors","isFocused","isZoomed","options"],"watch":["project","view","block","traits","place","title","cwd","foregroundCwd","process","scroll","neighbors","isFocused","isZoomed","options"],"events":["tab.opened","tab.closed","tab.changed","tab.alerted"]},
  request: {"name":"Request","prefix":"+","fields":["id","tab","kind","title","detail","options","tool","level","notification","source","created","expires"],"watch":[],"events":["request.opened","request.closed"]},
  plugin: {"name":"Plugin","prefix":"","fields":["id","version","source","path","enabled"],"watch":["version","source","path","enabled"],"events":["registry.changed"]},
} as const;
