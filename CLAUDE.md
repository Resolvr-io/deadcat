# Claude Code Instructions

Follow all instructions in [AGENTS.md](AGENTS.md) — especially the mandatory CI checks.

## Additional Claude-Specific Rules

- Do NOT add Claude as co-author or contributor in any commits or git metadata
- Do NOT commit or push without explicit user permission
- Do NOT push to master/main without explicit permission
- `.DS_Store` files are always untracked and gitignored — never stage or commit them
- Do NOT use nostr.band (defunct)
- Use `npx @biomejs/biome` (NOT `npx biome` which resolves to old 0.3.3)
- Rust MSRV is 1.77.2 — do NOT use `std::sync::LazyLock` (requires 1.80). Use `once_cell::sync::Lazy`
- Use `src/utils-react/friendly-error.ts` `friendlyError()` for all user-facing errors — never show raw SDK error strings
