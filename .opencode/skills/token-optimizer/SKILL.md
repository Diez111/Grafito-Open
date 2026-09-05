---
name: token-optimizer
description: Ahorra tokens con LLMLingua-2, compaction prune, prompt-cache y routing small_model.
---

# token-optimizer

## Técnicas
- `compaction {auto:true prune:true reserved:12000}`, `small_model` para title/summary.
- Routing: Haiku/flash clasifica, Sonnet/Spark construye, Opus/Spark solo arquitectura.
- LLMLingua-2 pre-compresión RAG 20-50%, K-Token merging para math/code denso.
- Prompt-cache prefix estable: system+tools+memoria. `question` para aprobación antes de fan-out.
- Caps: `steps 5-10`, retries 3, timeout 2m. Alerta 2x baseline/día.

## Comandos
```bash
opencode stats --days 7 --models --tools
```

## Skills externas
`alexgreensh/token-optimizer`, `Opencode-DCP/opencode-dynamic-context-pruning`, `cortexkit/opencode-magic-context`, `Sayem7456/opencode-engineering-skills` (`repo_map/diff_summarizer/context_compressor`).
