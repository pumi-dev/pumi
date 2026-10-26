// The worker SDK's `Port` over the server's socket instead of the app: what `pumi do` installs
// under the `pumi` module (runtime/app/worker/do.ts). A `call` goes to the server as a JSON-RPC
// request, its answer comes back as a `reply`, `rpc.event` (an event of a subscription this
// channel made) reaches `deliver.event`, and `rpc.callback` (a callback this channel holds,
// `{chan: 0, ref: "<refs>/<n>"}`) reaches `deliver.callback`.
import { connect, type Socket } from "node:net";
import { ApiError, deliver, type Port } from "./sdk/runtime.ts";

export interface RpcOptions {
  /** What this channel's callbacks are named on the server: `do` makes `do/3`. */
  refs: string;
  /** What a `provide` is refused with: a channel over JSON-RPC serves no functions. */
  noProvide: string;
  /** A call as it goes: its params, or a throw (an `ApiError`) refusing it before it leaves. */
  outgoing?(name: string, args: unknown[]): unknown[];
  /** Runs on a call's answer before its caller has it; what it throws is the caller's error. */
  answered?(name: string, args: unknown[], result: unknown): Promise<void> | void;
  /** Whether an event reaches its listener; events keep their order while it decides. */
  hears?(event: any): Promise<boolean> | boolean;
}

export interface Rpc {
  port: Port;
  /** Calls `method` on the server directly, not through the SDK (`channel.setInfo`, …). */
  request(method: string, params: unknown[]): Promise<any>;
  /** Settles when the server closes the socket, or it is ended. */
  closed: Promise<void>;
  /** Ends the socket; settles once it is closed. */
  end(): Promise<void>;
}

const fail = (err: any) => ({
  kind: err instanceof ApiError ? err.kind : "exception",
  message: err instanceof Error ? err.message : String(err),
});

/** Connects to the server at `socketPath` and answers the `Port` the SDK is installed over. */
export async function connectRpc(socketPath: string, opts: RpcOptions): Promise<Rpc> {
  const socket: Socket = connect(socketPath);
  await new Promise<void>((ok, failed) => socket.once("connect", ok).once("error", failed));
  const send = (message: unknown) => socket.write(`${JSON.stringify(message)}\n`);

  /** `args` as the server takes them: a callback `{ ref }` is this channel's (`<refs>/<n>`). */
  const bind = (value: any): any => {
    if (Array.isArray(value)) return value.map(bind);
    if (value && typeof value === "object") {
      const keys = Object.keys(value);
      if (keys.length === 1 && keys[0] === "ref" && typeof value.ref === "number") return { chan: 0, ref: `${opts.refs}/${value.ref}` };
      return Object.fromEntries(keys.map((k) => [k, bind(value[k])]));
    }
    return value;
  };

  // The SDK's calls are numbered by it; the requests made here directly are `rpc/<n>`.
  const sdkCalls = new Map<number, { name: string; args: unknown[] }>();
  const direct = new Map<string, { resolve(v: any): void; reject(e: any): void }>();
  let next = 0;

  const request = (method: string, params: unknown[]): Promise<any> =>
    new Promise((resolve, reject) => {
      const id = `rpc/${++next}`;
      direct.set(id, { resolve, reject });
      send({ jsonrpc: "2.0", id, method, params });
    });

  const port: Port = {
    post(m: any) {
      switch (m.t) {
        case "call": {
          let args: unknown[] = m.args ?? [];
          try {
            if (opts.outgoing) args = opts.outgoing(m.name, args);
          } catch (err) {
            return deliver.reply({ id: m.id, error: fail(err) });
          }
          if (opts.answered) sdkCalls.set(m.id, { name: m.name, args });
          return send({ jsonrpc: "2.0", id: m.id, method: m.name, params: bind(args) });
        }
        case "provide":
          return deliver.reply({ id: m.id, error: { kind: "invalid_params", message: opts.noProvide } });
        case "log":
          return void (m.stream === "stderr" ? process.stderr : process.stdout).write(`${m.line}\n`);
        default:
          return;
      }
    },
  };

  const answer = (m: any) => {
    const error = m.error ? { kind: m.error.data?.kind ?? "exception", message: m.error.message } : undefined;
    if (typeof m.id === "string") {
      const p = direct.get(m.id);
      if (!p) return;
      direct.delete(m.id);
      if (error) p.reject(new ApiError(error.kind, error.message));
      else p.resolve(m.result ?? null);
      return;
    }
    const call = sdkCalls.get(m.id);
    sdkCalls.delete(m.id);
    if (!call || error || !opts.answered) return deliver.reply({ id: m.id, ok: m.result, error });
    Promise.resolve()
      .then(() => opts.answered!(call.name, call.args, m.result ?? null))
      .then(
        () => deliver.reply({ id: m.id, ok: m.result }),
        (err) => deliver.reply({ id: m.id, error: fail(err) }),
      );
  };

  let buffered = "";
  /** The events `hears` is deciding, in order. */
  let heard = Promise.resolve();
  socket.on("data", (chunk) => {
    buffered += chunk.toString("utf8");
    let at: number;
    while ((at = buffered.indexOf("\n")) >= 0) {
      const line = buffered.slice(0, at);
      buffered = buffered.slice(at + 1);
      if (!line.trim()) continue;
      const m = JSON.parse(line);
      if (m.id !== undefined && m.id !== null && (m.result !== undefined || m.error !== undefined)) answer(m);
      else if (m.method === "rpc.event") {
        const [subscription, event] = m.params ?? [];
        if (typeof subscription !== "number") continue;
        if (!opts.hears) deliver.event(subscription, event);
        else {
          const hears = opts.hears;
          heard = heard.then(async () => {
            if (await Promise.resolve(hears(event)).catch(() => true)) deliver.event(subscription, event);
          });
        }
      } else if (m.method === "rpc.callback") {
        const [ref, list] = m.params ?? [];
        const fn = Number(String(ref).split("/").pop());
        deliver.callback(fn, Array.isArray(list) && list.length === 1 ? list[0] : list);
      }
    }
  });
  const closed = new Promise<void>((resolve) => socket.once("close", () => resolve()));
  closed.then(() => {
    for (const p of direct.values()) p.reject(new ApiError("provider_unavailable", "the server closed the connection"));
    direct.clear();
  });

  return {
    port,
    request,
    closed,
    end() {
      socket.end();
      return closed;
    },
  };
}
