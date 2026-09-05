---
description: Run fmt, clippy, test y check de todo el workspace Rust.
agent: build
---

Ejecuta en orden y reporta el primer fallo con log recortado:

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace --locked
cargo check --workspace --locked
```

MSRV 1.92. Si `fmt` falla, indica `cargo fmt --all`. No propongas fixes sin evidencia del log.
