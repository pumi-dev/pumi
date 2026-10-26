# pumi/browser

Bundled with pumi: a web page in a tab, with back, forward, reload and an address bar over it,
and where cmd+click sends a terminal's links. The app itself has only the webview (`<WebView>`,
`useWebview`), which any plugin draws pages with; the browser is this plugin.

It is listed with your plugins and can be turned off, not removed (Settings ▸ Plugins, or
`pumi plugin update pumi/browser --no-enabled`). Off, its tabs, commands and settings go, and a
terminal's cmd+clicked links open in the system's browser.

```sh
pumi call pumi.browser.open '{"url":"https://pumi.dev"}'    # beside this tab
pumi call pumi.browser.openView '{"url":"example.com"}'     # in a view of its own
pumi tab open pumi.browser.page '{"url":"https://example.com"}' --split right
```

`open`'s address is its argument (`{ url }`, or a plain string), else `home_url`, else the address
bar.

## Settings

| option | what it is |
| --- | --- |
| `pumi.browser.home_url` | the page a new browser tab opens; empty starts in the address bar |
| `pumi.browser.search_url` | the address bar's search engine; `{query}` is what was typed |
| `pumi.browser.open_links` | where cmd+click opens a terminal's links: a browser tab (`app`) or the system browser (`system`); shift+cmd+click opens the other |
| `pumi.browser.url` | a tab's own: the page it is on. Setting it navigates; the page writes it back as it goes |
| `pumi.browser.webview_devtools` | whether pages get their Web Inspector (alt+cmd+i), which is the webview's; the app reads it and hands it to the webview |

## Links from terminals

A terminal's cmd+clicked link goes to `pumi.browser.open` with `{url, tab, link: true, shift}`:
it opens beside that terminal, or in the system's browser when `open_links` says `system` (shift
says the other) or the link is no page (`mailto:`). While this plugin is off, the app opens every
link in the system's browser.

## The tab

| | |
| --- | --- |
| the toolbar | back, forward, reload (stop while it loads), the address, the page's name, devtools |
| the keys | `cmd+l` the address, `cmd+r` reload, `cmd+[` back, `cmd+]` forward, while it has the keyboard |
| the address bar | a URL, a host (`example.com`, `localhost:3000`) as an address, anything else as a search |
| the state | the address is the tab's `url` option and the name is its `title`: tabs that come back come back where they were |

`tab.call` reaches the block's own methods (`navigate`, `back`, `forward`, `reload`) and the
protocol's (`read`, `selection`, `input`):

```sh
pumi tab navigate %3 '{"url":"example.com"}'
pumi tab read %3                             # the name, then the address
pumi tab input %3 --text 'example.com/next'  # typed into the address bar, and Enter
pumi call pumi.browser.back %3                # also forward, reload, navigate, url, title
```

What a webview cannot do for it: no profile, cookies, downloads or zoom of its own; a link the page
opens in a new window goes to the system's browser; another site's document is not readable, so
`selection` is empty.

## Working on it

```sh
bun test                                       # the address bar's rules
../../app/node_modules/.bin/tsc --noEmit -p .  # its types, against the checkout's SDK
```
