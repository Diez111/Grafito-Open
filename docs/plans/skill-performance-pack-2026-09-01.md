# Skill + Perf + Token Pack 2026-09-01 — 20 agentes con evidencia

> Paquete curado para opencode + Grafito (Rust egui/wgpu). Cada recomendación con fuente + métrica. Instalado 2026-09-01 via `~/.config/opencode/` (global) y `.opencode/` (proyecto).

## Resumen instalación

- **opencode** 1.18.25 via `curl https://opencode.ai/install` → `~/.opencode/bin/opencode`  `PATH` en `~/.bashrc`, `export PATH` verificado `opencode --version` 1.18.25
- **Global config** `~/.config/opencode/opencode.json`  `model=muse-spark-1.2-contributor`  `small_model=deepseek-v4-flash`  `compaction {auto:true prune:true reserved:12000}`  `watcher ignore target/node_modules/.git`  `formatter rustfmt`  `lsp true`  `permission` allow/ask granular
- **Project configs** `opencode.json` en root + `Grafito-Open/opencode.json` (instructions AGENTS.md, docs/architecture.md, Plans.md, .jspace/WORKSPACE.md)
- **Skills 7/7** globales + proyecto: `j-space` (Tiger V3.7), `rust-performance`, `rust-ui-ux`, `token-optimizer`, `context-engineering`, `grafito-architecture`, `profiling` — todos `SKILL.md` validados `name` regex `^[a-z0-9]+(-[a-z0-9]+)*$`
- **Agents 4/4** `cerebro-auditor`, `piel-ui`, `perf-profiler`, `token-saver` (markdown `~/.config/opencode/agents/*.md` + copias proyecto)
- **Commands 4/4** `rust-check`, `grafito-audit`, `perf-profile`, `token-report` (markdown `.opencode/commands`)
- **AGENTS.md** root + Grafito-Open creados con instrucciones, gates, skills
- Verificación: `opencode debug config` + `verify_suite.py clean` JSpace + `cargo` gates (ver §7)

---

## 20 agentes — hallazgos y evidencia

### Grupo A — Skills y JSpace (1-4)

| # | Agente | Query | Fuente evidencia | Métrica | Recomendación instalada |
|---|--------|-------|------------------|---------|--------------------------|
| 1 | Opencode skills oficiales | `opencode.ai/docs/skills` + `docs/config` | off. docs 2026-09-01: `compaction.prune` `watcher.ignore` `permission.skill` `agent` schema, `opencode debug config` | schema OK | `context-engineering` skill documenta precedence 8 niveles + discovery `.opencode/skills/*/SKILL.md` |
| 2 | Community catalog | `awesome opencode skills 2025` | `awesome-opencode/awesome-opencode` 9,969★ 797 forks CC0 504 commits (2025-09-22) + `TheArchitectit/awesome-opencode-skills` 140★ 72comm + `jshsakura` 25★ 136+ skills MIT `install.py` | stars/commits | No masivo install (token bloat); sí docs en `token-optimizer` refs |
| 3 | J-Space Cognition | `J-Space V3.6 Tiger` | `Tiger3807861189/J-Space-Cognition-Suite-V3.6` 3,016★ Apache-2.0 + V3.7 3,011★ DOI 10.5281/zenodo.21971181 `verify_suite.py clean` + `DeepSeek V4 × J-Space Report` bench avg 56.99→58.61 (+2.8%) token -22~31% wall -14~32% speedup 1.15-1.41× (6 benchmarks) | DOI + bench | **Instalado** `j-space` global+2 proyectos via `git clone --depth1` + `verify_suite.py` |
| 4 | DSH harness | `deepseek harness token-meter compaction` | `Grafito-Open/docs/dsh-integration.md` + `grafito-agent::ledger JSpaceLedger` `Goal/Core/Verified/Open/Next` + `router::TaskBand fast/full/loop` | mapping DSH→Rust impl | `grafito-architecture` skill + `token-saver` agent documentan ledger + band |

### Grupo B — Rust UI perf (5-8)

