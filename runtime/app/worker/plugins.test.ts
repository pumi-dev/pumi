import { afterAll, test } from "bun:test";
import { mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { Host, LIMITS, type Frame, type Load } from "./plugins.ts";

const dir = mkdtempSync(join(tmpdir(), "pumi-host-"));
afterAll(() => rmSync(dir, { recursive: true, force: true }));

/** The host, and the lines its plugin logs as they come. */
function hosted() {
  const lines: string[] = [];
  const waiting = new Set<() => void>();
  const host = new Host((f: Frame) => {
    if (f.type !== "log") return;
    lines.push(f.line);
    for (const wake of [...waiting]) wake();
  }, LIMITS);
  const logged = (line: string) =>
    new Promise<void>((resolve, reject) => {
      const timer = setTimeout(() => reject(new Error(`never logged ${line}: ${JSON.stringify(lines)}`)), 5000);
      const check = () => {
        if (!lines.includes(line)) return;
        clearTimeout(timer);
        waiting.delete(check);
        resolve();
      };
      waiting.add(check);
      check();
    });
  return { host, logged };
}

test("a reload runs the plugin's code as it is now, not as it first loaded", async () => {
  const main = join(dir, "main.ts");
  writeFileSync(main, 'console.log("one");\n');
  const spec: Load = { pkg: "me/dev", dir, main, api: 1, prefix: "me.dev.", paths: { config: "", state: "", cache: "" }, isolate: false };
  const { host, logged } = hosted();
  try {
    host.frame({ type: "load", ...spec });
    await logged("one");
    writeFileSync(main, 'console.log("two");\n');
    host.frame({ type: "reload", pkg: spec.pkg, spec });
    await logged("two");
  } finally {
    host.shutdown();
  }
});

test("a reload takes the spec the app gives it", async () => {
  const other = join(dir, "other.ts");
  writeFileSync(join(dir, "first.ts"), 'console.log("first");\n');
  writeFileSync(other, 'console.log("other");\n');
  const spec: Load = { pkg: "me/moved", dir, main: join(dir, "first.ts"), api: 1, prefix: "me.moved.", paths: { config: "", state: "", cache: "" }, isolate: false };
  const { host, logged } = hosted();
  try {
    host.frame({ type: "load", ...spec });
    await logged("first");
    host.frame({ type: "reload", pkg: spec.pkg, spec: { ...spec, main: other } });
    await logged("other");
  } finally {
    host.shutdown();
  }
});
