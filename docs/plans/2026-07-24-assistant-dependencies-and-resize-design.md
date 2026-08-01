# Assistant Dependencies and Resize Recovery Design

## Problem

The assistant can verify a finite `grafito-param` proposal and then reject a
following graph command that needs that parameter, because every proposal is
preflighted against the unchanged original document. During native resize,
each intermediate canvas size invalidates the exact GPU cache key and can
schedule synchronous GPU warmup work before the window settles.

## Reframe

The graph failure is not malformed Grafito syntax: it is a missing dependency
between independently reviewed proposals. The resize issue is not solved by
removing size from the cache key, because screen-space geometry must match the
current canvas. Preview quality must also act as a scheduling boundary, not
only as a lower sampling resolution.

## Approach

Keep proposal application explicit while recording ordered parameter
prerequisites during isolated verification. A graph card that depends on prior
parameters clearly states that its explicit Apply action stages those
parameters and the graph together, then commits one transactional result. A
parameter card remains independently applicable. Verification and correction
remain local-first and a correction remains user-triggered, single-use, and
attachment-free.

For resize, update the document screen size only when the actual canvas rect
changes. Mark the existing interaction hysteresis as Preview on that change,
render through the CPU fallback while it is active, and defer GPU warmup until
the existing 150 ms idle promotion restores High quality. Preserve exact cache
keys and stale-buffer rejection.

## Scope

In scope: ordered `grafito-param` prerequisites for verified graph/scene
proposals, explicit atomic Apply of required parameters plus a graph, correction
eligibility when a parameter is valid but no command is valid, conditional
screen-size mutation, resize preview scheduling, and CPU ComplexGrid preview
resolution.

Out of scope: autonomous document mutations, remote tool loops, attachment
retransmission, unbounded correction attempts, removing screen size from GPU
cache keys, or replacing synchronous GPU readback globally.

## Technical Design

`VerifiedAssistantProposal` identifies a parsed candidate in the latest
assistant response by ordinal, retains its original `AssistantProposal`, and
records accepted earlier parameter assignments. Historical assistant turns never
receive the latest response's verified records. The verification worker advances
only a detached parameter document; graph and scene preflights consume that
detached context but never commit it. UI actions use the ordinal rather than
proposal equality, so duplicate fences cannot borrow another card's prerequisite
list.

On graph or scene Apply, the app clones the current live document, stages the
recorded finite parameter assignments, preflights the graph in that staged
document, and commits the final result with one undo snapshot. The UI explains
which assignments the click includes. A lone parameter still uses the ordinary
command transaction. Rejected commands may offer the existing one correction
when no graphical command verified, even if parameter cards did.

`Document::set_screen_size` performs an exact-change check before marking the
spatial index dirty. Both 2D and 3D canvases call it once before input handling;
input and paint no longer write size independently. Their GPU routes receive the
existing interaction-in-progress state and return CPU/no-warmup while it is
true. ComplexGrid CPU fallback caps Preview resolution consistently with the
renderer policy.

## Acceptance Criteria

1. A response containing `a = 1` followed by an `ImplicitCurve` using `a`
   verifies both without mutating the original document.
2. Clicking the dependent graph card explicitly applies its required parameter
   and graph as one undoable transaction; it never mutates before that click.
3. Duplicate identical command fences retain their own dependency identities.
4. A valid parameter plus an invalid graph can offer one attachment-free,
   user-triggered correction; scenes remain excluded.
5. Repeated frames at an unchanged canvas size do not mark the spatial index
   dirty through a screen-size write.
6. During resize/pan Preview, 2D and 3D rendering remain CPU-owned and schedule
   no GPU warmup; after the existing idle threshold, one normal warmup is eligible.
7. CPU ComplexGrid preview uses a bounded low resolution.

## Test Strategy

Add app assistant tests for parameter-dependent verification, atomic explicit
Apply staging, stale live parameter replacement, duplicate candidates, and
correction eligibility. Add UI tests for candidate ordinal matching. Add canvas
planning tests for interaction-time GPU suppression, core document tests for
idempotent screen-size updates, and render tests for ComplexGrid Preview caps.
Run focused crate tests before workspace fmt, strict Clippy, tests, release
build, diff check, and graph refresh.

## Risks

- Applying prerequisites inside a graph card could surprise users; the card
  explicitly lists the assignments and the click remains the only mutation.
- A changed document may make an earlier response invalid; current live
  preflight is retained immediately before commit.
- CPU rendering is still work during resize; Preview sampling and deferred GPU
  callbacks remove the avoidable synchronous GPU thrash without compromising
  projection correctness.
