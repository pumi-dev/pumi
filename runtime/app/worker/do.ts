// `pumi do FILE`: one script, now, against the server at `$PUMI`, with its answer. The `pumi`
// module is the worker's SDK, as a plugin's main.ts has it, over the server's socket instead of
// the app: calls go to the server as JSON-RPC, and events and callbacks come back over it.
//
//   bun runtime/app/worker/do.ts FILE      (pumi do runs this; crates/pumi-cli/src/run_do.rs)
//
// What `pumi do` passes, in the environment: PUMI_DO (the arguments, JSON), PUMI_DO_JSON,
// PUMI_DO_FOR (with `--for`, in milliseconds, a script that listens keeps running that long),
// PUMI_DO_PROJECT and PUMI_DO_TAB (`--project`, `--tab`: `self`'s project and tab, else the
// environment's PUMI_PROJECT and PUMI_TAB), PUMI_DO_ALL (`--all`: its handlers hear every
// project's events, not only its own project's) and PUMI_DO_FAIL_FAST (`--fail-fast`: a handler
// that throws ends the script, exit status 1; without it the error is printed and it goes on).
//
// FILE's default export, when it is a function, is called with the arguments; what it answers is
// printed (JSON, or a string as it is). The process exits with `process.exitCode`, 0 by default.
import { dirname, resolve } from "node:path";
import * as sdk from "./sdk/index.ts";
import { connectRpc } from "./rpc.ts";
import { cache } from "./sdk/model.ts";
import { connection } from "./sdk/runtime.ts";

const file = process.argv[2];
const socketPath = process.env.PUMI;
if (!file || !socketPath) {
  console.error("pumi do: needs a file and $PUMI");
  process.exit(2);
}
const args = JSON.parse(process.env.PUMI_DO ?? "{}");
const json = process.env.PUMI_DO_JSON === "1";
const lasting = Number(process.env.PUMI_DO_FOR ?? "0");
const given = (name: string) => process.env[name] || null;
const project = given("PUMI_DO_PROJECT") ?? given("PUMI_PROJECT");
const tab = given("PUMI_DO_TAB") ?? given("PUMI_TAB");
const all = process.env.PUMI_DO_ALL === "1";
const failFast = process.env.PUMI_DO_FAIL_FAST === "1";

/** The project an event is about: its own, or its view's or tab's; `null` when it names none. */
async function projectOf(event: any): Promise<string | null> {
  if (typeof event?.project === "string") return event.project;
  if (typeof event?.data?.project === "string") return event.data.project;
  if (typeof event?.view === "string") return ((await cache.load("view", event.view))?.project as string) ?? null;
  if (typeof event?.tab === "string") return ((await cache.load("tab", event.tab))?.project as string) ?? null;
  return null;
}

/** Without `--all`, a handler hears its own project's events and those about no project. */
async function hears(event: any): Promise<boolean> {
  if (all || !project) return true;
  const of = await projectOf(event).catch(() => null);
  return of === null || of === project;
}

Bun.plugin({
  name: "pumi",
  setup(build) {
    build.module("pumi", () => ({ exports: { ...sdk }, loader: "object" }));
  },
});

const rpc = await connectRpc(socketPath, {
  refs: "do",
  noProvide: "pumi do: a one-shot script cannot provide functions; write a plugin",
  hears,
});
rpc.closed.then(() => process.exit(process.exitCode ?? 1));

rpc.request("channel.setInfo", [{ name: "pumi do" }]).catch(() => {});
const full = resolve(file);
connection.install(rpc.port, { pkg: "local/do", prefix: "local.do.", dir: dirname(full), project, tab, args });
// A handler's error is printed where it happens; with `--fail-fast` it ends the script too.
if (failFast) connection.raised = () => {
  process.exitCode = 1;
  finish();
};

const module = await import(full);
let answer: unknown;
if (typeof module.default === "function") answer = await module.default(args);
if (answer !== undefined) console.log(typeof answer === "string" && !json ? answer : JSON.stringify(answer, null, 2));

function finish() {
  rpc.end();
  process.exit(process.exitCode ?? 0);
}
if (lasting > 0 && connection.live.size > 0) setTimeout(finish, lasting);
else finish();
