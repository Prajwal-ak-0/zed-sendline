# Changelog

## 0.5.0

First Sendline release candidate, based on doani/zed-restclient.

- Separate request execution, parsing, transport, environments, storage, scripting, and editor integration.
- Select environments at the top of `.http` and `.rest` files, with nearby inline choices.
- Run individual requests through Zed tasks. Show metadata and response links in a reused terminal without moving editor focus.
- Support public/private environment files, structured variables, multiline URLs, authentication, and JSON or text bodies.
- Execute inline JavaScript response tests and persist environment-scoped JSON globals for later requests.
- Preserve HTTP error responses and script reports privately. Bound response size, script memory, and execution time.
- Follow same-origin redirects only, with TLS verification enabled.
- Download a matching native release on macOS or Linux and verify its SHA-256 checksum. Local builds install independently of their source directory.
- Add loopback LSP/CLI/task regression checks, source-independent installation checks, CI, and a draft-release workflow.

The extension is a release candidate. External file bodies, imported or pre-request scripts, async handlers, cookie persistence, multipart helpers, and Windows are not supported.
