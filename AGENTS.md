# Sendline for Zed

This is a standalone repository. Do not depend on the PostgreSQL extension. Keep the upstream MIT attribution.

## Ownership

- `src/lib.rs`: versioned native download, checksum verification, and Zed integration.
- `client/src/cli.rs`: typed command-line arguments.
- `client/src/lsp.rs`: open documents, environment controls, and diagnostics. It never sends HTTP requests.
- `client/src/parser.rs`: `.http` syntax and request locations.
- `client/src/environment.rs`: public/private environment loading and precedence.
- `client/src/variables.rs`: bounded variable interpolation.
- `client/src/script.rs` and `script_runtime.js`: bounded response-handler execution and the JetBrains script API. Never expose host IO or transmit handler text.
- `client/src/globals.rs`: private variables scoped to the environment directory and selected environment, merged under a short file lock.
- `client/src/runner.rs`: select, prepare, send, save, report.
- `client/src/http_client.rs`: the prepared request and HTTP transport. Display the prepared URL; never resolve a dynamic URL twice.
- `client/src/output.rs`: terminal presentation and saved response files.
- `client/src/storage.rs`: private atomic files and environment selection.

Keep the request editor focused when sending. Open responses only after an explicit user action. Keep environment choices beside their header. Treat missing variables and removed environment selections as recoverable errors before sending.

Save the HTTP response even when assertions or scripts fail. Keep script logs and assertion messages in the private report because they may contain response data or tokens. Syntax checks must compile handlers without executing them or sending requests.

## Verification

Run `scripts/check.sh` for formatting, Clippy, unit tests, and native/WASM builds. Run `scripts/check.sh --integration` to include the loopback HTTP and real LSP checks.

Use only disposable loopback fixtures for verification. Do not copy real project tokens into demos. Do not print authorization values, bodies, URL user information, or query values. Preserve response bodies and headers privately, including HTTP error responses.

After changing the UI integration, inspect it in Zed. Check environment selection, send, cancellation, diagnostics, and Cmd-clicking a response filename. After a language-server rebuild, close and reopen the demo file to avoid Zed retaining an old CodeLens action.

Replace the native executable by rename, as `scripts/build.sh` does. Never overwrite a running executable in place.
