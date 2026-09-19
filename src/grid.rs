//! The neutral terminal-grid wire contract.
//!
//! Engine-agnostic, owned, `Send`, serde-serializable. The producer owns the VT
//! engine; the engine's native grid never crosses this seam. A [`GridDelta`] is
//! the inter-producer/consumer message — owned and **mergeable**, so under
//! backpressure successive frames coalesce (never an unbounded queue, never a
//! dropped frame).
//!
//! These types are deliberately *resolved* (concrete RGB, one interned style key
//! per cell): any VT engine maps onto them via its own projection function —
//! none is assumed to memcpy into a [`NeutralCell`]. Two independent producers
//! projecting the same byte stream must emit identical deltas (the cross-producer
//! conformance corpus in `conformance/grid/`).

use std::collections::BTreeMap;

use bitflags::bitflags;
use serde::{Deserialize, Serialize};

/// 8-bit RGB. The projection resolves engine colors — including the indexed
/// palette and the fg/bg/cursor defaults the engine does not supply — to
/// concrete RGB before they cross the seam.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Rgb {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

impl Rgb {
    pub const fn new(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b }
    }
}

impl From<(u8, u8, u8)> for Rgb {
    fn from((r, g, b): (u8, u8, u8)) -> Self {
        Self { r, g, b }
    }
}

bitflags! {
    /// Per-cell rendering attributes — a superset both engines map onto
    /// (libghostty's `Style` POD carries exactly these as bools; alacritty's
    /// `Flags` map by name).
    #[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
    pub struct Attrs: u16 {
        const BOLD      = 1 << 0;
        const ITALIC    = 1 << 1;
        const UNDERLINE = 1 << 2;
        const INVERSE   = 1 << 3;
        const DIM       = 1 << 4;
        const STRIKEOUT = 1 << 5;
        const BLINK     = 1 << 6;
        const HIDDEN    = 1 << 7;
        const OVERLINE  = 1 << 8;
    }
}

/// Interned style key. The projection assigns ids and ships first-seen styles in
/// a delta's [`GridDelta::new_styles`]; render caches glyph rasters on
/// `(glyph, StyleId)` so unchanged glyphs skip rasterization (design §4, §6).
#[derive(
    Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
)]
pub struct StyleId(pub u32);

/// The resolved style behind a [`StyleId`].
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Style {
    pub fg: Rgb,
    pub bg: Rgb,
    pub attrs: Attrs,
}

/// A cell's printable content: a single char (the common case), a multi-codepoint
/// grapheme cluster, or empty (blank — render fills the cell's bg, no glyph).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub enum CellContent {
    #[default]
    Empty,
    Char(char),
    Cluster(Box<str>),
    /// The second column of a 2-column (wide) glyph. Self-describing so a
    /// spacer-only patch stays unambiguous (per-cell damage can legally split
    /// the leader/spacer pair): render draws no glyph and no bg of its own —
    /// the leader at `col - 1` owns both; cursor/selection snap to/span from it.
    WideSpacer,
}

/// One neutral cell: resolved content + an interned style key.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct NeutralCell {
    pub content: CellContent,
    pub style: StyleId,
    /// Set on the **last cell of a row** when that row soft-wraps into the next
    /// (alacritty's `WRAPLINE`). Lets copy join a wrapped logical line without a
    /// spurious newline. Meaningless on non-last cells.
    pub wrapped: bool,
    /// This cell's glyph spans 2 columns (CJK, most emoji) — set for wide
    /// `Char` AND wide `Cluster` leaders; the following cell is a
    /// [`CellContent::WideSpacer`]. Defaulted so pre-wide serialized frames
    /// parse as narrow.
    #[serde(default)]
    pub wide: bool,
}

impl NeutralCell {
    pub fn new(content: CellContent, style: StyleId) -> Self {
        Self {
            content,
            style,
            wrapped: false,
            wide: false,
        }
    }
}

/// Grid dimensions in cells. Carries resize across the seam.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct GridDims {
    pub columns: u16,
    pub screen_lines: u16,
}

impl GridDims {
    pub const fn new(columns: u16, screen_lines: u16) -> Self {
        Self {
            columns,
            screen_lines,
        }
    }

