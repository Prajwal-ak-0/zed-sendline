# Publish Sendline

Sendline has two distributions: the native executables on GitHub Releases and the WASM/language package in Zed's extension registry. The registry package downloads the native version that matches its own manifest.

## Build and publish the native release

1. Keep the versions in `Cargo.toml`, `crates/sendline/Cargo.toml`, and `extension.toml` equal. The extension ID remains `sendline-http-lsp`.
2. Run `scripts/check.sh --integration` and `python3 tests/installation.py`.
3. Push the reviewed source to `main`. Wait for every CI job, including `registry-package`, to pass.
4. Tag that commit with `v<version>` and push the tag. The draft-release workflow builds and tests the four native targets and packages dependency notices and checksums.
5. Verify the draft's assets and publish the release with its release notes.
6. Run `python3 tests/release.py --tag v<version>`. It downloads every asset without authentication, checks both checksums and the CPU architecture, verifies the Linux glibc minimum, and exercises the downloaded host executable through the integration suite.

Do not move a published version tag or replace published assets. Fixes use a new version.

## Verify a first installation

Download `registry-package` from the successful CI run. This is the package produced by Zed's own extension builder with the registry's Rust version.

Install the existing HTTP language provider. Test the candidate in a separate Zed user-data directory and configuration, with an empty native extension work directory. Verify that opening an HTTP project downloads the published client and can execute a request. Also check environment code actions with default CodeLens settings, then enable `languages.http.code_lens` and check inline Send Request, Cancel Request, the environment header, and response links. Confirm the metadata console opens without stealing focus.

A development installation with a pre-seeded native cache does not prove that the public download path works.

## Submit to Zed's registry

Read Zed's [contribution guidelines](https://github.com/zed-industries/extensions/blob/main/CONTRIBUTING.md), [AI policy](https://github.com/zed-industries/extensions/blob/main/AI_POLICY.md), and [publishing guide](https://zed.dev/docs/extensions/publishing/publishing-guide).

For a first contribution, the account owner must review and sign Zed's [Contributor License Agreement](https://zed.dev/cla). If the CLA bot has already flagged the PR, the owner can post `@cla-bot check` after signing to refresh that check.

Zed requires a human who understands and owns the submission. It does not accept autonomous agent contributions or AI-generated maintainer comments. The owner must review the exact change and own the PR description and any replies. AI may assist with code and with editing the owner's wording, as allowed by that policy.

The registry package must provide only `language-servers`. Run `python3 tests/registry_package.py dist/registry` against the actual package. The registry change adds exactly one HTTPS submodule, its `extensions.toml` entry, and the submodule commit. Use a commit reachable from `main`, with the same version as the public native release. Run `pnpm sort-extensions` and the registry's validation checks.

The owner then submits the reviewed change to `zed-industries/extensions`. Zed's maintainers control approval and merging. Submission alone does not make the extension installable from the registry.

After the merge and registry deployment, search for Sendline in a clean Zed Extensions view, install it, and repeat the first-installation checks. Confirm availability in Zed before announcing that registry installation is live.
