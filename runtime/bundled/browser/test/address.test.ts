// The address bar's rules: what counts as a page, what is a link for the system, what is a search.
import { expect, test } from "bun:test";
import { hostOf, normalize, percentEncode, resolve } from "../src/address.ts";

const SEARCH = "https://s.example/?q={query}";

test("input resolves to urls or a search", () => {
  expect(resolve("https://a.dev/x?y#z", SEARCH)).toEqual({ kind: "web", url: "https://a.dev/x?y#z" });
  expect(resolve(" about:blank ", SEARCH)).toEqual({ kind: "web", url: "about:blank" });
  expect(resolve("localhost:3000/app", SEARCH)).toEqual({ kind: "web", url: "http://localhost:3000/app" });
  expect(resolve("localhost", SEARCH)).toEqual({ kind: "web", url: "http://localhost" });
  expect(resolve("127.0.0.1:8080", SEARCH)).toEqual({ kind: "web", url: "http://127.0.0.1:8080" });
  expect(resolve("[::1]:5173", SEARCH)).toEqual({ kind: "web", url: "http://[::1]:5173" });
  expect(resolve("devbox:8000", SEARCH)).toEqual({ kind: "web", url: "http://devbox:8000" });
  expect(resolve("example.com/path", SEARCH)).toEqual({ kind: "web", url: "https://example.com/path" });
  expect(resolve("rust lang & co", SEARCH)).toEqual({ kind: "web", url: "https://s.example/?q=rust+lang+%26+co" });
  expect(resolve("hello", SEARCH)).toEqual({ kind: "web", url: "https://s.example/?q=hello" });
  expect(resolve("mailto:a@b.c", SEARCH)).toEqual({ kind: "system", url: "mailto:a@b.c" });
  expect(resolve("app://settings", SEARCH)).toEqual({ kind: "system", url: "app://settings" });
  expect(resolve("javascript:alert(1)", SEARCH)).toBeNull();
  expect(resolve("  ", SEARCH)).toBeNull();
});

test("urls normalize for comparison", () => {
  expect(normalize("HTTPS://Example.COM")).toBe("https://example.com/");
  expect(normalize("https://example.com/A#")).toBe("https://example.com/A");
  expect(normalize("http://x.dev?q=1")).toBe("http://x.dev/?q=1");
  expect(normalize("https://x.dev/a")).not.toBe(normalize("https://x.dev/b"));
});

test("a search encodes what it is given, byte by byte", () => {
  expect(percentEncode("a b")).toBe("a+b");
  expect(percentEncode("x&y=1")).toBe("x%26y%3D1");
  expect(percentEncode("-_.~")).toBe("-_.~");
  expect(percentEncode("é")).toBe("%C3%A9");
});

test("a page is named by its host, and `about:blank` has none", () => {
  expect(hostOf("https://a.dev/b?c#d")).toBe("a.dev");
  expect(hostOf("http://localhost:3000/")).toBe("localhost:3000");
  expect(hostOf("file:///tmp/a.html")).toBe("");
  expect(hostOf("about:blank")).toBe("");
});
