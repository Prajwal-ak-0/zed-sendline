# Change the HTTP client

Read [the architecture](docs/architecture.md) to find the module that owns a change.

## Build and install

Run `scripts/build.sh` after native changes. Existing requests and language servers keep their old executable until they exit.

Run `python3 scripts/install.py --enable-buttons` after changing WASM, language queries, or task definitions. The installer copies the native executable into Zed's versioned extension work directory. The checkout is not required after installation.

After rebuilding the language server, restart it from a `.http` editor. Close and reopen that file so Zed creates fresh CodeLens actions.

For a fresh checkout, `scripts/package.sh` also builds the pinned HTTP grammar with the official WASI SDK. `WASI_SDK_PATH` can point to an existing SDK.

## Verify a change

Run `scripts/check.sh --integration`. This checks formatting, Clippy, unit tests, native and WASM builds, and the full LSP/task/HTTP flow against a loopback server.

For a visual review, run `python3 scripts/demo_server.py` and open `examples/requests.http` in Zed. Choose an environment, send a request, and inspect its response through the terminal filename. Sending keeps the request editor focused.

Use temporary fixtures for new regressions. Check the observable request, response, or diagnostic. Avoid tests that only repeat an implementation detail.

Keep the repository independent. Its build and tests must work without any sibling extension checkout.

## Prepare a release

Run `python3 scripts/preflight.py`, `scripts/check.sh --integration`, `scripts/package.sh`, and `python3 scripts/verify_install.py`. Inspect the installed build in Zed before committing. Tag the reviewed commit with its manifest version, for example `v0.5.0`. The release workflow builds four native targets and creates a draft only after every build passes. Publish that draft after checking all assets. The Zed registry submission is a separate review; see [Zed publishing](https://zed.dev/docs/extensions/publishing/publishing-guide).

Keep commits and PRs focused. Preserve the upstream license attribution. Do not add AI authors or co-authors.
