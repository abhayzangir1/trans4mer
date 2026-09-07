# Contributing

## Ground rules

1. **No direct state mutation** — every runtime mutation goes through `commit_and_emit` (event + projection, one transaction). Recovery/shutdown corrections are the only sanctioned direct writes.
2. **Providers stay stateless** — no storage imports in `trans4mers-providers`. Providers return metrics; the engine persists.
3. **No mocks in production code** — test doubles live only under `#[cfg(test)]` / the integration crate's `support` module.
4. **Exhaustive projections** — `apply_projection` has no `_` arm; new `DomainEvent` variants must add their projection in the same PR.
5. **EffectClass-aware retries** — never auto-retry `NonIdempotentMutation`/`Unknown` tools.

## Development loop

```bash
cargo test --workspace --exclude trans4mers-desktop   # core + integration (fast, no LLM)
cd apps/desktop/frontend && npx tsc -b && npx vitest run
cd apps/desktop/src-tauri && cargo tauri dev          # full app (needs Ollama for agents)
```

## Before opening a PR

- `cargo test --workspace --exclude trans4mers-desktop` green
- `cargo clippy --workspace --exclude trans4mers-desktop -- -D warnings` clean
- `npx tsc -b` clean in the frontend
- New events have projections; new commands are registered in `generate_handler!`
