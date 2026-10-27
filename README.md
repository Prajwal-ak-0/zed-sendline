# Sendline for Zed

A local HTTP client for `.http` and `.rest` files. Release candidate for macOS and Linux.

This repository is being prepared for publication. Sendline is not yet listed in Zed's extension registry; use the local build below until a reviewed release is available.

Choose an environment in the header at the top of a `.http` file. Click the play button in the gutter beside any request to send it.

The integrated terminal shows the endpoint, environment, status, duration, response size, and content type. The request editor keeps focus. Cmd-click the **Response** path to open the formatted body, or the **Headers** path to inspect headers. Sending a request does not open a response tab automatically.

```http
### Get current user
GET {{baseUrl}}/api/users/me
Authorization: Bearer {{token}}

### Create user
POST {{baseUrl}}/api/users
Content-Type: application/json

{"name": "Test User", "role": "admin"}
```

## Use environments

Put `http-client.env.json` beside your requests or in a parent directory:

```json
{
  "$shared": {"apiVersion": "v1"},
  "local": {"baseUrl": "http://localhost:8000"},
  "staging": {"baseUrl": "https://staging.example.com"}
}
```

Put tokens in `http-client.private.env.json` in the same directory, and add that filename to your project's `.gitignore`:

```json
{
  "local": {"token": "your-local-token"},
  "staging": {"token": "your-staging-token"}
}
```

Click the header to reveal environment choices beside it. Click a choice to select it and collapse the list. Click the header again to close the choices without changing environments. Zed extensions cannot position a custom dropdown beneath the header.

The selector stays available while requests contain syntax errors or unsupported response scripts. Multiline URLs with indented `?` and `&` query parameters are supported.

If a saved environment is removed, select another environment before sending. The client does not silently switch to a different server.

Your choice is shared by files using the same environment directory and survives editor and language-server restarts. A `# @env local` comment supplies the initial choice. Otherwise, `local` is the initial default when available. A sole environment is selected automatically. With multiple environments and no default, select one before running a request.

The nearest parent directory containing either environment file supplies both files. Private values override public values. A selected environment overrides `$shared` within each file. File declarations such as `@name = value` override environment variables. Nested variables and JSON objects or arrays work. For example, `{{account.ids[0]}}` reads a structured environment value. Missing and circular variables stop the request before it is sent.

## Run requests

The gutter play button uses Zed's native task system. It saves the current `.http` file before sending, so the request matches your latest edits. The terminal tab is reused for subsequent requests. Ctrl-C in that terminal cancels an active request. Missing variables appear as diagnostics on their request lines before sending. An active request must finish or be stopped before rerunning the task.

Example output:

```text
Sendline  ·  local  ·  Get current user
GET http://localhost:8000/api/users/me
200 OK  ·  24 ms  ·  1.2 KiB  ·  application/json
Response  /Users/you/Library/Caches/sendline/responses/...json
Headers   /Users/you/Library/Caches/sendline/responses/...headers.txt
Cmd-click a filename to open it.
```

JSON responses are formatted into `.json` files. Text and binary bodies are preserved in `.txt` and `.bin` files. HTTP errors keep their response bodies and headers and mark the task unsuccessful.

Request authorization and body values are not printed to the terminal. URL user information and query values are hidden. Responses can contain sensitive server data. The cache directory is private to your user, and response files use owner-only permissions on macOS and Linux. Response files remain available after a language-server restart.

On macOS the cache is `~/Library/Caches/sendline`. On Linux it is `$XDG_CACHE_HOME/sendline`, or `~/.cache/sendline`. `SENDLINE_HOME` overrides the location for tests.

## Install locally

Requires Rust installed through rustup and Zed:

```sh
./scripts/package.sh
python3 scripts/install.py --enable-buttons
```

The packaging script downloads the pinned HTTP grammar and official WASI SDK, then compiles the extension and native backend. The installer copies the compiled package into Zed's extensions directory. It only updates an installation created by this script. The native client is copied into the extension's versioned work directory. You can move or remove the source checkout after installation.

After updating an open Zed window, run **editor: restart language server** in a `.http` file, then close and reopen that file. Zed can retain the old header click action across a server restart. The header may sit just above the first visible line, so scroll to the top. If it remains hidden, run **editor: toggle code lens**. The language server must start once to register the versioned request runner used by the gutter buttons.

To rebuild after source changes:

```sh
./scripts/build.sh
python3 scripts/install.py --enable-buttons
```

