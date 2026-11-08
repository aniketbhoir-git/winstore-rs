# Contributing

Thanks for helping. This is a slow, async project: **there are no deadlines and no one blocks on
anyone else.** Every implementation issue is a self-contained vertical slice, so you can work
fully independently.

## Ways to help

1. **Implement an issue** from [`docs/ISSUE-BACKLOG.md`](docs/ISSUE-BACKLOG.md) — code you
   generate with your own LLM subscription is welcome (see below).
2. **Test on real Windows** — issues labeled `needs-windows-test` are especially valuable.
3. **Improve docs** — `docs/SPEC.md`, the issue template, and the backlog.
4. **Review PRs** against the guardrail checklist in `docs/SPEC.md` §9.

## Process

1. Pick an unassigned issue at https://github.com/aniketbhoir-git/winstore-rs/issues; comment "I'm on this" and assign it to yourself.
2. Branch from `main`: `impl/<issue#>--short-name`.
3. Follow `docs/SPEC.md`. If you're generating code with a model, read
   [`docs/CONTRIBUTING-WITH-LLM.md`](docs/CONTRIBUTING-WITH-LLM.md) first.
4. Compile; run `cargo test`, `cargo clippy`, `cargo fmt`; do a Windows smoke test when the
   issue asks for it.
5. Open a PR with the PR template; tick every acceptance criterion and the guardrail checklist.

## Model tiers

Issues are labeled so you can pick what your subscription can afford:

| Label | Meaning |
|---|---|
| `tier:max` | Needs a top-tier model (Claude Max/Ultra, Sonnet 4.5+, Opus, GPT-5 class). The maintainer usually owns these. |
| `tier:any` | Fine with any decent model (Haiku 4.5, DeepSeek V3.2, Gemini 2.5 Flash, Qwen3). **Community tokens matter most here.** |

Cheap tokens on `tier:any` issues are the single most valuable thing you can donate.

## Code standards

- Positive, neutral terminology per `docs/SPEC.md` §0.
- **No payment, account, or licensing/entitlement logic — ever** (§0 hard rules).
- No telemetry; only the catalogs the user explicitly enables are contacted.
- Rust: clippy clean, `cargo fmt` clean, tests for parsers and state transitions.

## Maintainer

Currently a solo maintainer moving at a slow cadence. PRs are reviewed asynchronously; if yours
sits for a while, it is not personal — feel free to ping.