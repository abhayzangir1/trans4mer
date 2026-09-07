# Contributing

## Ground rules

1. **No direct state mutation** — every runtime mutation goes through `commit_and_emit` (event + projection, one transaction). Recovery/shutdown corrections are the only sanctioned direct writes.
2. **Providers stay stateless** — no storage imports in `trans4mers-providers`. Providers return metrics; the engine persists.
3. **No mocks in production code** — test doubles live only under `#[cfg(test)]` / the integration crate's `support` module.
4. **Exhaustive projections** — `apply_projection` has no `_` arm; new `DomainEvent` variants must add their projection in the same PR.
5. **EffectClass-aware retries** — never auto-retry `NonIdempotentMutation`/`Unknown` tools.

## Development loop

```bash
cargo test --workspace --exclude trans4mers-desktop -j 2 # run all unit and integration tests
cargo check -p trans4mers-domain -p trans4mers-storage -p trans4mers-engine -p trans4mers-providers -p trans4mers-app
cd apps/desktop && npm run build                         # typecheck and compile frontend bundle
cargo tauri dev                                          # run full desktop app (requires Ollama)
```

## Before opening a PR

- `cargo test --workspace --exclude trans4mers-desktop -j 2` green
- `cargo check --workspace` clean
- `npm run build` in `apps/desktop` clean (0 TypeScript / bundling errors)
- New events have projections; new commands are registered in `generate_handler!`
