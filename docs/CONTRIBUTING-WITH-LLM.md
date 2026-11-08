# Contributing with your own LLM tokens

This repository is designed so that community members can contribute **compute/tokens** rather
than just hours. Every implementation issue is a paste-ready prompt: you use your own model
subscription (Cursor, Claude, ChatGPT, OpenRouter, anything) to generate the code, then you
verify it and open a PR. The project gets reviewed, tested code — you get to ship something real
on whatever budget you already pay for.

## The flow in one screen

1. Pick an unclaimed issue in `docs/ISSUE-BACKLOG.md` whose tier matches what you have.
2. Read `docs/SPEC.md` §0 plus the sections the issue references.
3. In a fresh chat/session, paste **both**:
   - `docs/SPEC.md` §0 (terminology + the four hard rules), and
   - the full issue body, verbatim.
4. Generate into the repo. Ask the model to work in small increments and write tests as it goes.
5. Compile, run the tests, and fix what breaks — **your judgment here is the real contribution;
   never paste code you haven't read and run.**
6. Open a PR with the template, including test output or a Windows screenshot.

## Which issues need which tier

| Label | Model you need | Typical issues |
|---|---|---|
| `tier:max` | Claude Max/Ultra, Sonnet 4.5+, Opus, GPT-5 class | winget parser, Windows Deployment interop, msstore adapter, elevation |
| `tier:any` | Haiku 4.5, DeepSeek V3.2, Gemini 2.5 Flash, Qwen3 | scaffolding, DB, HTTP catalogs, Svelte UI, Scoop/Choco, axum server |

The maintainer owns most `tier:max` issues (they hold a Max/Ultra plan and treat this as a slow
project). The `tier:any` issues are exactly where your spare tokens are worth the most.

## Prompting the model correctly

- Paste the issue **verbatim**; do not summarize away file paths or acceptance criteria.
- Include a one-line directive: _"Implement exactly per SPEC.md. No payment, account, or
  licensing logic. Stay within the public interfaces named. Write tests. Output code only."_
- Work incrementally — one file or one behavior at a time, compiling between steps.
- If the model proposes anything outside the SPEC scope (payment, auth, protection-related),
  drop that part and re-prompt. It is out of scope by design.

## What counts as a finished PR

- Issue linked; all acceptance criteria ticked.
- `cargo test`, `cargo clippy`, and `cargo fmt` pass.
- Windows behavior verified (your machine, or CI, with a screenshot/log for UI/executor work).
- Guardrail checklist (`docs/SPEC.md` §9) ticked.

## Cutting token cost (so your spare usage goes further)

- **Cache read ≈ 10× cheaper:** re-send `SPEC.md` §0 + the issue in every message; nearly all
  your context is then served from cache, not billed at full input price.
- **`tier:any` needs no big model:** DeepSeek V3.2 ($0.26 in / $0.38 out per M) or Gemini Flash
  is usually plenty.
- **Batch endpoints** (e.g. OpenRouter `:batch`) cut input ~50% for non-interactive passes
  (refactors, test generation, doc edits).

## If you're the maintainer (max-tier subscriber)

- Drive the `tier:max` issues with your Ultra/Max plan (winget parser, msstore adapter,
  Deployment interop, elevation).
- Use Cursor pointed at this repo for the interactive, compile/fix loop.
- Ruthlessly split work into `tier:any` issues so the community can fill in around you.
- Track rough token spend in PR comments (optional) to show the project's true, shared cost.