# gridwire

The neutral terminal-grid and backend wire contract: resolved, owned, mergeable grid
deltas, the control-in wire, the semantic events-out wire, and the producer's projection
seam. It is what a terminal frontend and a session backend agree on so that either can be
replaced without the other noticing.

`gridwire` depends on `serde` and `bitflags` only. It carries no I/O, no async runtime,
and no terminal engine, so it sits below every consumer and can be a shared dependency
without inverting anyone's layering.

## What is in it

- `grid` — `NeutralCell`, `GridDelta`, `Style`/`Attrs`, `CursorState`, and
  `GridDelta::merge`, the coalescing operation whose correctness is stated as a law and
  property-tested: for all deltas `a`, `b` — resets included — the observable grid after
  applying `merge(a, b)` equals the observable grid after applying `a` then `b`, given
  that every delta's `new_styles` covers the styles its cells reference.
- `backend` — `BackendControl` (control in), `BackendEvent` and its payloads (events
  out), and `VtProjection`, the trait a producer implements to drain its engine's damage
  into a `GridDelta`. Runtime concerns — channels, wakers, process handles — are
  deliberately not here.
- `conformance/grid/` — the **cross-producer conformance corpus**. Every producer of a
  `GridDelta` from a byte stream must turn the same input into the same delta. The
  corpus is the contract; a producer's agreement with it is that producer's proof, and
  it belongs in that producer's own CI. The runner in this repository checks only that
  the corpus is well-formed and paired.

## The invariant

> The neutral grid is always self-sufficient. `Passthrough` and any raw byte lane are
> additive-only; a client without them drops a `Passthrough` silently and renders the
> placeholder cells.

Everything needed to render a correct screen is in `GridDelta`. Fidelity beyond it is a
choice, made per feature, never a discovery. Where a feature lands: if a producer can
model it and more than one client wants it, it becomes a typed additive field; if it is
opaque, engine-specific and rare — graphics protocols, control-mode passthrough — it
rides `BackendEvent::Passthrough`. **OSC 52 clipboard is never `Passthrough`:** it is
write-back, not render data, and a security surface; if carried at all it is its own
capability-gated, typed event.

## Versioning

Semver. Wire enums are `#[non_exhaustive]`; new fields on wire structs carry
`#[serde(default)]` so older deltas still deserialize. Not yet published to crates.io.

## License

Dual-licensed under MIT or Apache-2.0, at your option.
