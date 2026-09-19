//! The backend seam: the control-in wire, the events-out wire, and the
//! producer's projection trait.
//!
//! Scope: **wire types + the producer seam only**.
//! Everything that crosses the process boundary as data is here — the control-in
//! lane ([`BackendControl`]), the semantic events-out lane ([`BackendEvent`] and
//! its payloads), and [`VtProjection`] (a trait over `&mut GridDelta`, no runtime
//! types).
//!
//! **Deliberately omitted (stated, per the extraction rule):** the *runtime*
//! that carries these — anything holding a file descriptor, channel, waker, or
//! frame receiver (`BackendHandle`, `FrameRx`, `SessionBackend::spawn` in the
//! reference). Those are not wire; they stay in the producer/consumer, not in
//! this crate. The surface is otherwise a complete copy of the reference at its
//! shipped version.

use serde::{Deserialize, Serialize};

use crate::grid::{GridDelta, GridDims};

/// The projection: drain the engine's accumulated damage into a render-bound
/// delta, then clear the engine's native damage. The implementor owns the VT
/// engine; each engine (e.g. `alacritty_terminal`, `libghostty`) supplies its
/// own `VtProjection`. This is the seam the cross-producer conformance corpus
/// exercises: two implementors must drain the same bytes into the same delta.
pub trait VtProjection {
    /// Drain accumulated damage into `out`, merging into whatever is already
    /// pending, then clear the engine's native damage.
    fn drain_damage_into(&mut self, out: &mut GridDelta);
}

/// Inbound control — the "command-in" face. Data-only + serde so it serializes
/// unchanged onto the backend bus; `#[non_exhaustive]` for additive evolution.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub enum BackendControl {
    /// Bytes to write to the session (keyboard input, paste, …).
    Input(Box<[u8]>),
    /// Resize the session to a new grid.
    Resize(GridDims),
    /// Focus gained/lost (drives focus-reporting + cursor blink).
    Focus(bool),
    /// Scroll the display through scrollback history (engine-agnostic).
    Scroll(ScrollAmount),
    /// Jump the viewport to the previous (`-1`) / next (`+1`) prompt mark.
    JumpMark(i8),
    /// Search scrollback + screen for a regex and scroll the display to the
    /// match. `forward` continues past the previous match toward the bottom;
    /// `!forward` searches back up.
    Search {
        /// The regex pattern to search for.
        pattern: String,
        /// Search toward the bottom (`true`) or back up (`false`).
        forward: bool,
    },
    /// Re-ship everything: a full-reset delta carrying the complete style table,
    /// for a consumer with no accumulated state (a rebuilt renderer, or a pane
    /// re-hosted to a new window).
    RequestFull,
    /// Tear the session down.
    Shutdown,
}

/// A scrollback movement, in engine-neutral terms.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub enum ScrollAmount {
    /// Scroll by `n` lines: positive = up (into history), negative = down.
    Lines(i32),
    /// Jump to an absolute display offset (lines up from the bottom).
    To(u16),
    /// Up one screenful.
    PageUp,
    /// Down one screenful.
    PageDown,
    /// Jump to the oldest history line.
    Top,
    /// Jump to the live bottom.
    Bottom,
}

// ── Events-out (the semantic lane) ──────────────────────────────────────────
// Pure data that crosses the process boundary, so it is wire (serde) — the
// runtime handle that *carries* it (`BackendHandle`, frame receiver, waker)
// stays in the producer/consumer, not here.

/// Semantic-lane event — the "events-out" face. This lane is **ordered +
/// reliable**: events must not be dropped or reordered.
///
/// **Fidelity-ceiling invariant.** The neutral grid ([`GridDelta`]) is always
/// self-sufficient; `Passthrough` (and any out-of-band raw byte tap) is
/// **additive-only**. A consumer that cannot interpret a `Passthrough` drops it
/// silently and renders the placeholder cells — so adding a passthrough feature
/// later never breaks a grid-only consumer. Nothing a consumer needs for a
/// correct screen may live only in `Passthrough`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub enum BackendEvent {
    /// The terminal's title (OSC 0/2).
    Title(String),
    /// The bell (BEL / `\a`).
    Bell,
    /// A shell-integration / OSC semantic mark.
    Osc(OscEvent),
    /// A clipboard request (OSC 52). See [`ClipboardOp`] — this is the **named
    /// exception**: clipboard is never carried in `Passthrough`.
    Clipboard(ClipboardOp),
    /// **Opaque, additive-only** passthrough for engine-specific byte protocols
    /// the neutral grid deliberately does not model — Kitty graphics, iTerm2 OSC
    /// 1337 images, `tmux -CC`. A grid-only consumer drops it and stays correct.
    Passthrough(PassthroughEvent),
    /// Reply to [`BackendControl::Search`]: the match found (display already
    /// scrolled to show it), or `None` for no match / invalid pattern.
    SearchResult(Option<SearchHit>),
    /// The session ended.
    Exited(ExitStatus),
}

/// Shell-integration / OSC semantic events (OSC 133 marks + an iTerm2 OSC 1337
/// subset). Rides the ordered semantic lane.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub enum OscEvent {
    /// OSC 133 (FinalTerm): a prompt started.
    PromptStart,
    /// OSC 133: the command (user input) started.
    CommandStart,
    /// OSC 133: command output started.
    OutputStart,
    /// OSC 133: the command ended, with its exit code if known.
    CommandEnd(Option<i32>),
    /// OSC 633;E (VS Code shell integration) — the literal command line the
    /// shell is about to run.
    CommandLine(String),
    /// iTerm2 OSC 1337: the working directory changed.
    CurrentDir(String),
    /// iTerm2 OSC 1337: the remote host changed.
    RemoteHost(String),
    /// iTerm2 OSC 1337: a user-placed mark.
    SetMark,
}

/// Clipboard request from the session (OSC 52).
///
/// **Named exception to the opaque→`Passthrough` rule.** Clipboard is *write-back*,
/// not render data, and a security surface — a consumer blindly honoring a
/// `Passthrough`'d OSC 52 would be a program writing the operator's clipboard.
/// So it is **never** carried in `Passthrough`; it is this own typed event, and
/// honoring it is gated behind an explicit capability at the consumer (policy
/// lives with the consumer, not on the wire).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub enum ClipboardOp {
    /// The program asked to set the clipboard to this text.
    Set(String),
    /// The program asked to paste (read the clipboard).
    RequestPaste,
}

/// Opaque passthrough payload: engine-specific bytes the neutral grid does not
/// model (Kitty graphics, `tmux -CC`). Additive-only — see [`BackendEvent`].
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PassthroughEvent(pub Vec<u8>);

/// How a session ended.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExitStatus {
    /// The process exit code, if the session exited normally.
    pub code: Option<i32>,
}

/// A search match in scrollback-absolute coordinates — see
/// [`BackendEvent::SearchResult`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SearchHit {
    /// Match start `(line, column)`, scrollback-absolute, inclusive.
    pub start: (u32, u16),
    /// Match end `(line, column)`, scrollback-absolute, inclusive.
    pub end: (u32, u16),
    /// 1-based position of this match among all matches for the query.
    pub ordinal: u32,
    /// Total matches for the query (capped; a capped count reads as "N+").
    pub total: u32,
}
