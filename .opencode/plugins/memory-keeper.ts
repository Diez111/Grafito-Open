// MemoryKeeper — presupuesto de memoria + inyección anti-amnesia en compaction.
//
// - `session.idle`: controla que MEMORY.md no exceda el presupuesto (60 líneas).
//   Si excede, logea un aviso para invocar al agente `memory-keeper` (único escritor).
// - `experimental.session.compacting`: inyecta el índice de memoria curada
//   (decisiones MEMORY.md + Next de .jspace) en el contexto de compaction.
// Sin LLM, sin red, solo lectura de ficheros + logs. Linux-safe (sin osascript).

export const MemoryKeeper = async ({ client, $, directory }) => {
  const memFile = `${directory}/MEMORY.md`;
  const jspaceFile = `${directory}/.jspace/WORKSPACE.md`;
  const MEM_BUDGET_LINES = 60;

  async function readFirstLines(path, maxLines) {
    try {
      const out = await $`head -n ${String(maxLines)} ${path}`.text();
      return out.trim();
    } catch {
      return "";
    }
  }

  async function countLines(path) {
    try {
      const out = await $`wc -l < ${path}`.text();
      return parseInt(out.trim(), 10) || 0;
    } catch {
      return 0;
    }
  }

  return {
    event: async ({ event }) => {
      if (event.type === "session.idle") {
        const lines = await countLines(memFile);
        if (lines > MEM_BUDGET_LINES) {
          await client.app.log({
            body: {
              service: "memory-keeper",
              level: "warn",
              message: `MEMORY.md tiene ${lines} líneas (presupuesto ${MEM_BUDGET_LINES}). Invoca al agente memory-keeper para curar (dedup/TTL).`,
            },
          });
        }
      }
    },

    "experimental.session.compacting": async (input, output) => {
      const memory = await readFirstLines(memFile, MEM_BUDGET_LINES);
      const workspace = await readFirstLines(jspaceFile, 40);
      const index = [
        "## Memoria curada del harness (inyectada por memory-keeper)",
        memory ? `### MEMORY.md\n${memory}` : "### MEMORY.md\n(vacía)",
        workspace ? `### .jspace/WORKSPACE.md (inicio)\n${workspace}` : "",
        "Regla: al resumir, preserva Decisiones con fecha y el Next del workspace.",
      ]
        .filter(Boolean)
        .join("\n");
      output.context.push(index);
    },
  };
};
