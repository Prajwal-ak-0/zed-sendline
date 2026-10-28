# Sendline ownership and execution

The existing HTTP language extension owns file recognition and highlighting. Sendline registers one language server for HTTP. It ships no language configuration, grammar, query, task, or global launcher.

## Execution flow

The editor sends a versioned document snapshot through LSP. Sendline supplies a Send Request CodeLens for each parsed request and equivalent code actions. The action contains the document URI, source line, and document version. The server rejects stale actions before preparing or sending a request.

`runner::prepare` parses that snapshot, chooses an environment, loads private variables and saved globals, validates the response handler, and resolves the endpoint once. `runner::execute` sends the prepared operation, saves the response, runs bounded JavaScript, merges global mutations, and saves the test report. It writes no protocol or terminal output.

The CLI and LSP are separate presentation adapters over that same runner. The CLI prints metadata and handles Ctrl-C. `editor::execution` owns the LSP execution lifecycle and standard `window/showDocument` requests.

## Request lifecycle

Each open document has either a running operation or its last completed result. A running operation owns a cancellation channel. Duplicate sends in that document are rejected. Requests in separate documents can run independently.

Cancellation, document closure, and server shutdown drop the HTTP future. A run ID prevents an older operation from replacing the state of a reopened document. Each run owns a separate console file so completion and cancellation cannot overwrite a later run's metadata.

The server asks Zed to open the private console with `takeFocus: false` before sending. If the editor cannot open it, no request is sent. Explicit Response, Headers, Tests, and Console actions open only the stored artifacts of that document, with focus requested. Arbitrary file paths are not accepted as action arguments.

## Module ownership

| Module | Owns |
| --- | --- |
| `src/lib.rs` | Versioned native download, checksum, and extension-work-directory environment |
| `editor.rs` | Open documents, environment controls, versioned diagnostics, and LSP dispatch |
| `editor/execution.rs` | Request controls, cancellation, and explicit artifact opening |
| `request` | Source blocks, headers, bodies, environments, and interpolation |
| `runner` | Preparation, HTTP execution, saved responses, and response-handler orchestration |
| `transport` | Verified TLS, bounded HTTP, same-origin redirects, and URL redaction |
| `response` | Private body/header files and CLI presentation |
| `response/console.rs` | Read-only metadata documents and artifact links |
| `response/script` | Bounded QuickJS execution without host IO |
| `state` | Private atomic artifacts and selected environments |
| `state/globals` | Per-environment mutations under a short file lock |
| `cli` | Standalone command parsing and process exit behavior |

The globals lock covers only reading, merging, and atomically publishing state. HTTP and JavaScript never hold that lock. Script logs and assertion details stay in the private report. Received responses survive test failures.

## Installation boundary

The WASM adapter first checks the explicit executable setting and PATH. Otherwise it downloads the manifest-matching native release, verifies the executable checksum, and installs it in Zed's extension work directory. It supplies that directory's `state` child as `SENDLINE_HOME`. Runtime artifacts therefore stay inside the extension environment.

The standalone CLI has a separate default home-cache location. No launcher or project task is created for Zed. Development installs can seed the native cache; public release tests separately verify anonymous downloads and the downloaded executable.

## Alternatives considered

The previous design bundled an HTTP grammar, runnable queries, a terminal task, and a home-cache launcher. That preserved gutter controls, but duplicated the existing HTTP language and depended on static task-to-binary indirection.

A language-server-only package plus a user-installed global task would preserve terminal execution, but require manual task setup and executable discovery. Sendline instead uses standard CodeLens, code actions, and response documents. This keeps installation independent of project configuration and gives the existing HTTP extension one clear owner for language resources.

The tradeoff is explicit: no bundled gutter tasks or automatic terminal. Zed's current extension API does not expose a general task-registration method or an arbitrary custom panel. The response console is an editor document.
