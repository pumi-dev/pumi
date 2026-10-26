// What the SDK talks to: the wire, which speaks the server's export (api/pumi-api.json): prefixed
// ids, the export's function and event names.
//
// An event as the wire sends it: what fired, what it matched, the objects it is about, its data.
export interface WireEvent<D = any> {
  event: string;
  match: string;
  /** The session it is about (`$3`, `devbox/$3`); none when it is about no one. */
  session?: string | null;
  view?: string | null;
  tab?: string | null;
  data: D;
  seq?: number;
}

export interface WireOnOptions {
  /** Globs tested against the event's `match`: every one without them. */
  match?: string | string[];
  /** Runs once, then it is gone. */
  once?: boolean;
  desc?: string;
}

/** The wire the app's engine and a worker's connection are: the export's names, prefixed ids. */
export interface Wire {
  /** Calls `name` (a function of the export's, or one a plugin provides) with its arguments in order. */
  call(name: string, args: unknown[]): Promise<any>;
  /** Hears `events` (names or globs) that match `opts.match`; answers the subscription's handle. */
  on(events: string[], opts: WireOnOptions, fn: (e: WireEvent) => void): Promise<number>;
  /** Drops the subscription `on` answered. */
  off(handle: number): Promise<unknown>;
}

/** Whether `glob` (`*` any run of characters) matches `text`. */
export function globMatch(glob: string, text: string): boolean {
  const re = new RegExp(`^${glob.replace(/[.+?^${}()|[\]\\]/g, "\\$&").replaceAll("*", ".*")}$`);
  return re.test(text);
}

/** Whether any of `match` (none: every one) matches `text`. */
export function matches(match: string | string[] | undefined, text: string): boolean {
  if (match === undefined) return true;
  return [match].flat().some((m) => globMatch(m, text));
}
