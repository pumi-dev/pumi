# Security

## Reporting a vulnerability

Report it privately, through GitHub's private vulnerability reporting: on
[github.com/pumi-dev/pumi](https://github.com/pumi-dev/pumi), Security → Report a vulnerability.
Don't open a public issue or pull request for it. Say what pumi version (`pumi --version`) and OS
it is on, what an attacker needs, and how to reproduce it.

A fix ships as a new release; the updater installs it. Releases are not patched in place.

## What pumi does not protect against

Plugins and themes are not sandboxed, by design. One runs with your permissions: it reads and
writes your files, reaches the network, runs programs, and drives every terminal through the same
API pumi uses. Install what you would run as code. A plugin doing what its code says is not a
vulnerability in pumi.

The server's socket (`$PUMI`) is readable and writable only by your user on macOS and Linux.
Anything that runs as you can drive pumi through it, as the `pumi` command does.

Updates are trusted by the release manifest's minisign signature, which names each archive's
SHA-256 ([Releasing](docs/releasing.md)). A way past that check is a vulnerability.
