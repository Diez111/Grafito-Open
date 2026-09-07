---
description: Perfila con puffin/criterion antes de optimizar.
agent: build
---

Usa `skill({name:"profiling"})`:

```bash
cargo run -p grafito-app --features profile -- --profile
cargo bench -p grafito-geometry
cargo bench -p grafito-render
```

Reporta ms/frame, cells/dispatch, regresión >10% = FAIL. Prohíbe optimizar sin número previo.