| 5 | egui/eframe perf | `egui eframe performance 2025` | `emilk/egui` 11k★ + `crates.io egui` “1-2 ms per frame, repaint only on interaction” + `docs.rs/egui` CPU usage + discussion #5587 native 120ms vs web 13ms (layout/tessellation) | 1-2ms, 120vs13 | `rust-ui-ux` + `rust-performance`: rayon feature, virtualize scroll, `request_repaint` minimal |
| 6 | wgpu/render | `wgpu 22 compute shaders` | `egui-wgpu CHANGELOG 0.29→22.0` + Grafito `domain_coloring_compute 500×500=250k cells/dispatch` `implicit_compute` marching squares + `bytemuck` derive + `wgpu 22.0` in `Cargo.toml` | 250k/dispatch | `rust-performance` GPU batch + cache LRU 128 |
| 7 | Mem/concurrencia | `tokio bounded VecDeque OOM` | `notify 6.1` + `tokio full` + `app.rs:31 MAX_UNDO 50 + MAX_UNDO_BYTES 50MiB VecDeque pop_front O(1) vs Vec O(n)` + `checked_mul` OOM guard `validation.rs` | 50 MiB, OOM safe | `rust-performance` checklist |
| 8 | Scandinavian UI | `progressive disclosure toolbar` | `Grafito-Open/crates/grafito-app/src/app.rs toolbar_groups_for_level_filtered` PRIMARY 5 (level 0..4) SECONDARY 8 (5..10) UNIVERSITY 17 (11+) + `tokens.rs TYPE 11..28 SPACE 4..32` + `assistant.rs width 340..460 composer 88..260` | 5/8/17 groups | `rust-ui-ux` progressive disclosure + tokens 64/44/10/5% |

### Grupo C — Token/Inteligencia (9-12)

