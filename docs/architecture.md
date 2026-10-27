# HTTP client architecture

The editor owns request text. A native Zed task owns each HTTP operation. The language server owns environment controls and diagnostics. This keeps sending independent of CodeLens visibility and preserves Zed's task cancellation and terminal behavior.

## Request flow

`cli` parses the task arguments. `runner` reads the file, selects the request, loads the chosen environment, and applies file variables. `transport::prepare` resolves variables and builds the request once. The displayed endpoint comes from that prepared request.

`transport::execute` sends the prepared request with verified TLS, redirect limits, timeouts, and a response-size limit. It records the final redirected endpoint. `response` saves the body and headers through `state`, then prints a concise result with editor-openable paths. Ctrl-C drops the request future and does not publish a partial response.

## Editor flow

`editor` keeps open document text and per-document expansion state. Environment selection is stored by environment directory, so related request files share a choice. Changing the selection does not send requests. Invalid variables are diagnosed on their request line.

Environment controls read the document's environment hint independently of request parsing. Incomplete requests and invalid response handlers can produce diagnostics, but cannot hide the selector. The execution parser joins multiline query parameters and separates HTTP bodies from inline response handlers. The language server compiles scripts for syntax checks without executing them.

## Response-handler flow

`runner` validates the selected handler before sending. After receiving and saving the HTTP response, it calls `script::run(handler, response, globals)`. The embedded QuickJS engine receives only response data and a snapshot of saved variables. It has memory and execution limits and no host IO APIs. `script_runtime.js` implements the documented client methods, queues tests, and returns results and variable mutations. The native code handles the result as data.

`state::globals` scopes state by environment directory and environment name. It records mutations rather than replacing a stale snapshot: acquire a per-scope file lock, read the current values, apply the mutations, and atomically publish the private JSON file. Network and script execution never hold this lock. A top-level script error or interrupted execution discards its mutations. Completed tests can save variables even if another test fails.

`response` writes the private test report beside the response, then prints counts and a filename. Test names, assertion details, and script logs can contain response data, so they remain in the report. HTTP failures, script failures, and assertion failures all produce a failing task exit status; an HTTP response is preserved whenever one was received.

The alternatives were to recognize selected JavaScript expressions or launch Node.js. Recognizing expressions would break ordinary JavaScript control flow and closures. A Node subprocess would add a runtime requirement and host capabilities. An embedded engine supports JavaScript semantics within the existing native executable.

A removed saved environment produces an unselected state. The user must make another choice before sending. The client never silently substitutes another server.

Zed can retain a CodeLens renderer and its old action when its text has not changed across a language-server restart. Closing and reopening the file creates fresh actions. The local update instructions include this step.

## Boundaries

| Module | Owns |
| --- | --- |
| `request` | Syntax, request names, source lines, headers, and bodies |
| `request::environment` | Nearest environment directory and public/private precedence |
| `request::variables` | Interpolation, dynamic values, cycles, and expansion limits |
| `response::script` and `script_runtime.js` | Bounded JavaScript execution, response API, tests, and reports |
| `state::globals` | Environment-scoped script state, mutations, and concurrent merges |
| `transport` | Prepared requests, HTTP transport, and redacted endpoints |
| `state` | Private directories and atomic state/artifact writes |
| `response` | Terminal text, useful response filenames, and body formatting |
| `runner` | The request workflow and cancellation |
| `editor` | Zed protocol state and diagnostics |
| `cli` | Command-line input and exit behavior |

The alternative was to execute inside the language server. That would couple requests to editor protocol lifetime and make terminal focus, cancellation, and repeat execution less predictable. The native task keeps these concerns separate.

The repository has its own Cargo workspace, native executable, WASM launcher, grammar, installer, and verification scripts. There are no imports or build dependencies on the database extension.

## Installation boundary

The WASM adapter honors an explicit LSP executable setting or `sendline` on PATH. Otherwise it downloads the release matching the manifest version and platform, verifies the extracted executable against the release checksum, and installs it inside Zed's extension work directory. It never embeds a developer checkout path.

Zed language tasks are static and cannot receive the WASM adapter's resolved executable path. When the native language server starts, `state::register_runner` atomically publishes a versioned symlink in its private cache. The portable shell task runs through that link. Versioned links keep extension upgrades separate. The local installer seeds the same work directory, so local and published builds use the same task flow. The terminal task saves the current file, keeps editor focus, and reuses its terminal.

Parsing reports errors per request block. A malformed neighboring request does not block a valid request, and selecting a malformed block never falls through to another endpoint.

The language server watches the private globals directory revision once per second. An atomic write from a terminal task triggers diagnostics for open documents, so successful request chaining clears missing-variable errors without an editor change. The watcher stops on LSP shutdown and performs no network requests.

Editor diagnostics carry the document version used for validation. Incoming older edits are ignored, and work for a changed or closed document is discarded before publication. Integration tests wait for the matching diagnostic notification rather than treating a CodeLens response as a synchronization barrier.
