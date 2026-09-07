#!/usr/bin/env bash
# install-skills.sh — instala TODAS las skills por grupos, la IA elige según proyecto.
# Uso:
#   bash scripts/install-skills.sh --list
#   bash scripts/install-skills.sh --group rust|ui|memory|orchestration|mcp|all
#   bash scripts/install-skills.sh --group ui --yes   (sin confirmar)
set -euo pipefail
YES=0
GROUP=""

while [[ $# -gt 0 ]]; do
  case "$1" in
    --list) GROUP="list"; shift ;;
    --group) GROUP="${2:-}"; shift 2 ;;
    --all) GROUP="all"; shift ;;
    --yes|-y) YES=1; shift ;;
    *) echo "flag desconocida: $1"; exit 1 ;;
  esac
done

add() { # add <repo> <skill>
  echo "→ npx skills add $1 --skill $2 -a opencode"
  if [[ "$YES" == "1" ]]; then npx skills add "$1" --skill "$2" -a opencode || true; fi
}

group_rust() {
  add https://github.com/apollographql/skills rust-best-practices
  add leonardomso/rust-skills rust-skills
  add https://github.com/anton-shomin/agents-skills rust-pro
  add https://github.com/oakoss/agent-skills rust
  add melonask/axum-skills axum-skills
  add https://github.com/bwbioinfo/skill_dioxus dioxus-project-setup
  add https://github.com/TerminalSkills/skills leptos
  add https://github.com/dchuk/claude-code-tauri-skills integrating-tauri-rust-frontends
  add https://github.com/Mte90/dotfiles ratatui
  add https://github.com/GhostCodeByte/FastGTrack slint-android
  echo "# webgpu/Rust UI: cazala/webgpu-skill (manual: npx skills add cazala/webgpu-skill)"
  echo "# egui premium: Zuytan/rustrade .agent/skills/ui-design (copiar a .opencode/skills/)"
}

group_ui() {
  add hueyexe/frontend-agent-skills accessibility-inclusive-design
  add hueyexe/frontend-agent-skills design-systems-frontend-architecture
  add hueyexe/frontend-agent-skills forms-inputs-checkout
  add hueyexe/frontend-agent-skills information-architecture-navigation
  add hueyexe/frontend-agent-skills interaction-patterns-components
  add hueyexe/frontend-agent-skills ui-visual-composition
  add hueyexe/frontend-agent-skills ux-usability-foundations
  add hueyexe/frontend-agent-skills ux-writing-content-design
  add https://github.com/anthropics/skills frontend-design
  add vercel-labs/agent-skills web-design-guidelines
  add ehmo/platform-design-skills web
  add https://github.com/Leonxlnx/taste-skill design-taste-frontend
  add julianoczkowski/designer-skills design-tokens
  add julianoczkowski/designer-skills frontend-design
  add https://github.com/christopherlouet/wcag-audit wcag-audit
  add https://github.com/content-designer/ux-writing-skill ux-writing
  add https://github.com/solinkz/micro-interactions-skill micro-interactions-skill
  add https://github.com/nextlevelbuilder/ui-ux-pro-max-skill ui-ux-pro-max
  echo "# ibelick/ui-skills: npx ui-skills start && npx ui-skills get <skill>"
  echo "# figma: npx skills add https://github.com/openai/skills --skill figma-implement-design"
}

group_memory() {
  echo "# plugin base (una vez): npm i -g opencode-mem || opencode plugin add opencode-mem"
  echo "#   opencode-mem: SQLite+FTS5+USearch local, WebUI :4747"
  echo "# auto-memoria: https://github.com/daniloaguiarbr/opencode-auto-memory (dual-write Serena+MEMORY.md)"
  echo "# grafo local: uvx kuzu-memory / uvx cognee-mcp / npx @letta-ai/memory-mcp"
  echo "# RAG codebase: https://github.com/DeusData/codebase-memory-mcp (14 tools grafo código)"
  echo "# SaaS híbrido solo no-PII: https://beta.memory.store/mcp (sucesor Julep)"
}

group_orchestration() {
  echo "# elige 1 orquestador según tamaño:"
  echo "#   crítico multi-sesión: https://github.com/code-yeongyu/oh-my-openagent (Sisyphus+Prometheus+Metis, /start-work)"
  echo "#   2-3 slices: https://github.com/hueyexe/opencode-ensemble (worktrees+dashboard :4747)"
  echo "#   ligero: https://github.com/moinulmoin/opencode-arise (Monarch token-efficient)"
  echo "#   research largo: https://github.com/kdcokenny/opencode-background-agents (delegate persistente)"
  echo "#   fan-out con conflictos: https://github.com/AutomatorAlex/opencode-background-tasks (bg_task+reconcile)"
  echo "# calidad: tdd-workflow (PedroHBO/composio/FrancoStino), reviewer (staff-engineer-review), debugger, git-release (docs oficial)"
}

group_mcp() {
  echo "# MCP mínimos ya en opencode.json (filesystem/git/fetch/memory/sequential/context7)."
  echo "# full bajo demanda:"
  echo "#   dev: github/github-mcp-server (Go oficial), rust-analyzer-mcp, oraios/serena"
  echo "#   UI: mcp.figma.com/mcp, storybook addon-mcp, microsoft/playwright-mcp"
  echo "#   memoria: cognee-mcp, kuzu-memory, corporatepiyush/mcp-memory (Rust 1 binario)"
  echo "#   infra: bytebase/dbhub (no server-postgres archivado), awslabs/mcp, ckreiling/mcp-server-docker, Flux159/mcp-server-kubernetes"
  echo "#   web: exa-labs/exa-mcp-server (solo en subagente)"
  echo "# registries: registry.modelcontextprotocol.io, glama.ai/mcp (grades A-F), smithery.ai, mcp.so"
}

case "$GROUP" in
  list) echo "grupos: rust ui memory orchestration mcp all"; ;;
  rust) group_rust ;;
  ui) group_ui ;;
  memory) group_memory ;;
  orchestration) group_orchestration ;;
  mcp) group_mcp ;;
  all) group_rust; group_ui; group_memory; group_orchestration; group_mcp ;;
  *) echo "uso: bash scripts/install-skills.sh --list | --group <rust|ui|memory|orchestration|mcp|all> [--yes]"; exit 1 ;;
esac
