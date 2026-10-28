# How Sendline relates to existing HTTP extensions

Sendline supplies request execution for the existing HTTP language. It does not provide a second grammar, syntax-highlighting configuration, or runnable query. Install the published HTTP language extension for those language resources.

The distinction is the execution workflow. Sendline downloads its versioned native request engine, sends the current editor snapshot, resolves public/private environments, and runs bounded inline JavaScript response tests. Results are private files linked from an editor metadata console. No project task, separately installed HTTP CLI, Node runtime, or home-cache launcher is needed.

## Existing integrations

The [HTTP extension's README](https://github.com/tie304/zed-http/blob/main/README.md) documents installing an HTTP CLI such as httpYac and adding `.zed/tasks.json` definitions to send requests.

The [Kulala HTTP README](https://github.com/HusuSama/zed-kulala-http/blob/main/README.md) documents a Node-based language server and an additional HTTP CLI plus a task definition for execution. Its syntax, completion, and formatting workflows remain useful; Sendline does not claim to replace all of them.

These are setup comparisons based on those repositories' published documentation, checked on 28 October 2026. They are not claims that other engines cannot support environments or response scripts.

## Scope of this submission

| Resource | Owner |
| --- | --- |
| HTTP file recognition, grammar, and highlighting | Existing HTTP language extension |
| Send Request and Cancel Request controls | Sendline's standard LSP CodeLens and code actions |
| Environment selection and diagnostics | Sendline language server |
| HTTP transport and response tests | Downloaded native Sendline engine |
| Metadata console and explicit response links | Standard LSP document opening and private files |

![Integration diagram, not an editor screenshot](images/sendline-workflow.png)

The ID `sendline-http-lsp` identifies the function and the language-server-only package. The display name is Sendline HTTP Client. The official registry artifact must declare only `language-servers`; `tests/registry_package.py` verifies the actual archive.

## Deliberate limits

The language-server-only package does not supply gutter tasks or an automatic terminal. Inline controls use CodeLens, which users enable in HTTP settings, with code actions as a fallback. Responses use ordinary editor documents, not a custom panel. Windows, multipart helpers, external request-body files, pre-request scripts, and complete JetBrains API compatibility are outside this release.

The previous local build bundled the HTTP language because Zed ties static language tasks to language resources. This submission removes that ownership conflict and uses standard LSP execution instead.
