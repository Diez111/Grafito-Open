# Grafito Fuzz Targets

This directory is a standalone `cargo-fuzz` package. Its local `[workspace]`
section and committed `Cargo.lock` keep fuzz dependency resolution separate
from Grafito's root workspace and root lockfile.

The targets cover four untrusted-input boundaries:

- `ast_parser`: native mathematical AST parsing.
- `complex_parser`: complex-expression parsing.
- `document_deserialization`: validated persisted-document loading.
- `command_atomicity`: rejected commands must leave a document unchanged.

## Running locally

`cargo-fuzz` requires a Rust nightly toolchain and libFuzzer support. The
fuzz-specific `rust-toolchain.toml` pins `nightly-2025-02-15`; CI also pins
`cargo-fuzz` to `0.12.0`. Install that tool outside this repository, then run
a bounded target from the repository root:

```sh
rustup toolchain install nightly-2025-02-15 --profile minimal --component rust-src
cargo +nightly-2025-02-15 install --locked cargo-fuzz --version 0.12.0
cargo +nightly-2025-02-15 fetch --locked --manifest-path fuzz/Cargo.toml

for target in ast_parser complex_parser document_deserialization command_atomicity; do
  CARGO_NET_OFFLINE=true cargo +nightly-2025-02-15 fuzz run "$target" \
    --fuzz-dir fuzz -- -max_total_time=180 -max_len=65536
done
```

The targets independently cap decoded input at 64 KiB, matching the command
and AST parser limits. `cargo-fuzz` creates corpus and crash artifacts under
`fuzz/` during local runs.

## Continuous Fuzzing

`.github/workflows/fuzz.yml` runs every Sunday at 03:17 UTC and on manual
dispatch. It runs all four targets above for at most 180 seconds each, so
libFuzzer execution is capped at 12 minutes per workflow run; the GitHub job
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
cargo +nightly-2025-02-15 generate-lockfile --manifest-path fuzz/Cargo.toml
cargo +nightly-2025-02-15 metadata --locked --manifest-path fuzz/Cargo.toml --format-version 1 --no-deps
```

Review the resulting `fuzz/Cargo.lock`; do not update the root `Cargo.lock` as
part of this operation.