The portable task uses a versioned launcher registered in the private cache. It contains no checkout paths. The WASM adapter downloads the matching GitHub release into Zed's extension work directory and verifies the executable's SHA-256 checksum. Before that release exists, use the installer above. A custom executable can be set with `lsp.sendline.binary.path` in Zed settings.

Try the included requests with a loopback-only demo API:

```sh
python3 examples/server.py
```

Open `examples/requests.http` and click a request's gutter play button.

## Supported requests

GET, POST, PUT, PATCH, DELETE, HEAD, OPTIONS, TRACE, and CONNECT are supported. Separate requests with `###`. No additional blank line is needed before the method. Headers, bearer tokens, Basic authentication, JSON and text bodies, CRLF files, and Unicode bodies work.

Basic authentication accepts an encoded value or `Authorization: Basic username password`. Built-in variables include `{{$uuid}}`, `{{$guid}}`, `{{$timestamp}}`, and `{{$datetime}}`.

TLS verification stays enabled. Connections time out after 10 seconds and complete requests after 30 seconds. Same-origin redirects are limited to 10. A redirect to a different scheme, host, or port is returned as a 3xx response without forwarding credentials. Send a separate request to that destination if intended. Responses are limited to 10 MiB. Multipart helpers, external file bodies, external JavaScript imports, pre-request scripts, asynchronous handlers, and cookie persistence are not implemented.

## Response scripts and tests

Inline JetBrains response handlers run after the HTTP response arrives:

```http
GET {{baseUrl}}/api/items

> {%
client.test("List items", function () {
    client.assert(response.status === 200, "Expected HTTP 200");
    client.global.set("items", response.body.items);
});
%}
```

The script receives `response.status`, the parsed JSON or text `response.body`, `response.contentType`, and `response.headers.valueOf`, `valuesOf`, and `all`. `client.test` registers tests that run after the handler, in order. A failed assertion fails that test and the task; other tests still run. The response remains available even when a test or script fails.

`client.global.set`, `get`, `clear`, `clearAll`, and `isEmpty` work with JSON values. Saved values are available as `{{name}}` in later requests. JSON property and index paths work too, for example `{{items[0]["id"]}}` or `{{user.profile.name}}`. Globals persist privately in this client's cache and are isolated by environment directory and environment name. Environment variables override globals; file declarations override both. Concurrent requests merge their changes without overwriting unrelated variables. Use `client.global.clearAll()` to clear that environment's saved variables.

The terminal shows passed and failed counts and a Cmd-clickable **Report** filename. That private JSON file contains test names, assertion details, and `client.log` or `console.log` output. Logs and assertion messages stay out of terminal output because they can contain tokens or response data.

Scripts run in an embedded QuickJS engine; Node.js is not required. Each handler is limited to one second and 128 MiB, without filesystem, process, or network APIs. Script syntax is checked before sending, without executing the handler. Unsupported external script and body directives fail before sending.

## Repository layout

```text
src/                       Zed WASM adapter and native-client installer
crates/sendline/src/       native request runner
  request/                 environments and variable resolution
  response/                JavaScript response handlers and tests
  state/                   persisted response variables
  editor.rs                LSP controls and versioned diagnostics
  request.rs               request syntax and source locations
  response.rs              terminal output and response files
  state.rs                 private files and environment selection
  transport.rs             HTTP preparation and execution
  runner.rs                request workflow
  cli.rs                   command-line input
  main.rs                  process entry point
tests/                     real HTTP, LSP and installation checks
scripts/                   build, install and release tools
examples/                  requests, sample environments and demo API
docs/                      architecture and ownership
```

## Maintain the client

See [the architecture](docs/architecture.md) for module ownership and [contributing](CONTRIBUTING.md) for the update workflow. The HTTP client has its own repository, builds, installer, runtime state, and tests. It does not depend on the DB client.

## Compatibility

Sendline supports the request, environment, and inline response-test workflows described above. It does not implement every JetBrains HTTP Client API. See the unsupported features above before moving an existing collection. After a response script saves globals, diagnostics for dependent open requests refresh automatically within about one second.

## Verify changes

```sh
scripts/check.sh --integration
```

The verification script uses a loopback HTTP server and real LSP and CLI processes. It checks inline selection, persistence across restarts, shared environments, the native task command, filenames containing shell characters, seven HTTP methods, private response links, Unicode bodies, HTTP errors, query redaction, response handlers, failed assertions, script limits, and variable chaining across CLI processes.

Based on [doani/zed-restclient](https://github.com/doani/zed-restclient), under the included MIT license.
