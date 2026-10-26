// What runs a plugin: a worker in the host's process, or (`isolate`, or with the inspector) a
// process of its own running worker.ts with `--isolate`, on the same Bun.
import { fileURLToPath } from "node:url"
import { COMPILED, embedded } from "../native/platform.ts"

export interface Runner {
  post(message: any): void;
  /** Ends it; its `exit` then says `null`. */
  stop(): void;
}

export interface RunnerEvents {
  message(message: any): void;
  /** It ended: why, or `null` when `stop` ended it. */
  exit(why: string | null): void;
}

// In the compiled app (`bun run app`) worker.ts is an entry point of the executable, embedded in
// it (`/$bunfs/root/worker/worker.js`, `B:\~BUN\root\worker\worker.js`).
const WORKER = COMPILED ? embedded("worker", "worker.js") : fileURLToPath(new URL("./worker.ts", import.meta.url));

export function startWorker(on: RunnerEvents): Runner {
  const worker = new Worker(WORKER, { smol: true } as any);
  let [closed, stopping] = [false, false];
  worker.onmessage = (e: MessageEvent) => on.message(e.data);
  worker.addEventListener("error", (e: ErrorEvent) => on.message({ t: "log", stream: "stderr", line: e.message }));
  worker.addEventListener("close", (e: any) => {
    closed = true;
    on.exit(stopping ? null : `its worker exited (code ${e.code})`);
  });
  return {
    post: (m) => {
      if (!closed) worker.postMessage(m);
    },
    stop() {
      // Terminating a worker that has already closed takes Bun down (1.3.14).
      if (closed || stopping) return;
      stopping = true;
      worker.terminate();
    },
  };
}

export function startProcess(on: RunnerEvents, opts: { inspect: boolean }): Runner {
  const argv = [process.execPath, ...(opts.inspect ? ["--inspect"] : []), WORKER, "--isolate"];
  let [exited, stopping] = [false, false];
  const child = Bun.spawn(argv, {
    stdin: "ignore",
    stdout: "pipe",
    stderr: "pipe",
    ipc: (m) => on.message(m),
    onExit: (_p, code, signal) => {
      exited = true;
      on.exit(stopping ? null : `its process exited (${signal ?? `code ${code}`})`);
    },
  });
  lines(child.stdout, (line) => on.message({ t: "log", stream: "stdout", line }));
  lines(child.stderr, (line) => on.message({ t: "log", stream: "stderr", line }));
  return {
    post: (m) => {
      if (!exited) child.send(m);
    },
    stop() {
      if (exited || stopping) return;
      stopping = true;
      child.kill();
    },
  };
}

async function lines(stream: ReadableStream<Uint8Array>, on: (line: string) => void) {
  const decoder = new TextDecoder();
  // A child on Windows ends its lines with `\r\n`.
  const send = (text: string) => on(text.replace(/\r$/, ""));
  let rest = "";
  for await (const chunk of stream as unknown as AsyncIterable<Uint8Array>) {
    rest += decoder.decode(chunk, { stream: true });
    const parts = rest.split("\n");
    rest = parts.pop()!;
    for (const part of parts) send(part);
  }
  if (rest) send(rest);
}
