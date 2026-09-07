# Grafito Fuzz Targets

This directory is a standalone `cargo-fuzz` package. Its local `[workspace]`
section and committed `Cargo.lock` keep fuzz dependency resolution separate
from Grafito's root workspace and root lockfile.

The targets cover seven untrusted-input boundaries:

- `ast_parser`: native mathematical AST parsing.
- `complex_parser`: complex-expression parsing.
- `document_deserialization`: validated persisted-document loading.
- `command_atomicity`: rejected commands must leave a document unchanged.
- `validation_limits`: document and expression limits (10M, 5000 objects, 2000 expr).
- `spreadsheet_recompute`: spreadsheet formula recomputation.
- `anim_wire`: animation IPC wire JSON v1 + expression mirror (`protocol.rs` + `__main__.py`; line_cap 64 KiB, `percent` 0..=100, `SAFE_FUNCS`, `MAX_NODES` 200, `MAX_EXPR_LEN` 500, dunder `__` rejected). Seed corpus: `fuzz/corpus/anim_wire/` (9 vectors, including `z^2`, `exp(x)`, `__import__('os')`, hello/progress/result/error v1, 66 KiB oversize).

## Running locally

`cargo-fuzz` requires a Rust nightly toolchain and libFuzzer support. The
fuzz-specific `rust-toolchain.toml` pins `nightly-2026-08-01`; CI also pins
`cargo-fuzz` to `0.12.0`. Install that tool outside this repository, then run
a bounded target from the repository root:

```sh
rustup toolchain install nightly-2026-08-01 --profile minimal --component rust-src
cargo +nightly-2026-08-01 install --locked cargo-fuzz --version 0.12.0
cargo +nightly-2026-08-01 fetch --locked --manifest-path fuzz/Cargo.toml

for target in ast_parser complex_parser document_deserialization command_atomicity validation_limits spreadsheet_recompute anim_wire; do
  CARGO_NET_OFFLINE=true cargo +nightly-2026-08-01 fuzz run "$target" \
    --fuzz-dir fuzz -- -max_total_time=180 -max_len=65536
done
```

The targets independently cap decoded input at 64 KiB, matching the command
and AST parser limits. `cargo-fuzz` creates corpus and crash artifacts under
`fuzz/` during local runs. Seed corpus is committed under `fuzz/corpus/<target>/`
(9 vectors for `anim_wire`, 1–2 for the others); `fuzz/artifacts/` remains
gitignored.

## Continuous Fuzzing

`.github/workflows/fuzz.yml` runs every Sunday at 03:17 UTC and on manual
dispatch. It runs all seven targets above for at most 180 seconds each, so
libFuzzer execution is capped at 21 minutes per workflow run; the GitHub job
has a 30-minute timeout. The workflow uses `fuzz/Cargo.lock`, verifies it with
`cargo metadata --locked`, and uploads any files under `fuzz/artifacts/` for 14
days when a crash is found.

Fuzzing is deliberately excluded from the normal pull-request CI workflow, so
PR checks remain bounded and do not depend on installing a nightly fuzzing
toolchain. This coverage can find crashes and invariant violations, but a clean
run does not establish the absence of defects or security vulnerabilities.

## Updating Fuzz Dependencies

Update fuzz dependencies intentionally and independently of the root workspace:

```sh
cargo +nightly-2026-08-01 generate-lockfile --manifest-path fuzz/Cargo.toml
cargo +nightly-2026-08-01 metadata --locked --manifest-path fuzz/Cargo.toml --format-version 1 --no-deps
```

Review the resulting `fuzz/Cargo.lock`; do not update the root `Cargo.lock` as
part of this operation.

## Findings

No reproducible crashes found via `cargo fuzz` as of 2026-09-04 (weekly CI on
`fuzz.yml`, all seven targets at `-max_total_time=180`, corpus
`fuzz/corpus/<target>/` seeded as above). Crash artifacts, if any, are uploaded
as `fuzz-crashes-*` for 14 days and triaged via `fuzz/corpus/<target>/` + unit
tests (e.g. `crates/grafito-anim/tests/anim_wire_fuzz_corpus.rs`). Pending:
expand `anim_wire` corpus with real worker traces once nightly CI reports
coverage.
