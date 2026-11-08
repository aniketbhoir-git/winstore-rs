# Skills for contributors

Agent "skills" are small instruction packs (a `SKILL.md` plus optional references) that a
coding model loads on demand. This page lists the ones that help on this codebase. All are
optional; the spec and the issue text are always enough on their own.

## Project skills (in this repo, loaded automatically by Claude Code)

| Skill | Path | Use it for |
|---|---|---|
| `store-work-resume` | `.claude/skills/store-work-resume/` | Loading the right context to start or resume one issue. |
| `winget-executor` | `.claude/skills/winget-executor/` | Flags, output shapes, exit codes and test fixtures for the winget subprocess layer (issues #4, #6, #7, #12). |

Other agents (Cursor, Codex, pi) can read the same `SKILL.md` files; copy them into the
folder your tool expects.

## Ecosystem skills (install globally in your own tool)

Install with the Skills CLI (`npx skills add <owner/repo@skill> -g -y`) or the Claude Code
plugin marketplace. Checked 2026-11-09; licences are the repos' own.

| Skill | Why it fits here |
|---|---|
| `rust-analyzer-lsp@claude-plugins-official` (plugin) | Compiler-grade diagnostics inside the session. |
| `apollographql/skills@rust-best-practices` | Idiomatic Rust: ownership, `Result` handling, API shape. |
| `wshobson/agents@rust-async-patterns` | tokio tasks, channels, streams, cancellation (executor and services). |
| `affaan-m/ecc@rust-testing` | rstest, proptest, mockall, cargo-llvm-cov; matches SPEC §8. |
| `trailofbits/skills@rust-review` | Security-flavoured Rust review before a PR (CC-BY-SA licence; use, do not copy text into this repo). |
| `nodnarbnitram/claude-code-extensions@tauri-v2` | tauri.conf.json, commands, capabilities, IPC errors. Check against the current Tauri docs. |
| `full-stack-skills/tauri-skills@tauri-ipc`, `@tauri-security` | Typed IPC contracts and the Tauri security model. |
| `sveltejs/ai-tools@svelte-code-writer`, `@svelte-core-bestpractices` | Official Svelte 5 guidance for the `ui/` frontend. |

Reference material that is not a skill: Tauri's `https://v2.tauri.app/llms-full.txt` and
Microsoft's [Pragmatic Rust Guidelines](https://github.com/microsoft/rust-guidelines).

## Not recommended for this project

- Windows VM image builders (Packer) and WPF/WinForms UI-automation testers: Tauri is a
  WebView2 app, so those tools do not apply.
- Any skill outside the public, documented interfaces named in `docs/SPEC.md` §0.

## Reviewing changes

Built-in Claude Code commands `/simplify` and `/code-review`, and the `code-simplifier`
agent, cover simplification and general review. Run them before opening a PR.