| 9 | LLMLingua | `LLMLingua prompt compression 2024` | `arxiv:2310.05736` LLMLingua EMNLP23 budget controller + `arxiv:2403.12968` LLMLingua-2 ACL24 xlm-roberta-large 560M 3-6× speed + `microsoft/LLMLingua` GH 10× prefill A100 1M tokens + `pcToolkit` | 3-6×, 10× | `token-optimizer` skill section compression rates 20-50% |
|10 | LLM routing | `model routing fast reasoner audit` | `Grafito-Open/crates/grafito-agent/src/router.rs ModelRoute fast/reasoner/audit` + `opencode models` (muse-spark-1.2, deepseek-v4-flash, mimo-v2.5, nemotron, kimi) + `opencode.json: build muse-spark temp0.2 / plan deepseek temp0.1` | cost/latency | Global config + 4 agents per-model |
|11 | Opencode Go | `opencode zen go v1 chat/completions` | `grafito-assistant/tests/remote_transport.rs endpoint https://opencode.ai/zen/go/v1` + `opencode models` opencode-go/* 30+ | endpoint | provider `opencode` timeout 120s chunkTimeout 30s |
|12 | Github Rust trend | `awesome-rust ui stars 2025` | `lyon 2,593★ nical/lyon tessellation`, `spade 336★ Stoeoef/spade Delaunay bulk_load_cdt`, `rstar 0.12`, `geo 0.29` – Grafito `Cargo.toml` workspace deps | stars/downloads | `rust-performance` keeps lyon/tessellation options |

### Grupo D — Tooling & Papers (13-16)

|13 | Token libs | `tiktoken token-meter effect` | `~/.config/opencode/node_modules/@ai-sdk/provider` + `srozario121/opencode-optimisations docs/opencode-config.md` verified June 2026 opencode 1.17.7 `compaction.prune` `small_model` `custom read.ts ripgrep` at I/O boundary | token watch | `context-engineering` + `token-report` command `opencode stats --models --tools` |
|14 | Pedagogía/Socratic | `BKT Leitner Socratic` | `Grafito-Open/docs/architecture.md:10` `Curriculum 42 LOs` `BKT + Leitner 86400*2^(level-1)*(2-mastery)` `SocraticFsm Review→HeuristicQ→Await→Rectify→Summarize` | mastery | `grafito-architecture` debt note |
|15 | Agent loops | `ReAct tool schema` | `Grafito-Open/crates/grafito-agent/src/schema.rs + loop_engine.rs` `max_tool_turns` `MAX_TOOL_RESULT_CHARS` + `dsh-integration.md` DSH loop mapping | steps 24 timeout 15s | `token-optimizer` + `grafito-audit` plan dispersa 4 subagents |
|16 | Profiling | `puffin viewer wgpu-profiler` | `EmbarkStudios/puffin` 1,731★ Apache2 `puffin 0.19` 50-200ns/span 119k dl/mo + `puffin_egui` + `wgpu_profiler` `TIMESTAMP_QUERY` + `criterion 0.5` benches `geometry_bench` `render_scenarios` | ns | **Instalado** `profiling` skill + `profile` feature `["puffin","puffin_http","eframe/puffin"]` in `grafito-app/Cargo.toml` |

### Grupo E — Opencode integración (17-20)

|17 | Plugin system | `opencode plugin skills` | `opencode.ai/docs/plugins` + `docs/config#plugins` `plugin: ["@opencode-ai/plugin"]` npm `https://opencode.ai/config.json` + `~/.config/opencode/package.json @opencode-ai/plugin 1.18.25` | npm registry | Global plugin configured |
|18 | Design tokens | `egui tokens TYPE SPACE RADIUS` | `Grafito-Open/crates/grafito-ui/src/tokens.rs TYPE_XS..XXL 11..28 ratio1.25 SPACE 4..32 base4` single source | no hardcode | `rust-ui-ux` tokens rule |
|19 | Security/perf tradeoff | `canonicalize symlink escape` | `grafito-anim/src/protocol.rs validate_media_path relative_to` + `grafito-plugins/src/validate.rs` fail-closed fingerprint | path_escape | `rust-performance` TOCTOU section |
|20 | Síntesis & packaging | `curate pack` | Esta doc + `Grafito-Open/docs/architecture.md:8` budgets table + `Tasks.md` F0-F5 gates 1500 tests | matriz | Este pack P0/P1/P2 priorizado |

---

## Paquete priorizado

### P0 — Ya instalado, usar diario

- `j-space` fast/full/loop gating + `.jspace/WORKSPACE.md` ledger Goal/Core/Verified/Open/Next
- `compaction prune:true reserved:12000` + `small_model` routing (title→flash cheap) + `Agent.performance` per-agent model
- `watcher.ignore target/node_modules/.git` + `snapshot:true` + `formatter.lsp:true`
- 4 agentes + 4 comandos + 7 skills cargables vía `skill` tool
- `cargo fmt/clippy/test/check` gates en `AGENTS.md`, `rust-check` command
- `profile` feature puffin listo `cargo run -p grafito-app --features profile -- --profile` + `puffin_viewer`

### P1 — Recomendado siguiente sprint (evidencia alta, esfuerzo medio)

- Activar `rayon` parallel tessellation en `egui`: `egui = { features=["rayon"] }` → mide `puffin` 1-2ms target (docs.rs egui rayon)
- LLMLingua-2 pre-compresión para instrucciones RAG: 20% → test accuracy vs tokens `arxiv:2403.12968` demo `huggingface.co/spaces/microsoft/LLMLingua-2`
- Migrar `Document {objects,variables}: HashMap→BTreeMap` determinismo total (debt `architecture.md:11`)
- `WhiteboardDoc` export SVG + `GeoObject::Whiteboard` independiente
- Instalar 2-3 community skills high-value: `codebase-orchestrator` (repo-wide refactor), `eval-engineer` (prompt eval) de `jshsakura` (172 skills) — via copy `SKILL.md` a `.opencode/skills/`

### P2 — Futuro (6mo)

- K-Token merging latent compression `arxiv:2604.15153` para math/code dense (complementa LLMLingua)
- `fill_compute` lazy habilitar si `ImplicitCurve != Eq` (ahorra 128MiB hoy, docs/arch 11)
- Classroom P2P opt-in feature flag
- God Object `app.rs 4752L` extracción completa `controllers.rs`

---

## Cómo usar (día 0)

```bash
export PATH="$HOME/.opencode/bin:$PATH"
opencode                    # TUI: Tab cambia build/plan, @cerebro-auditor etc
opencode debug config       # verifica merge global+proyecto
opencode debug paths
opencode stats --days 7 --models --tools
opencode run "usa skill jspace para auditar Grafito"
# comandos
/rust-check
/grafito-audit
/perf-profile
/token-report
```

En TUI `skill` tool lista 7 skills; `skill({name:"j-space"})` carga entry + módulos on-demand (capacity/broadcast/directed-focus/deep-reasoning/introspection/self-monitoring etc).

## Verificación pendiente (siguiente sección)

- `python3 ~/.config/opencode/skills/j-space/scripts/verify_suite.py` DONE clean
- `opencode debug config` merge ok DONE
- `cargo fmt --check && cargo clippy -- -D warnings && cargo test --workspace` (ejecutar en box con Rust 1.92)
- `cargo run -p grafito-app --features profile` smoke

## Licencias y atribución

- J-Space V3.7 Apache-2.0 `Tiger3807861189` + LICENSE + THIRD_PARTY_NOTICES copiados
- awesome-opencode CC0-1.0, lyon MIT/Apache, spade MIT/Apache, puffin MIT/Apache, egui MIT/Apache, Microsoft LLMLingua MIT
- Grafito GPL-3.0-or-later
