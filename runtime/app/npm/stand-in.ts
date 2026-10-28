// What `pumi` and `pumi/theme` export from the npm package (build/sdk.ts): a stand-in for each
// of the app's values, since only the app has the real ones. A test replaces the module with
// `mock.module("pumi", () => ({ … }))`; without that, code that only imports it still loads.
//
// - reading a property gives another stand-in, so `fs.open` can be kept and `self.plugin` read;
// - calling one gives a rejected promise, as the SDK does when the app is not there: `await`
//   and `.catch` see the error, and reading anything else of the result throws it;
// - using one as a value (`${paths.cache}`, `new ApiError()`, an assignment) throws.

function missing(module: string, name: string): Error {
  return new Error(
    `${module}: \`${name}\` is the app's, and a test has no app. Stand in for it with ` +
      `mock.module(${JSON.stringify(module)}, () => ({ … })) before importing the code that uses it.`,
  )
}

/** A call's result: a rejected promise whose other properties throw the error. */
function rejected(error: Error): Promise<never> {
  const promise = Promise.reject(error)
  return new Proxy(promise, {
    get(target, key) {
      if (key === "then" || key === "catch" || key === "finally") return target[key].bind(target)
      if (key === Symbol.toStringTag) return "Promise"
      throw error
    },
  })
}

export function standIn(module: string, name: string): any {
  return new Proxy(function () {}, {
    apply: () => rejected(missing(module, name)),
    construct: () => {
      throw missing(module, name)
    },
    get: (_, key) => {
      if (key === Symbol.toPrimitive) {
        return () => {
          throw missing(module, name)
        }
      }
      if (typeof key === "symbol" || key === "then") return undefined
      return standIn(module, `${name}.${key}`)
    },
    set: () => {
      throw missing(module, name)
    },
  })
}
