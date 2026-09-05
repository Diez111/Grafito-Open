---
name: context-engineering
description: Gestiona contexto opencode con precedence, discovery, prune y anti-amnesia en compaction.
---

# context-engineering

## Precedence (8 niveles)
Remote < global `~/.config/opencode/opencode.json` < `OPENCODE_CONFIG` < proyecto `opencode.json` < `.opencode/` dirs < `OPENCODE_CONFIG_CONTENT` < managed `/etc/opencode/` < MDM.

## Discovery skills
`.opencode/skills/*/SKILL.md` > `.claude/` > `.agents/` (walk-up a worktree) + global `~/.config/opencode/skills/`. Valida `name ^[a-z0-9]+(-[a-z0-9]+)*$` == dirname, `description 1-1024`.

## Anti-amnesia
- `session.idle` → dual-write `MEMORY.md` + `.jspace/` + Serena si existe.
- `experimental.session.compacting` solo inyecta índice (sin tools por limitación).
- Sesión caliente `/tmp` + promoción curada a workspace + perfil global `~/.opencode-mem/`.
- Nunca append-only sin dedup/decay.

## Skills externas
`NeoLabHQ/context-engineering-kit`, `composio-community/context-engineering`, `boazcstrike/context-optimization`, `joshuadavidthomas/opencode-agent-skills` (sobrevive compaction).
