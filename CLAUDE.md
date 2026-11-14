# Squirrel

Self-hosted home drive: Rust backend (`crates/`), Svelte web UI (`web/`), Kotlin Multiplatform phone app (`mobile/`, later). Design: `docs/plans/2026-11-13-squirrel-home-drive-design.md`. Phase plans: `docs/plans/`.

## Visual design is governed by MASTER.md

- `MASTER.md` (repo root) is the validated design system and its two theses. It is the single source of truth for every visual decision: color, type, spacing, radii, depth, motion, components.
- Web: use only the custom properties in `web/src/lib/styles/tokens.css` (which implements MASTER.md) and the components in `web/src/lib/components/ui/`. No literal colors, sizes, shadows or durations in components.
- Native apps (Compose / SwiftUI): derive `Theme.kt` / `Color+App.swift` etc. from MASTER.md, never invent values.
- UI code snippets in plans written before 2026-11-14 (Phase 1, Phase 2 milestones A–E) predate the design system: follow MASTER.md, not those snippets.
- To change the design, change MASTER.md first (and re-validate with the user), then the tokens.

## Local environment

- Dev Postgres: `docker compose -f deploy/docker-compose.dev.yml up -d --wait`, on localhost:55432. Never use localhost:5433 (an unrelated database proxy).
- Node 22 via nvm: prefix web commands with `source ~/.nvm/nvm.sh && nvm use 22 >/dev/null &&`.
- clippy: `rtk proxy cargo clippy --workspace --all-targets -- -D warnings`.
- E2E: `SQUIRREL_E2E_PORT=8199 pnpm test:e2e` (8099 is taken locally).
- Migrations already applied anywhere are frozen: add new migration files instead of editing.
