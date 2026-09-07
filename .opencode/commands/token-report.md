---
description: Reporta uso de tokens y coste por modelo/herramienta.
agent: build
---

Usa `skill({name:"token-optimizer"})`:

```bash
opencode stats --days 7 --models --tools
```

Resume top gastadores, propone routing a small_model y qué podar. Alerta si 2x baseline/día.