    pub fn cells(&self) -> usize {
        self.columns as usize * self.screen_lines as usize
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub enum CursorShape {
    #[default]
    Block,
    Underline,
    Beam,
    Hidden,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct CursorState {
    pub row: u16,
    pub col: u16,
    pub shape: CursorShape,
    pub visible: bool,
}

/// A single damaged cell at `(row, col)`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CellPatch {
    pub row: u16,
    pub col: u16,
    pub cell: NeutralCell,
}

/// Shell-integration command status, shown as a colored mark in the pane's
/// left gutter at the command's prompt line. `Running` = command in flight
/// (no exit yet); `Ok`/`Fail` from the command's exit code (OSC 133);
/// `Manual` = a user-placed mark independent of a command (iTerm2 OSC 1337
/// `SetMark`) — same gutter + jump-navigation, distinct color.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub enum MarkStatus {
    #[default]
    Running,
    Ok,
    Fail,
    Manual,
}

/// Which xterm mouse-reporting protocols the app enabled (terminal state,
/// latest-wins on merge). `click`/`drag`/`motion` are cumulative levels
/// (1000/1002/1003); `sgr` (1006) selects the modern encoding.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct MouseProto {
    /// DECSET 1000: report button press/release.
    pub click: bool,
    /// DECSET 1002: also report motion while a button is held (drag).
    pub drag: bool,
    /// DECSET 1003: also report all motion.
    pub motion: bool,
    /// DECSET 1006: SGR extended encoding (unlimited coordinates).
    pub sgr: bool,
}

/// Owned, `Send`, **mergeable** render-bound delta.
///
/// Under backpressure the producer merges successive drains into one delta (the
/// union of patches) — frames coalesce, never drop, so render never needs a
/// resync. Bounded by viewport size, so there is no unbounded queue.
///
/// **On `#[serde(default)]` and merge semantics.** The terminal-state fields
/// (`alt_screen`, `bracketed_paste`, `mouse_reporting`, `mouse`, `app_cursor`,
/// `marks`) default when absent, so an evolving `#[non_exhaustive]` delta stays
/// forward/backward compatible on the wire. But these fields are **latest-wins on
/// `merge`**, and a *producer* always emits them — so on the wire "absent" is not
/// a distinct "unknown" state: it decodes to the default (`false` / empty), which
/// then overwrites on the next merge exactly as an emitted default would. Absent
/// == false here by construction; there is no reset-marker or tri-state to lose.
/// A future field where absent must mean "unchanged" would need an `Option`, not
/// a defaulted bool.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct GridDelta {
    /// Monotonic; render asserts "this is newer".
    pub epoch: u64,
    /// Current dims (carries resize).
    pub dims: GridDims,
    /// `Damage::Full` → render rebuilds from scratch.
    pub reset: bool,
    /// Only the damaged cells.
    pub cells: Vec<CellPatch>,
    /// Styles first referenced by this delta.
    pub new_styles: Vec<(StyleId, Style)>,
    pub cursor: CursorState,
    /// Snapshot of the engine's bracketed-paste mode (DEC 2004) as of this drain —
    /// terminal state, like `cursor`, not damage. Lets the app wrap pastes in
    /// `ESC[200~`…`ESC[201~` only when the app asked for it. Latest-wins on merge.
    #[serde(default)]
    pub bracketed_paste: bool,
    /// Scrollback viewport state (terminal state, latest-wins on merge): how many
    /// lines the display is scrolled **up** from the live bottom (`0` = at bottom),
    /// and how many lines of history exist above.
    pub display_offset: u16,
    pub history_len: u16,
    /// Alternate screen active (vim/less/htop/…): there is NO scrollback here, so
    /// the app must suppress history scrolling and translate the wheel to arrows.
    pub alt_screen: bool,
    /// The app has enabled mouse reporting — the wheel should go to it as mouse
    /// events, not be translated to arrow keys.
    #[serde(default)]
    pub mouse_reporting: bool,
    /// Application cursor keys (DECCKM, mode ?1): arrows must be sent as
    /// `ESC O A`… instead of `CSI A`…. Latest-wins on merge; defaulted so
    /// pre-field serialized frames parse.
    #[serde(default)]
    pub app_cursor: bool,
    /// Which mouse protocols are enabled (refines `mouse_reporting`).
    #[serde(default)]
    pub mouse: MouseProto,
    /// OSC 133 command marks currently **visible** in the viewport, as
    /// `(visible_row, status)` — recomputed each drain from the marks' absolute
    /// history lines + `display_offset`, so they scroll with the content. Latest-
    /// wins on merge (terminal state, not damage).
    #[serde(default)]
    pub marks: Vec<(u16, MarkStatus)>,
}

