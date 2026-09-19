//! `gridwire` — the neutral terminal-grid + backend wire contract.
//!
//! The shared contract a terminal frontend and a session backend agree on:
//! resolved, owned, serde-serializable grid deltas plus the control-in wire and
//! the producer's projection seam. Depends on `serde` + `bitflags` only, so it
//! sits below every consumer and can be a shared dependency without inverting the
//! layering.
//!
//! The crate is a contract between independent producers and consumers; changes
//! are additive under `#[non_exhaustive]`, and every producer proves agreement
//! against the shared corpus rather than against another producer.
//!
//! The cross-producer conformance corpus lives in `conformance/grid/`: two
//! independent producers projecting the same bytes must emit identical
//! [`grid::GridDelta`]s.
//!
//! # Fidelity-ceiling invariant
//!
//! > The neutral grid is always self-sufficient. `Passthrough` and the raw tap
//! > are additive-only; a client without the tap drops a `Passthrough` silently
//! > and renders the placeholder cells.
//!
//! Everything a consumer needs to render a correct screen is in the
//! [`grid::GridDelta`]. The semantic events-out lane ([`backend::BackendEvent`])
//! is additive fidelity on top; OSC 52 clipboard is a **named exception** carried
//! as its own capability-gated typed event, never in `Passthrough` (see
//! [`backend::ClipboardOp`]).

pub mod backend;
pub mod grid;

pub use backend::{
    BackendControl, BackendEvent, ClipboardOp, ExitStatus, OscEvent, PassthroughEvent, ScrollAmount,
    SearchHit, VtProjection,
};
pub use grid::{
    Attrs, CellContent, CellPatch, CursorShape, CursorState, GridDelta, GridDims, MarkStatus,
    MouseProto, NeutralCell, Rgb, Style, StyleId,
};
