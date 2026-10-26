(() => {
  const init = __PUMI_INIT__;
  if (location.protocol !== init.at[0] || location.host !== init.at[1] || !window.ipc) return;
  let next = 1;
  const waiting = new Map();
  const callbacks = new Map();
  const encode = (value) =>
    JSON.stringify(value === undefined ? null : value, (_, v) => {
      if (typeof v !== "function") return v;
      const ref = next++;
      callbacks.set(ref, v);
      return { ref };
    });
  const request = (op, body) =>
    new Promise((resolve, reject) => {
      const id = next++;
      waiting.set(id, { resolve, reject });
      window.ipc.postMessage(`{"t":"pumi","k":${JSON.stringify(init.key)},"id":${id},"op":${JSON.stringify(op)},"body":${encode(body)}}`);
    });
  let theme = init.theme || {};
  const themeListeners = new Set();
  // The app's menu for the page: `pumi.menu(items, at?)` with `{ title, icon?, detail?, checked?,
  // disabled?, run? }` or `{ separator: true }`, drawn by the app over the page at `at` (page px;
  // the last press by default). A right-click the page doesn't take itself (`preventDefault`)
  // opens the edit items it would have had. Resolves when the menu closes.
  let lastPointer = { x: 0, y: 0 };
  addEventListener("pointerdown", (e) => { lastPointer = { x: e.clientX, y: e.clientY }; }, true);
  const menu = (items, at) => {
    const p = at || lastPointer;
    return request("menu", { items: items || [], x: p.x, y: p.y });
  };
  const editable = (el) => !!el && (el.isContentEditable || (/^(INPUT|TEXTAREA)$/.test(el.tagName) && !el.readOnly && !el.disabled));
  const edit = (what) => () => request("edit", what);
  addEventListener("contextmenu", (e) => {
    if (!e.isTrusted || e.defaultPrevented) return;
    e.preventDefault();
    const selected = String(getSelection() || "").length > 0;
    const items = editable(e.target)
      ? [{ title: "Cut", run: edit("cut"), disabled: !selected }, { title: "Copy", run: edit("copy"), disabled: !selected },
         { title: "Paste", run: edit("paste") }, { separator: true }, { title: "Select All", run: edit("selectAll") }]
      : selected ? [{ title: "Copy", run: edit("copy") }] : [];
    if (items.length) menu(items, { x: e.clientX, y: e.clientY });
  });
  // The app's drags over the page, as DOM drag events on what is under the pointer: their
  // `dataTransfer` has `application/x-pumi+json` (what the app carries), `text/plain` and, for a
  // path, `text/uri-list`. A page takes one as it takes any drag: `preventDefault` on `dragover`.
  let dragOver = null, dragTaken = null;
  const transfer = (data) => {
    const dt = new DataTransfer();
    try { dt.setData("application/x-pumi+json", JSON.stringify(data)); } catch { /* not JSON: the other types carry it */ }
    const path = data && typeof data === "object" ? data.path || data.file || null : null;
    const text = typeof data === "string" ? data : path || (data && (data.text || data.title)) || "";
    if (text) dt.setData("text/plain", String(text));
    if (typeof path === "string") dt.setData("text/uri-list", "file://" + path.split("/").map(encodeURIComponent).join("/"));
    return dt;
  };
  const fire = (type, el, x, y, dt) => {
    if (!el) return false;
    const init = { bubbles: true, cancelable: true, composed: true, clientX: x, clientY: y };
    let ev = null;
    try { ev = new DragEvent(type, Object.assign({ dataTransfer: dt }, init)); } catch { ev = null; }
    if (!ev || ev.dataTransfer !== dt) {
      ev = new MouseEvent(type, init);
      Object.defineProperty(ev, "dataTransfer", { value: dt });
    }
    return !el.dispatchEvent(ev);
  };
  const taken = (accepted) => {
    if (accepted === dragTaken) return;
    dragTaken = accepted;
    request("dragAccept", !!accepted).catch(() => {});
  };
  const drag = (kind, x, y, data) => {
    const el = kind === "leave" ? null : document.elementFromPoint(x, y);
    const dt = transfer(data);
    if (dragOver && dragOver !== el) fire("dragleave", dragOver, x, y, dt);
    if (el && dragOver !== el) fire("dragenter", el, x, y, dt);
    dragOver = el;
    if (kind === "leave") return taken(null);
    const accepted = fire("dragover", el, x, y, dt);
    if (kind === "drop") {
      if (accepted) fire("drop", el, x, y, dt);
      dragOver = null;
      return taken(null);
    }
    taken(accepted);
  };
  const setThemeCss = (css) => {
    let el = document.getElementById("pumi-theme");
    if (!el) {
      el = document.createElement("style");
      el.id = "pumi-theme";
      (document.head || document.documentElement).prepend(el);
    }
    el.textContent = css;
  };
  if (init.theme_css) {
    if (document.documentElement) setThemeCss(init.theme_css);
    else document.addEventListener("DOMContentLoaded", () => setThemeCss(init.theme_css), { once: true });
  }
  const pumi = {
    plugin: init.plugin,
    get theme() { return theme; },
    onTheme(callback) {
      themeListeners.add(callback);
      return () => themeListeners.delete(callback);
    },
    component: init.component,
    tab: init.tab,
    args: init.args,
    call: (name, ...args) => request("call", { name, args }),
    on(event, opts, callback) {
      if (typeof opts === "function") [callback, opts] = [opts, {}];
      return pumi.call("event.on", event, Object.assign({}, opts, { callback }));
    },
    off: (id) => pumi.call("event.off", id),
    option: (name, opts = {}) => pumi.call("option.get", name, opts),
    setOption: (name, value, opts = {}) => pumi.call("option.set", name, value, opts),
    menu,
  };
  Object.defineProperty(window, "__pumi", {
    value: Object.freeze({
      reply(id, ok, value) {
        const w = waiting.get(id);
        if (!w) return;
        waiting.delete(id);
        if (ok) w.resolve(value);
        else w.reject(Object.assign(new Error(value && value.message), value || {}));
      },
      callback(ref, args) {
        const f = callbacks.get(ref);
        if (!f) return;
        try {
          f(...(Array.isArray(args) ? args : [args]));
        } catch (e) {
          console.error(e);
        }
      },
      theme(css, tokens) {
        setThemeCss(css);
        theme = tokens;
        for (const f of themeListeners) {
          try { f(tokens); } catch (e) { console.error(e); }
        }
      },
      drag,
    }),
  });
  Object.defineProperty(window, "pumi", { value: Object.freeze(pumi) });
})();
