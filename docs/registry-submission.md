# Registry submission draft for owner review

This is proposed wording for the existing registry PR. The owner must review it and make it their own before publication, in line with Zed's AI policy. This file is not a posted maintainer reply.

---

Sendline HTTP Client adds request execution and environment selection to Zed's existing HTTP language. It does not bundle another HTTP grammar.

The existing HTTP extension documents a separate HTTP CLI and project tasks for execution. Sendline downloads its native request engine, sends the current editor buffer, and runs inline response tests without Node. Results appear in a private response console with links to the body, headers, and test report.

![Sendline integration diagram](https://raw.githubusercontent.com/Prajwal-ak-0/zed-sendline/main/docs/images/sendline-workflow.png)

The extension ID is `sendline-http-lsp`. It supplies only a language server; the existing HTTP extension owns highlighting. Inline Send Request and Cancel Request controls replace the earlier bundled gutter task. The response console is an editor document.

The comparison and limits are documented in [the extension repository](https://github.com/Prajwal-ak-0/zed-sendline/blob/main/docs/comparison.md). Native release downloads are public and checksum-verified. CI checks both CLI and real stdio LSP execution on macOS and Linux, plus the actual archive produced by Zed's registry packager.

- [x] The contribution guidelines have been reviewed and the relevant guidance followed.

---

Before marking the PR ready, record the exact source commit, its successful CI/release links, and the owner's manual Zed check. Do not claim the manual check before it happens.