impl GridDelta {
    /// An empty (no-damage) delta at `epoch`/`dims`.
    pub fn new(epoch: u64, dims: GridDims) -> Self {
        Self {
            epoch,
            dims,
            ..Default::default()
        }
    }

    /// A full-reset delta (render rebuilds from scratch).
    pub fn full(epoch: u64, dims: GridDims) -> Self {
        Self {
            epoch,
            dims,
            reset: true,
            ..Default::default()
        }
    }

    /// Merge `newer` into `self` (coalescing under backpressure). Newer per-cell
    /// patches win; newer cursor/dims/epoch take precedence; first-seen styles
    /// accumulate (newer wins per id); a `reset` supersedes all pending state.
    ///
    /// # The coalescing law (apply-equivalence)
    ///
    /// Applying a merged delta equals applying its parts in order —
    /// `observe(apply(merge(a, b))) == observe(apply(b, apply(a)))` — **for all
    /// `a`, `b`, reset included**, stated on the *observable* grid (each cell
    /// resolved through its style). Property-tested over generated deltas.
    ///
    /// Two caveats make "universal" precise:
    /// * The law is on the **observable** grid, not the raw style table. On a
    ///   coalesced *reset*, `merge` carries prior styles forward, so its style
    ///   table can be a **superset** of a strict sequential apply's. That is
    ///   harmless — unreferenced styles do not render — and it is the deliberate
    ///   safety below.
    /// * It assumes the **producer invariant** that every delta's `new_styles`
    ///   covers the styles its cells reference (a reset ships its complete table).
    ///   Where that is *violated* (a coalesced incomplete reset), `merge` is not
    ///   merely equivalent to sequential apply — it is strictly **safer**: the
    ///   carry-forward resolves a cell whose style a naive sequential apply would
    ///   have stranded to black-on-black.
    pub fn merge(&mut self, mut newer: GridDelta) {
        if newer.reset {
            // A reset rebuilds cells from scratch, but `StyleId`s are stable and
            // cumulative: the engine's interner ships each style as `new_styles`
            // exactly once, and `newer`'s cells may reference ids first seen in the
            // delta being superseded. Carry the learned styles forward (newer wins
            // per id) so a coalesced reset never strands a cell with no known style
            // — otherwise the consumer falls back to the default style and renders
            // black-on-black.
            let mut styles: BTreeMap<StyleId, Style> =
                std::mem::take(&mut self.new_styles).into_iter().collect();
            for (id, style) in std::mem::take(&mut newer.new_styles) {
                styles.insert(id, style);
            }
            newer.new_styles = styles.into_iter().collect();
            *self = newer;
            return;
        }
        // Union cell patches — newer wins per (row, col); BTreeMap keeps the
        // result deterministically ordered (row, then col).
        let mut cells: BTreeMap<(u16, u16), NeutralCell> = std::mem::take(&mut self.cells)
            .into_iter()
            .map(|p| ((p.row, p.col), p.cell))
            .collect();
        for p in newer.cells {
            cells.insert((p.row, p.col), p.cell);
        }
        self.cells = cells
            .into_iter()
            .map(|((row, col), cell)| CellPatch { row, col, cell })
            .collect();

        // Union first-seen styles — newer wins per id.
        let mut styles: BTreeMap<StyleId, Style> =
            std::mem::take(&mut self.new_styles).into_iter().collect();
        for (id, style) in newer.new_styles {
            styles.insert(id, style);
        }
        self.new_styles = styles.into_iter().collect();

        self.epoch = newer.epoch;
        self.dims = newer.dims;
        self.cursor = newer.cursor;
        self.bracketed_paste = newer.bracketed_paste;
        self.display_offset = newer.display_offset;
        self.history_len = newer.history_len;
        self.app_cursor = newer.app_cursor;
        self.mouse = newer.mouse;
        self.alt_screen = newer.alt_screen;
        self.mouse_reporting = newer.mouse_reporting;
        self.marks = newer.marks;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn patch(row: u16, col: u16, ch: char, style: u32) -> CellPatch {
        CellPatch {
            row,
            col,
            cell: NeutralCell::new(CellContent::Char(ch), StyleId(style)),
        }
    }

    fn style(v: u8) -> Style {
        Style {
            fg: Rgb::new(v, v, v),
            bg: Rgb::default(),
            attrs: Attrs::empty(),
        }
    }

    /// A minimal painter model: reduce a delta sequence into a resolved grid.
    /// A `reset` rebuilds from scratch; otherwise cells/styles accumulate and the
    /// terminal-state scalars are latest-wins.
    #[derive(Clone, Debug, Default, PartialEq, Eq)]
    struct Painter {
        cells: BTreeMap<(u16, u16), NeutralCell>,
        styles: BTreeMap<StyleId, Style>,
        epoch: u64,
        dims: GridDims,
        cursor: CursorState,
        alt_screen: bool,
    }

    impl Painter {
        fn apply(&mut self, d: &GridDelta) {
            if d.reset {
                self.cells.clear();
                self.styles.clear();
            }
            for p in &d.cells {
                self.cells.insert((p.row, p.col), p.cell.clone());
            }
            for (id, s) in &d.new_styles {
                self.styles.insert(*id, *s);
            }
            self.epoch = d.epoch;
            self.dims = d.dims;
            self.cursor = d.cursor;
            self.alt_screen = d.alt_screen;
        }
        fn of(deltas: &[&GridDelta]) -> Self {
            let mut p = Painter::default();
            for d in deltas {
                p.apply(d);
            }
            p
        }
    }

    /// The coalescing law: merging two deltas then applying the result equals
    /// applying them in order — `apply(merge(a, b)) == apply(b, apply(a))`.
    #[test]
    fn merge_is_apply_equivalent_for_non_reset_deltas() {
        let dims = GridDims::new(80, 24);
        let mut a = GridDelta::new(1, dims);
        a.cells = vec![patch(0, 0, 'a', 0), patch(0, 1, 'b', 1)];
        a.new_styles = vec![(StyleId(0), style(10)), (StyleId(1), style(20))];

        let mut b = GridDelta::new(2, dims);
        // (0,1) overlaps a — b must win; (1,0) is new; StyleId(2) is fresh.
        b.cells = vec![patch(0, 1, 'B', 2), patch(1, 0, 'c', 0)];
        b.new_styles = vec![(StyleId(2), style(30))];
        b.cursor = CursorState {
            row: 1,
            col: 1,
            ..CursorState::default()
        };
        b.alt_screen = true;

        let sequential = Painter::of(&[&a, &b]);
        let mut merged = a.clone();
        merged.merge(b);
        let coalesced = Painter::of(&[&merged]);

        assert_eq!(
            sequential, coalesced,
            "coalescing two deltas must equal applying them in order"
        );
    }

    // ── The coalescing law as a property ─────────────────────────
    //
    // The law is stated on the OBSERVABLE grid — each cell resolved through its
    // style — not the raw style table, and it holds UNIVERSALLY (reset included)
    // GIVEN the producer invariant that every delta's `new_styles` covers the
    // styles its cells reference (a reset ships its complete table). The
    // generator honours that invariant. On a coalesced reset, `merge`'s raw style
    // table may be a *superset* (it carries prior styles forward) — harmless to
    // rendering, and precisely the safety that keeps a coalesced *incomplete*
    // reset from stranding a cell's style; see the note on [`GridDelta::merge`].

    use proptest::prelude::*;
    use std::collections::BTreeSet;

    fn arb_style_table() -> impl Strategy<Value = Vec<Style>> {
        // Styles for ids 0..4.
        prop::collection::vec(
            (any::<(u8, u8, u8)>(), any::<(u8, u8, u8)>(), 0u16..512).prop_map(
                |((fr, fg, fb), (br, bg, bb), bits)| Style {
                    fg: Rgb::new(fr, fg, fb),
                    bg: Rgb::new(br, bg, bb),
                    attrs: Attrs::from_bits_truncate(bits),
                },
            ),
            4,
        )
    }

    fn arb_delta(epoch: u64) -> impl Strategy<Value = GridDelta> {
        let dims = GridDims::new(8, 2);
        let cells = prop::collection::vec(
            (0u16..2, 0u16..8, prop::char::range('a', 'e'), 0usize..4),
            0..6,
        );
        (any::<bool>(), cells, arb_style_table()).prop_map(move |(reset, cells, table)| {
            let mut d = if reset {
                GridDelta::full(epoch, dims)
            } else {
                GridDelta::new(epoch, dims)
            };
            let mut referenced = BTreeSet::new();
            d.cells = cells
                .into_iter()
                .map(|(r, c, ch, sid)| {
                    referenced.insert(sid);
                    CellPatch {
                        row: r,
                        col: c,
                        cell: NeutralCell::new(CellContent::Char(ch), StyleId(sid as u32)),
                    }
                })
                .collect();
            // Self-contained: every referenced id gets its style (the producer
            // invariant a real reset upholds by shipping the full table).
            d.new_styles = referenced
                .into_iter()
                .map(|sid| (StyleId(sid as u32), table[sid]))
                .collect();
            d
        })
    }

    /// The observable grid: cells resolved through the accumulated style table,
    /// plus the latest terminal-state scalars. An unknown style resolves to the
    /// default (modelling a stranded cell), so a divergence there is caught.
    #[derive(Clone, Debug, PartialEq, Eq)]
    struct Observed {
        cells: BTreeMap<(u16, u16), (CellContent, Style)>,
        epoch: u64,
        dims: GridDims,
        cursor: CursorState,
        alt_screen: bool,
    }

    fn observe(deltas: &[&GridDelta]) -> Observed {
        let mut cells: BTreeMap<(u16, u16), NeutralCell> = BTreeMap::new();
        let mut styles: BTreeMap<StyleId, Style> = BTreeMap::new();
        let mut scalars = (0u64, GridDims::default(), CursorState::default(), false);
        for d in deltas {
            if d.reset {
                cells.clear();
                styles.clear();
            }
            for p in &d.cells {
                cells.insert((p.row, p.col), p.cell.clone());
            }
            for (id, s) in &d.new_styles {
                styles.insert(*id, *s);
            }
            scalars = (d.epoch, d.dims, d.cursor, d.alt_screen);
        }
        let resolved = cells
            .into_iter()
            .map(|((r, c), cell)| {
                let style = styles.get(&cell.style).copied().unwrap_or_default();
                ((r, c), (cell.content, style))
            })
            .collect();
        Observed {
            cells: resolved,
            epoch: scalars.0,
            dims: scalars.1,
            cursor: scalars.2,
            alt_screen: scalars.3,
        }
    }

    proptest! {
        /// `observe(apply(merge(a, b))) == observe(apply(b, apply(a)))` for all
        /// `a`, `b` — reset or not.
        #[test]
        fn merge_apply_equivalence_holds_universally(a in arb_delta(1), b in arb_delta(2)) {
            let sequential = observe(&[&a, &b]);
            let mut merged = a.clone();
            merged.merge(b);
            let coalesced = observe(&[&merged]);
            prop_assert_eq!(sequential, coalesced);
        }
    }

    #[test]
    fn merge_unions_distinct_cells() {
        let dims = GridDims::new(80, 24);
        let mut a = GridDelta::new(1, dims);
        a.cells = vec![patch(0, 0, 'a', 0)];
        let mut b = GridDelta::new(2, dims);
        b.cells = vec![patch(0, 1, 'b', 0)];
        a.merge(b);
        assert_eq!(a.epoch, 2);
        assert_eq!(a.cells, vec![patch(0, 0, 'a', 0), patch(0, 1, 'b', 0)]);
    }

    #[test]
    fn merge_newer_cell_wins_at_same_position() {
        let dims = GridDims::new(80, 24);
        let mut a = GridDelta::new(1, dims);
        a.cells = vec![patch(2, 3, 'o', 0)];
        let mut b = GridDelta::new(2, dims);
        b.cells = vec![patch(2, 3, 'n', 1)];
        a.merge(b);
        assert_eq!(a.cells, vec![patch(2, 3, 'n', 1)]);
    }

    #[test]
    fn merge_reset_supersedes_pending() {
        let dims = GridDims::new(80, 24);
        let mut a = GridDelta::new(1, dims);
        a.cells = vec![patch(0, 0, 'a', 0)];
        let b = GridDelta::full(5, dims);
        a.merge(b);
        assert!(a.reset);
        assert!(a.cells.is_empty());
        assert_eq!(a.epoch, 5);
    }

    #[test]
    fn merge_reset_carries_styles_forward() {
        // Regression: a styles-bearing delta coalescing with a later reset (e.g.
        // init styles + a resize reset, both pending before the first drain) must
        // not strand cells with no known style — else the consumer renders the
        // default style (black-on-black). The interner only ships each style once,
        // so the reset must keep the superseded delta's styles.
        let dims = GridDims::new(80, 24);
        let red = Style {
            fg: Rgb::new(255, 0, 0),
            ..Default::default()
        };
        let mut a = GridDelta::new(1, dims);
        a.new_styles = vec![(StyleId(0), red)];

        // A full reset whose cells reference StyleId(0) but ships no new styles.
        let mut b = GridDelta::full(2, dims);
        b.cells = vec![patch(0, 0, 'x', 0)];
        a.merge(b);

        assert!(a.reset);
        assert_eq!(
            a.new_styles,
            vec![(StyleId(0), red)],
            "reset must carry forward the style its cells reference"
        );
    }

    #[test]
    fn merge_accumulates_new_styles_newer_wins() {
        let dims = GridDims::new(80, 24);
        let mut a = GridDelta::new(1, dims);
        a.new_styles = vec![(StyleId(0), Style::default())];
        let mut b = GridDelta::new(2, dims);
        let red = Style {
            fg: Rgb::new(255, 0, 0),
            ..Default::default()
        };
        b.new_styles = vec![(StyleId(0), red), (StyleId(1), Style::default())];
        a.merge(b);
        assert_eq!(
            a.new_styles,
            vec![(StyleId(0), red), (StyleId(1), Style::default())]
        );
    }

    // ---  wide-char contract ---------------

    #[test]
    fn pre_wide_serialized_cells_parse_as_narrow() {
        // A frame recorded before the `wide` field existed must deserialize.
        let json = r#"{"content":{"Char":"x"},"style":7,"wrapped":false}"#;
        let cell: NeutralCell = serde_json::from_str(json).expect("old frame parses");
        assert!(!cell.wide);
        assert_eq!(cell.content, CellContent::Char('x'));
    }

    #[test]
    fn wide_leader_cluster_and_spacer_round_trip() {
        let mut cjk = NeutralCell::new(CellContent::Char('漢'), StyleId(1));
        cjk.wide = true;
        let mut emoji = NeutralCell::new(CellContent::Cluster("👩\u{200d}🚀".into()), StyleId(2));
        emoji.wide = true;
        let spacer = NeutralCell::new(CellContent::WideSpacer, StyleId(1));
        for cell in [&cjk, &emoji, &spacer] {
            let s = serde_json::to_string(cell).unwrap();
            assert_eq!(&serde_json::from_str::<NeutralCell>(&s).unwrap(), cell);
        }
    }

    #[test]
    fn spacer_only_patch_stays_self_describing_through_merge() {
        // Per-cell damage can deliver a spacer WITHOUT its leader; after any
        // amount of coalescing it must still be identifiable in isolation.
        let dims = GridDims::new(80, 24);
        let mut a = GridDelta::new(1, dims);
        let mut b = GridDelta::new(2, dims);
        b.cells = vec![CellPatch {
            row: 3,
            col: 11,
            cell: NeutralCell::new(CellContent::WideSpacer, StyleId(0)),
        }];
        a.merge(b);
        assert_eq!(a.cells.len(), 1);
        assert_eq!(a.cells[0].cell.content, CellContent::WideSpacer);
    }
}
