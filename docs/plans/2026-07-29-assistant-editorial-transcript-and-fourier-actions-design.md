# Assistant Editorial Transcript And Fourier Actions Design

## Problem

The shared-width Assistant transcript lost visual hierarchy when role bubbles
were removed. User and Assistant text now look nearly identical, per-turn copy
controls compete with the role labels, and verified proposal actions are
duplicated and easy to miss. Fourier requests also commonly receive explanatory
text only, so no explicit Apply action exists.

## Reframe

The goal is not to restore messaging bubbles or make arbitrary mathematical
text executable. The goal is an editorial, full-width conversation that makes
authorship and verified actions obvious while preserving the existing local
preflight and explicit user approval boundary.

## Approach

Use full-width semantic bands rather than left/right bubbles. The user band
uses the existing muted accent surface; the Assistant band uses the existing
elevated input-bar surface. Both have the same width and body typography.
Move Copy to a quiet footer and make a verified proposal one clear primary
Apply action plus an optional secondary Edit action.

For Fourier intent, retrieve the existing safe `Function[expr]` capability and
guide the remote model to emit an explicitly expanded finite partial sum. Do
not implement a general Fourier transform and do not turn LaTex into commands.

## Scope

In scope: Assistant transcript hierarchy, verified/rejected proposal action
presentation, Fourier retrieval/prompt guidance, and regression tests.

Out of scope: a new Fourier transform command, automatic execution, converting
inline math to executable commands, changing preflight rules, or core sum
resource limits.

## Technical Design

`conversation_turn_appearance` centralizes full-width user/Assistant surfaces
from existing theme tokens. `draw_conversation_turn` and the pending indicator
render those bands with shared width, spacing, and a footer Copy action.

`draw_verified_assistant_proposal` becomes one tonal action card with one
"Aplicar en Grafito" control. `Editar` remains secondary and only for a direct
verified command without prerequisites. A rejected candidate gets a distinct
warning area and keeps the existing one-shot correction boundary.

The Function capability gains Fourier keywords. Remote guidance allows only a
short, explicitly expanded numeric partial sum such as a square-wave example;
it rejects symbolic coefficients, unknown order, `sum(...)`, and a general
transform as executable proposals.

## Acceptance Criteria

1. User and Assistant turns have distinct full-width surfaces but no side
   alignment, width split, or WhatsApp-style bubble layout.
2. A verified proposal presents exactly one primary Apply control; Edit stays
   secondary and never executes automatically.
3. A plain LaTex or inline formula remains non-actionable.
4. A Fourier request retrieves `Function[expr]`, and guidance asks for a safe
   finite expanded series rather than inventing a Fourier command.
5. A finite Fourier partial-sum Function preflights without changing the live
   document.

## Test Strategy

Add unit coverage for role appearance, Fourier catalog retrieval, remote prompt
guidance, finite Fourier proposal parsing/preflight, and action-card source
contracts. Run focused UI, command, assistant, and app tests before workspace
verification.

## Risks

The remote provider can still answer explanatory-only text when the request
lacks mathematical input. That response remains safe and non-actionable; the
new guidance makes the supported finite example path discoverable without
pretending a general Fourier transform exists.
