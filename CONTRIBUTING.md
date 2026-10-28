# Change Sendline

Read [the architecture](docs/architecture.md) to find the module that owns a change.

## Build and install

Run `scripts/package.sh` to compile the native backend and WASM adapter. The package contains no HTTP grammar or language resources. Install the existing HTTP language extension in Zed separately.

Run `python3 scripts/install.py --replace-legacy --enable-buttons` to install an owned development build. Existing servers keep their executable until they exit. Restart the language server, then close and reopen the request file so its controls belong to the new server.

## Verify a change

Run `scripts/check.sh --integration` for formatting, Clippy, unit tests, native and WASM builds, CLI regressions, and real stdio LSP request execution against loopback fixtures. Run `python3 tests/installation.py` against the packaged development install.

`tests/editor_execution.py` verifies unsaved editor snapshots, stale action rejection, private metadata, explicit artifact opening, HTTP and test failures, cancellation, duplicate clicks, and document close/reopen behavior. Test only observable behavior and boundary contracts.

For a visual check, run `python3 examples/server.py` and open `examples/requests.http` in Zed. Select an environment, send a request, cancel another request, and open the response, headers, and test report. Verify that the console does not steal focus. Test code actions with CodeLens disabled too.

Keep the repository independent of the database extension. Preserve upstream attribution. Never add AI authors or co-authors.

## Prepare a release

Run `python3 scripts/preflight.py`, `scripts/check.sh --integration`, `scripts/package.sh`, and `python3 tests/installation.py`. Inspect the exact candidate in Zed. Push the reviewed source to `main` and wait for CI, including the official registry package builder.

Tag the reviewed commit with its manifest version, such as `v0.6.0`. Verify the four draft release targets before publishing. Run `python3 tests/release.py --tag v0.6.0` against anonymous public downloads.

The registry review is separate from the native release. Follow [the publication procedure](docs/publishing.md). The owner must review the submission wording and own replies to maintainers.
