# Security Policy

## Supported Versions

| Version | Supported          |
| ------- | ------------------ |
| 1.2.x   | :white_check_mark: |
| < 1.2   | :x:                |

## Reporting a Vulnerability

We take the security of Grafito seriously. If you believe you have found a security vulnerability, please report it to us as described below.

**Please do NOT report security vulnerabilities through public GitHub issues.**

### How to Report

Email us at [diezprocapoxd@gmail.com](mailto:diezprocapoxd@gmail.com) with the following information:

- Description of the vulnerability
- Steps to reproduce the issue
- Potential impact
- Any suggested fixes (if applicable)

### What to Expect

- **Acknowledgment**: We will acknowledge receipt of your vulnerability report within 48 hours.
- **Assessment**: We will assess the vulnerability and determine its impact within 7 days.
- **Resolution**: We will work on a fix and release a patched version as soon as possible.
- **Disclosure**: We will coordinate with you on the disclosure timeline.

### Build and Release Integrity

- CI resolves Cargo dependencies with `--locked`, runs CodeQL, and checks the
  lockfile with `cargo-audit` and the committed `cargo-deny` policy. Tag
  releases invoke that CI workflow directly as `ci-gate`; its formatting,
  all-target clippy, workspace tests, required Vulkan renderer tests,
  audit/deny, Debian build/install smoke test, and workflow/shell lint must
  all succeed before artifact builds or publication can start.
- Release tags must match the Cargo version, have a matching changelog entry,
  and point to a commit reachable from `main` before builds start.
- Cargo versions containing a prerelease suffix (for example `-beta`) are
  published as GitHub prereleases. Stable download links use `releases/latest`
  and therefore do not select those prerelease artifacts.
- Release archives are published with a `SHA256SUMS.txt` manifest. Verify it
  with `sha256sum -c SHA256SUMS.txt` before using an archive.
- The release workflow publishes an SPDX SBOM with every release. The SBOM and
  checksums are not signatures or provenance attestations.
- The release workflow intentionally does not claim to sign artifacts or
  generate provenance attestations until maintainers provision and review the
  necessary protected infrastructure.
- Before enabling signing or provenance publication, maintainers must
  add a separately reviewed protected release step, pin its tool/action by
  immutable digest or commit SHA, grant only the needed job permissions, and
  document the verification command alongside the release.

### Temporary quick-xml advisory exceptions

`RUSTSEC-2026-0194` and `RUSTSEC-2026-0195` are temporary, explicit exceptions
for the locked `quick-xml` 0.30.0 package. They are not a general
advisory bypass.

| Field | Value |
| --- | --- |
| Owner | Grafito maintainers (security contact above) |
| Reviewed | 2026-07-18 |
| CI expiry | 2026-12-31; CI fails on and after this date |
| Removal condition | Every locked quick-xml path is patched at >=0.41.0 or its parent no longer uses quick-xml |

The complete reviewed all-feature ancestor DAG is below. Each edge points from
the dependent to its dependency; CI compares this block and the live Cargo
metadata against the same canonical edge sets.

<!-- reviewed-quick-xml-ancestor-edges:start -->
```text
quick-xml 0.30.0:
accesskit_atspi_common@0.9.3 -> atspi-common@0.6.0
accesskit_unix@0.12.3 -> accesskit_atspi_common@0.9.3
accesskit_unix@0.12.3 -> atspi@0.22.0
accesskit_winit@0.22.4 -> accesskit_unix@0.12.3
atspi-common@0.6.0 -> zbus-lockstep-macros@0.4.4
atspi-common@0.6.0 -> zbus-lockstep@0.4.4
atspi-connection@0.6.0 -> atspi-common@0.6.0
atspi-connection@0.6.0 -> atspi-proxies@0.6.0
atspi-proxies@0.6.0 -> atspi-common@0.6.0
atspi@0.22.0 -> atspi-common@0.6.0
atspi@0.22.0 -> atspi-connection@0.6.0
atspi@0.22.0 -> atspi-proxies@0.6.0
eframe@0.29.1 -> egui-winit@0.29.1
eframe@0.29.1 -> egui_glow@0.29.1
egui-winit@0.29.1 -> accesskit_winit@0.22.4
egui_glow@0.29.1 -> egui-winit@0.29.1
workspace:grafito-app -> eframe@0.29.1
zbus-lockstep-macros@0.4.4 -> zbus-lockstep@0.4.4
zbus-lockstep-macros@0.4.4 -> zbus_xml@4.0.0
zbus-lockstep@0.4.4 -> zbus_xml@4.0.0
zbus_xml@4.0.0 -> quick-xml@0.30.0
```
<!-- reviewed-quick-xml-ancestor-edges:end -->

The 0.30.0 branch includes both `zbus-lockstep-macros` and the regular
`zbus-lockstep` library. The proc macro parses the crate-shipped AT-SPI XML
during compilation. `atspi-common` references the regular library's XML helper
functions only from its `cfg(test)` module in the reviewed immutable release;
Grafito's release targets do not call those helpers. Every 0.39.4 route reaches
the `wayland-scanner` proc macro through the Wayland protocol/toolkit graph.

