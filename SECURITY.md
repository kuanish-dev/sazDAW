# Security policy

## Supported versions

SazDAW is in beta: project formats and APIs still change between releases. Security fixes go into `main` and the
next release; older builds are not patched separately, so please use the latest release.

| Version                                 | Supported |
|-----------------------------------------|-----------|
| Latest release (`0.1.0-beta.1`), `main` | ✅        |
| Older builds                            | ❌        |

Only macOS builds are supported (see "Platform" in the [README](README.md)).

## Reporting a vulnerability

**Please do not report vulnerabilities in public issues, pull requests or discussions.**

Report privately through GitHub: the repository's **Security** tab → **Report a vulnerability**
(<https://github.com/kuanish-dev/sazDAW/security/advisories/new>). Please include:

- the version or commit, and your macOS version;
- what an attacker can do and under which conditions;
- steps to reproduce or a proof of concept (a project file, sound, MIDI file or request), kept as small as possible;
- whether the issue is already known publicly.

If you attach the app's error log (`errors.log`, "Project" → "Error log"), check it first: it contains file paths
from your machine.

SazDAW is a small open-source project, so these times are a best effort:

- acknowledgement of your report within **7 days**;
- a first assessment (confirmed or not, severity, plan) within **14 days**;
- a fix in `main` and a release as soon as it is ready, followed by a published GitHub security advisory that credits
  you (unless you prefer to stay anonymous).

Please keep the details private until the fix is released or 90 days have passed since your report, whichever comes
first. We will tell you before publishing.

## Scope

**In scope** — what SazDAW itself controls:

- **The local MCP server** started by the app (`127.0.0.1:47321/mcp`, see "AI control (MCP)" in the README): any way
  for a web page, another machine or another user account to reach it or control the app through it — for example,
  getting around its protections (it listens only on loopback and refuses requests with an `Origin` header, a foreign
  `Host`, a non-JSON `Content-Type`, or another path or method).
- **Opening untrusted files**: projects, sounds (WAV, FLAC, MP3…), MIDI files, presets and templates — memory
  corruption, code execution, or reading and writing files outside what the action is meant to touch.
- **The desktop shell** (Tauri): what the interface can reach through IPC, its permissions (`app/src-tauri/capabilities`)
  and the content it loads.
- **The VST3 host** (`crates/daw-vst3`, the only crate where `unsafe` is allowed): memory-safety bugs on the host side,
  and the isolation of the plugin scan (it runs in a separate process).
- **Building and releasing**: the CI workflows, the macOS app bundle and DMG, and dependencies we ship, when a flaw in
  them is exploitable through SazDAW.

**Out of scope or by design**:

- **Local programs using the MCP server.** While the app runs, any program running as your user can control it through
  the MCP server: there is no access key, the same as for anything else running under your account. Turn the server
  off with the `DAW_MCP=off` environment variable if you do not need it (`DAW_MCP_PORT` changes the port). Reports
  that let something *other* than a local program get in are in scope (see above).
- **What a connected AI client does.** An AI client you connect over MCP can do what the tools allow — the same as the
  interface, including saving and exporting files where it is told to. How your AI client treats the content it reads
  is a matter for that client.
- **VST3 plugins.** Plugins are native code you choose to install: they run with your user's rights, and plugin
  windows load them into the app's process. Report vulnerabilities in a plugin to its vendor.
- **Crashes without a security impact.** A malformed file that makes the app stop with an error, without memory
  corruption or code execution, is an ordinary bug — please open a regular
  [issue](https://github.com/kuanish-dev/sazDAW/issues/new?template=bug_report.md).
- **Dependency advisories with no reachable path** in SazDAW: please report them upstream (we update dependencies
  anyway).
- Builds for platforms other than macOS, which are not supported.
