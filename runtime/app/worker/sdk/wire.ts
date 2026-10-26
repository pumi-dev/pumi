// The worker's wire: the connection to the server the plugin (or `pumi do`) runs against.
//
// Events are `event.on`'s: the server sends each to the channel that subscribed (the app's, for a
// plugin, which passes them to the plugin's worker; `pumi do`'s own socket), with the handle.
import { connection } from "./runtime.ts";
import type { Wire } from "./transport.ts";

/** The plugin's connection, as the SDK's wire: calls and events go to its server. */
export function connectionWire(): Wire {
  return {
    call: (name, args) => connection.call(name, args),
    on: async (events, opts, fn) => {
      const handle = await connection.call("event.on", [events, { match: opts.match, once: opts.once, desc: opts.desc }]);
      connection.listen(handle, fn, !!opts.once);
      return handle;
    },
    off: (handle) => {
      connection.unlisten(handle);
      return connection.call("event.off", [handle]);
    },
  };
}