No compatible lockfile update exists for the remaining zbus path. `zbus_xml` 4.0.0 requires quick-xml
`^0.30` and cannot select the patched >=0.41 line; `wayland-scanner` 0.31.11 now
requires `^0.41` and is verified patched (quick-xml 0.41.0 in Cargo.lock), so only the
zbus AT-SPI path remains vulnerable. The eframe 0.31.1 line is the newest tested
line that retains Rust 1.88, but it still resolves the vulnerable quick-xml 0.30.0
via zbus and also changes wgpu 22 to 24. Even eframe 0.35.0 requires Rust 1.92 and
still uses winit 0.30.13. Disabling AccessKit or Wayland would remove paths by
regressing accessibility or native Linux support rather than by fixing the dependency.

Reachability is narrower than the package-level audit report:

- For `RUSTSEC-2026-0194`, quick-xml 0.30.0 duplicate-attribute checking is
  reached through `zbus_xml` deserialization used by AT-SPI validation macros,
  and quick-xml 0.30.0 is reached by wayland-scanner now via quick-xml 0.41.0 (patched); the former 0.39.4 path via `wayland-scanner` attribute iteration.
  Both consumers parse XML shipped inside locked dependency crates during
  compilation; Grafito accepts no XML through either path at runtime.
- For `RUSTSEC-2026-0195`, neither consumer uses `NsReader`, the affected API.
  `zbus_xml` 4 deserialization uses a plain `Reader`, as does
  `wayland-scanner`, so the vulnerable namespace-declaration allocation is not
  reached in the reviewed graph.

Residual risk is a build-time CPU denial of service if the locked, checksummed
AT-SPI or Wayland XML inputs themselves become malicious, plus the possibility
that a future dependency change creates runtime reachability. The CI boundary
therefore constructs cargo-audit arguments from one fixed six-ID tuple, requires
exact parity with `deny.toml`, and rejects direct audit invocations or either
space/equal ignore syntax in workflows. The audit runs from an isolated working
directory and `CARGO_HOME`, with an explicit lockfile, so project or user
`audit.toml` files cannot append hidden exceptions. CI resolves locked
all-feature metadata, inspects every workspace dependency declaration including
optional, target, and build dependencies, compares the complete ancestor DAG for
both vulnerable versions, requires the reviewed crates.io sources and proc-macro
identities, and expires automatically. Remove both IDs from
`.github/scripts/verify_advisory_exceptions.py` and `deny.toml` as soon as a
compatible parent release lands. Re-review immediately if that boundary fails,
XML parsing is added to application code, or either advisory is revised.

### Reviewed direct quick-xml 0.42 dependency in grafito-ggb

`crates/grafito-ggb` declares `quick-xml 0.42` directly (pinned `0.42`, locked
`0.42.0` in `Cargo.lock`) with `default-features = false`. This is the only
workspace manifest allowed to do so; the CI policy
(`.github/scripts/verify_advisory_exceptions.py`,
`REVIEWED_DIRECT_DEPENDENCIES`) fails closed on any other direct declaration,
on version drift outside `0.42.x`, on a non-crates.io source, and on or after
the expiry below.

| Field | Value |
| --- | --- |
| Owner | Grafito maintainers (security contact above) |
| Reviewed | 2026-09-12 |
| CI expiry | 2026-12-31; CI fails on and after this date |
| Removal condition | Untrusted XML is no longer parsed, or the crate is replaced/audited anew |

Secure configuration (why XXE / billion-laughs cannot fire):

- Streaming pull parser only: `Reader::from_reader` over the size-capped
  `geogebra.xml` buffer; no DOM, no schema validation, no network access.
- `quick-xml` 0.42 never expands DTD internal/external entities by itself:
  custom entities resolve only through a caller-supplied resolver, and this
  code supplies none — attribute normalization expands only the five
  predefined entities plus character references and returns `Err` on unknown
  references. `DocType`/`Comment`/`PI`/`Text` events fall into the ignored `_`
  arm of the event loop (`crates/grafito-ggb/src/parse.rs`).
- Fail-closed explicit rejection: any `<!DOCTYPE` / `<!ENTITY` byte sequence
  is rejected with `DOCTYPE/ENTITY rechazado` both at ZIP extraction
  (`zip_read.rs::rechazar_doctype`) and at parse entry (`parse.rs`), so a
  future caller that skips the ZIP layer stays safe.
- Bounds: file 64 MiB / XML 10 MiB / ZIP entries 4096 / compression ratio 100
  with `checked_mul` (`zip_read.rs`, `lib.rs`); elements 5000, attribute
  values 8 KiB, attributes per element 128, command I/O attrs 64, XML depth
  64, expressions 2000 chars (`parse.rs`, `lib.rs`, `map.rs`).

Adversarial coverage (`crates/grafito-ggb/src/tests.rs`, all asserting honest
`Err`, none `#[ignore]`, all bounded payloads): billion laughs, XXE
`file:///etc/passwd` (also asserting no `root:` exfiltration in the error),
external parameter entity, quadratic blowup, oversized attribute, attribute
flood, excessive nesting depth, ZIP entry flood past `MAX_ZIP_ENTRIES`, and a
direct-`parsear` DOCTYPE rejection proving defense in depth.

Repository branch-protection rules, reviewer requirements, and commit-signing
requirements are configured in GitHub repository settings and cannot be
enforced or attested by this file.
