// A plugin's side of the host: one per worker, or the whole of an isolate's process. It installs
// the `pumi` SDK's connection, resolves `import "pumi"` to the SDK this app ships, imports the
// plugin's `main`, and hands the SDK what the host delivers.
//
// host → plugin: init {pkg, prefix, dir, main, paths, api, task} · reply {id, ok | error} ·
//   callback {fn, args, task} · event {subscription, event, task} · serve {id, name, args, caller, task} (`export:<name>` calls an
//   export of its main.ts, for its ui.tsx)
// plugin → host: ready · failed {error} · done {task} · call {id, name, args} ·
//   provide {id, name, spec} · withdraw {names} · reply {id, ok | error} · log {stream, line} ·
//   ui {topic, data} (for its own `ui.tsx`: `useChannel`)
import * as sdk from "./sdk/index.ts";
import { connection, deliver, type Init, type Port } from "./sdk/runtime.ts";
import { realpathSync } from "node:fs";
import { resolve } from "node:path";
import { samePath } from "../native/platform.ts";

/** `path` as Bun names a module: a linked plugin's files by where they really are, spelled as the
 *  window side spells them (`app/ui.ts` `real`) so that the two compare equal. */
const real = (path: string) => {
  try {
    return samePath(realpathSync(path));
  } catch {
    return samePath(path);
  }
};

/** Where the plugin's messages come from and go: the worker's port, or the isolate's IPC. */
export interface Channel extends Port {
  listen(on: (message: any) => void): void;
}

const line = (args: any[]) => args.map((a) => (typeof a === "string" ? a : Bun.inspect(a))).join(" ");

let resolving = false;
/**
 * `import … from "pumi"` is the SDK above, whatever the plugin has installed (its own
 * `@pumid/types`, or an npm package of that name): a virtual module wins over both.
 */
function resolvePumi() {
  if (resolving) return;
  resolving = true;
  Bun.plugin({
    name: "pumi",
    setup(build) {
      build.module("pumi", () => ({ exports: { ...sdk }, loader: "object" }));
          },
  });
}

export function run(channel: Channel) {
  const log = (stream: string, text: string) => channel.post({ t: "log", stream, line: text });
  // A rejection or throw nobody handles is the plugin's to hear about, not the host's end: Bun
  // takes the whole process down on a worker's unhandled rejection.
  process.on("unhandledRejection", (err: any) => {
    log("stderr", `unhandled rejection: ${err?.stack ?? err}`);
  });
  process.on("uncaughtException", (err: any) => {
    log("stderr", `uncaught exception: ${err?.stack ?? err}`);
  });
  for (const name of ["log", "info", "debug"] as const) console[name] = (...a: any[]) => log("stdout", line(a));
  for (const name of ["warn", "error", "trace"] as const) console[name] = (...a: any[]) => log("stderr", line(a));

  let started = false;
  channel.listen((m) => {
    try {
      if (m.t === "init") return init(m);
      if (!started) return;
      switch (m.t) {
        case "reply":
          return deliver.reply(m);
        case "callback":
          return deliver.callback(m.fn, m.args);
        case "event":
          return deliver.event(m.subscription, m.event);
        case "serve":
          return deliver.serve(m);
      }
    } catch (err: any) {
      log("stderr", `${m.t}: ${err?.stack ?? err}`);
    } finally {
      if (m.task !== undefined && m.t !== "init") channel.post({ t: "done", task: m.task });
    }
  });

  function init(m: Init & { main?: string | null; api?: number }) {
    // A linked plugin lives where it really is (`opt/<name>` is a link to its directory), and
    // Bun resolves a relative import against the path it was handed: `../../lib/x` from the
    // link reaches the store's sibling, not the checkout's. The window side does the same
    // (app/ui.ts `real`), which is also how it knows a `main.ts` import by its path.
    const file = m.main ? real(resolve(m.dir, m.main)) : undefined;
    connection.install(channel, { pkg: m.pkg, prefix: m.prefix, dir: m.dir, main: m.main ?? undefined, paths: m.paths });
    started = true;
    resolvePumi();
    // A plugin of themes alone has no `main`: it is ready, and its themes load when they mount.
    if (!file) return channel.post({ t: "ready" });
    import(file).then(
      (module: Record<string, unknown>) => {
        // Its exports are what its `ui.tsx` calls (an import of `main.ts` there is a proxy).
        for (const [name, value] of Object.entries(module)) {
          if (typeof value === "function") connection.provided.set(`export:${name}`, value as (...a: any[]) => any);
        }
        channel.post({ t: "ready" });
      },
      (err) => channel.post({ t: "failed", error: err?.stack ?? String(err) }),
    );
  }
}

// In a worker, this file is its whole program; with `--isolate`, a process's, spoken to over IPC.
declare const self: { onmessage: ((e: MessageEvent) => void) | null };
if (Bun.isMainThread && process.argv.includes("--isolate")) {
  run({ post: (m) => process.send!(m), listen: (on) => process.on("message", on) });
  process.on("disconnect", () => process.exit(0));
} else if (!Bun.isMainThread) {
  run({
    post: (m) => postMessage(m),
    listen: (on) => {
      self.onmessage = (e: MessageEvent) => on(e.data);
    },
  });
}
