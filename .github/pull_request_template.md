## What and why

<!-- Briefly: what changes and why. -->

## Before the code (CONTRIBUTING.md, "Feature workflow")

- **Where:** <!-- layer, crate/folder, which interface -->
- **How:** <!-- the simplest way within the contract -->
- **Does it belong there:** <!-- is a type getting a new role -->
- **What already exists:** <!-- what was reused -->

## Checks

- [ ] `cargo fmt --all --check`, `cargo test --workspace`, `cargo clippy --workspace --all-targets -- -D warnings`
- [ ] in `app/`: `npx tsc --noEmit`, `npm run lint`, `npx vite build`
- [ ] test plugins changed — `scripts/build-test-plugins.sh` (tests, clippy, bundles of `test-plugins/*`)
- [ ] there are contract tests
- [ ] contract changed — stated above and explained
- [ ] new errors are registered codes with a Russian translation in `app/src/i18n/errors/ru.ts`
- [ ] the doc of the changed area or feature updated (its folder's `CLAUDE.md` or `docs/features/`; the map `ARCHITECTURE.md` if a crate, area, feature doc or recipe appeared)
- [ ] dependencies changed — `python3 scripts/third_party_notices.py --check` passes
