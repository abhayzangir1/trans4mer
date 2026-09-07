## Description

Please include a summary of the change and which issue it fixes. Describe the motivation, architectural design, and impact.

Fixes # (issue)

## Type of change

- [ ] Bug fix (non-breaking change which fixes an issue)
- [ ] New feature (non-breaking change which adds capability)
- [ ] Breaking change (fix or feature that would cause existing functionality to not work as expected)
- [ ] Performance optimization / Refactor
- [ ] Documentation update

## Verification Checklist

In accordance with strict verification standards, please confirm all checks pass before submitting:

- [ ] Physical execution verification performed line-by-line (no mock stubs or fake facades)
- [ ] SQLite migrations verified (if schema changed, both up-migration and repository queries match 1:1)
- [ ] cargo check -p trans4mers-domain -p trans4mers-storage -p trans4mers-engine -p trans4mers-providers -p trans4mers-app passes cleanly
- [ ] cargo test --workspace --exclude trans4mers-desktop passes with zero failures
- [ ] cd apps/desktop && npm run build (	sc && vite build) passes with zero errors
- [ ] No personal tokens, private paths, or sensitive keys committed
