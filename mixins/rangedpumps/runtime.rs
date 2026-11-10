// Pure scan and tank logic for Ranged Pumps (standalone crate lib, injected
// as `crate::game::rangedpumps`). No game access lives here.
//
// Upstream keeps a pump's scan as `range`, `columnIndex` and `currentPos`:
// it walks one column of the world below the pump down to the dimension's
// minimum Y, then the eight columns of a radius-1 ring, then radius 2, and so
// on out to the configured range. Only a still source block (level 0) is
// drained, one whole bucket at a time, into a single 32-bucket tank.

pub type Pos = [i32; 3];

/// Upstream `ServerConfig` default for `range`.
pub const DEFAULT_RANGE: i32 = 64;
/// Upstream `ServerConfig` clamps the range to 0..=1024.
pub const MAX_RANGE: i32 = 1024;
/// Upstream `ServerConfig` default for `speed`: one move every 8 ticks.
pub const SPEED_TICKS: u32 = 8;
/// Upstream `tankCapacity`: 32 buckets, in millibuckets.
pub const TANK_CAPACITY: i32 = 32_000;
/// One bucket in millibuckets (`FluidType.BUCKET_VOLUME`).
pub const BUCKET: i32 = 1_000;

/// The saved state of one pump's scan (upstream `range`, `columnIndex`,
/// `currentPos`). `range` starts at -1, which names the single column below
/// the pump.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Scan {
    pub range: i32,
    pub column_index: i32,
    pub current: Option<Pos>,
}

impl Default for Scan {
    fn default() -> Self {
        Scan::new()
    }
}

impl Scan {
    pub const fn new() -> Self {
        Scan {
            range: -1,
            column_index: 0,
            current: None,
        }
    }

    /// Upstream `getColumnCount`: one column below the pump at range -1 and
    /// a `2 * range + 1` square's perimeter afterwards (`8 + 8 * range`).
    pub fn column_count(range: i32) -> i32 {
        if range < 0 { 1 } else { 8 + 8 * range }
    }

    /// Upstream `getColumn`: the top block of the column at `index`, one
    /// block below the pump. Indexes outside the perimeter are clamped to the
    /// last column by the caller's count.
    pub fn column(origin: Pos, range: i32, index: i32) -> Pos {
        let [x, y, z] = origin;
        let top = y - 1;
        if range < 0 {
            return [x, top, z];
        }
        let horizontal = 3 + 2 * range;
        let vertical = 1 + 2 * range;
        let mut i = index;
        if i < horizontal {
            // Top edge, left to right.
            return [x - range - 1 + i, top, z - range - 1];
        }
        i -= horizontal;
        if i < vertical {
            // Right edge, top to bottom.
            return [x - range - 1 + vertical + 1, top, z - range - 1 + i + 1];
        }
        i -= vertical;
        if i < horizontal {
            // Bottom edge, right to left.
            return [
                x - range - 1 + horizontal - i - 1,
                top,
                z - range - 1 + horizontal - 1,
            ];
        }
        i -= horizontal;
        // Left edge, bottom to top.
        [x - range - 1, top, z - range - 1 + vertical - i]
    }

    /// Upstream `updateCurrentPosition`: the next position this pump should
    /// inspect, or `None` once it has passed `max_range`. A column descends
    /// one block per call until it reaches `min_y`; the next call starts the
    /// next column.
    pub fn advance(&mut self, origin: Pos, min_y: i32, max_range: i32) -> Option<Pos> {
        let at_bottom = self.current.is_some_and(|pos| pos[1] == min_y);
        if self.current.is_none() || at_bottom {
            if self.column_index >= Self::column_count(self.range) {
                self.range += 1;
                self.column_index = 0;
            }
            if self.range > max_range {
                self.current = None;
                return None;
            }
            let pos = Self::column(origin, self.range, self.column_index);
            self.column_index += 1;
            self.current = Some(pos);
        } else {
            let mut pos = self.current.expect("a current column exists");
            pos[1] -= 1;
            self.current = Some(pos);
        }
        self.current
    }

    /// The three saved fields (upstream `range`, `columnIndex`, `currentPos`).
    pub fn saved(&self) -> (i32, i32, Option<Pos>) {
        (self.range, self.column_index, self.current)
    }

    /// Restore from a save made by [`Scan::saved`].
    pub fn from_saved(range: i32, column_index: i32, current: Option<Pos>) -> Self {
        Scan {
            range,
            column_index: column_index.max(0),
            current,
        }
    }
}

/// A still source block's fluid, as upstream `drainFluidAt` accepts it:
/// `LiquidBlock` with `LEVEL == 0`. A waterlogged stair is a stair block, and
/// flowing water (`level` 1..7) is not a source, so neither is pumped.
pub fn source_fluid(block: &str, level: Option<&str>) -> Option<&'static str> {
    match (block, level) {
        ("minecraft:water", Some("0")) => Some("minecraft:water"),
        ("minecraft:lava", Some("0")) => Some("minecraft:lava"),
        _ => None,
    }
}

/// The pump's single tank, in millibuckets. It holds one fluid at a time,
/// like the pin's `native_block_entity_fluids` tanks and upstream's
/// `FluidStacksResourceHandler`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Tank {
    pub fluid: Option<String>,
    pub amount: i32,
}

impl Default for Tank {
    fn default() -> Self {
        Tank::EMPTY
    }
}

impl Tank {
    pub const EMPTY: Tank = Tank {
        fluid: None,
        amount: 0,
    };

    /// A tank holding `fluid` millibuckets, clamped to `0..=TANK_CAPACITY`.
    /// No fluid is an empty tank.
    pub fn new(fluid: &str, amount: i32) -> Self {
        if amount <= 0 {
            return Tank::EMPTY;
        }
        Tank {
            fluid: Some(fluid.to_owned()),
            amount: amount.min(TANK_CAPACITY),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.fluid.is_none() || self.amount <= 0
    }

    /// Room for `fluid`: all of the capacity when empty, the free part when
    /// it holds the same fluid, and none when it holds another.
    pub fn room_for(&self, fluid: &str) -> i32 {
        if self.is_empty() {
            return TANK_CAPACITY;
        }
        if self.fluid.as_deref() == Some(fluid) {
            return (TANK_CAPACITY - self.amount).max(0);
        }
        0
    }

    /// Whether a whole bucket fits, which is what upstream requires before it
    /// removes a source block.
    pub fn can_take_bucket(&self, fluid: &str) -> bool {
        self.room_for(fluid) >= BUCKET
    }

    /// Insert exactly one bucket; `false` (and no change) when it does not
    /// fit, mirroring upstream's `internalInsert(...) != BUCKET_VOLUME` guard.
    pub fn insert_bucket(&mut self, fluid: &str) -> bool {
        if !self.can_take_bucket(fluid) {
            return false;
        }
        if self.is_empty() {
            self.fluid = Some(fluid.to_owned());
            self.amount = 0;
        }
        self.amount += BUCKET;
        true
    }
}

/// The message a right click reports (upstream `PumpState`), without the
/// energy states of the energy-gated slice.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PumpState {
    Working,
    Full,
    Done,
}

impl PumpState {
    pub fn of(scan: &Scan, max_range: i32, tank: &Tank) -> Self {
        if scan.range > max_range {
            PumpState::Done
        } else if tank.amount > TANK_CAPACITY - BUCKET {
            PumpState::Full
        } else {
            PumpState::Working
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn column_counts_match_the_upstream_ring() {
        assert_eq!(Scan::column_count(-1), 1);
        assert_eq!(Scan::column_count(0), 8);
        assert_eq!(Scan::column_count(1), 16);
        assert_eq!(Scan::column_count(2), 24);
    }

    #[test]
    fn the_first_column_is_the_block_below_the_pump() {
        let mut scan = Scan::new();
        assert_eq!(scan.advance([0, 64, 0], -64, 64), Some([0, 63, 0]));
    }

    #[test]
    fn a_column_descends_one_block_per_advance() {
        let mut scan = Scan::new();
        assert_eq!(scan.advance([0, 64, 0], -64, 64), Some([0, 63, 0]));
        assert_eq!(scan.advance([0, 64, 0], -64, 64), Some([0, 62, 0]));
        assert_eq!(scan.advance([0, 64, 0], -64, 64), Some([0, 61, 0]));
    }

    #[test]
    fn range_zero_visits_the_eight_neighbours_after_the_below_column() {
        let mut scan = Scan::new();
        // The column below the pump is the whole of range -1.
        assert_eq!(scan.advance([0, 64, 0], 63, 64), Some([0, 63, 0]));
        let mut ring = Vec::new();
        for _ in 0..8 {
            ring.push(scan.advance([0, 64, 0], 63, 64).expect("a ring column"));
        }
        ring.sort_unstable();
        assert_eq!(
            ring,
            vec![
                [-1, 63, -1],
                [-1, 63, 0],
                [-1, 63, 1],
                [0, 63, -1],
                [0, 63, 1],
                [1, 63, -1],
                [1, 63, 0],
                [1, 63, 1],
            ]
        );
    }

    #[test]
    fn range_one_visits_a_sixteen_column_ring_at_radius_two() {
        let mut scan = Scan::new();
        assert_eq!(scan.advance([0, 64, 0], 63, 1), Some([0, 63, 0]));
        // Consume the range-0 ring.
        for _ in 0..8 {
            scan.advance([0, 64, 0], 63, 1);
        }
        let mut ring = Vec::new();
        for _ in 0..16 {
            ring.push(scan.advance([0, 64, 0], 63, 1).expect("a ring column"));
        }
        assert_eq!(
            ring.iter()
                .filter(|p| p[0].abs() == 2 || p[2].abs() == 2)
                .count(),
            16
        );
        assert!(ring.iter().all(|p| p[0].abs() <= 2 && p[2].abs() <= 2));
    }

    #[test]
    fn the_scan_is_done_after_the_configured_range() {
        let mut scan = Scan::new();
        assert_eq!(scan.advance([0, 64, 0], 63, 0), Some([0, 63, 0]));
        // The next advance switches to the range-0 ring.
        assert_eq!(scan.advance([0, 64, 0], 63, 0), Some([-1, 63, -1]));
        for _ in 0..7 {
            assert!(scan.advance([0, 64, 0], 63, 0).is_some());
        }
        assert_eq!(scan.advance([0, 64, 0], 63, 0), None);
        assert_eq!(scan.advance([0, 64, 0], 63, 0), None);
        assert_eq!(scan.range, 1);
    }

    #[test]
    fn only_still_water_and_lava_sources_are_drained() {
        assert_eq!(
            source_fluid("minecraft:water", Some("0")),
            Some("minecraft:water")
        );
        assert_eq!(
            source_fluid("minecraft:lava", Some("0")),
            Some("minecraft:lava")
        );
        assert_eq!(source_fluid("minecraft:water", Some("1")), None);
        assert_eq!(source_fluid("minecraft:water", Some("7")), None);
        assert_eq!(source_fluid("minecraft:water", None), None);
        assert_eq!(source_fluid("minecraft:stone", None), None);
        assert_eq!(
            source_fluid("minecraft:oak_stairs", Some("0")),
            None,
            "a waterlogged stair is a stair block, not a source"
        );
    }

    #[test]
    fn a_tank_takes_whole_buckets_of_one_fluid_up_to_its_capacity() {
        let mut tank = Tank::EMPTY;
        assert_eq!(tank.room_for("minecraft:water"), TANK_CAPACITY);
        assert!(tank.insert_bucket("minecraft:water"));
        assert_eq!(tank.fluid.as_deref(), Some("minecraft:water"));
        assert_eq!(tank.amount, BUCKET);
        assert_eq!(tank.room_for("minecraft:water"), TANK_CAPACITY - BUCKET);
        assert!(tank.insert_bucket("minecraft:water"));
        assert_eq!(tank.amount, 2 * BUCKET);
    }

    #[test]
    fn a_tank_refuses_a_different_fluid_and_a_partial_bucket() {
        let mut tank = Tank::new("minecraft:water", BUCKET);
        assert_eq!(tank.room_for("minecraft:lava"), 0);
        assert!(!tank.can_take_bucket("minecraft:lava"));
        assert!(!tank.insert_bucket("minecraft:lava"));
        assert_eq!(tank.amount, BUCKET);

        let mut nearly = Tank::new("minecraft:water", TANK_CAPACITY - 999);
        assert!(!nearly.can_take_bucket("minecraft:water"));
        assert!(!nearly.insert_bucket("minecraft:water"));
        assert_eq!(nearly.amount, TANK_CAPACITY - 999);
    }

    #[test]
    fn the_state_is_done_then_full_else_working() {
        let working = Scan::new();
        assert_eq!(
            PumpState::of(&working, 64, &Tank::EMPTY),
            PumpState::Working
        );
        let full = Tank::new("minecraft:water", TANK_CAPACITY - 999);
        assert_eq!(PumpState::of(&working, 64, &full), PumpState::Full);
        let mut done = Scan::new();
        done.advance([0, 64, 0], 63, 0);
        for _ in 0..9 {
            done.advance([0, 64, 0], 63, 0);
        }
        assert_eq!(PumpState::of(&done, 0, &Tank::EMPTY), PumpState::Done);
    }

    #[test]
    fn scan_round_trips_through_its_saved_fields() {
        let mut scan = Scan::new();
        scan.advance([0, 64, 0], -64, 64);
        let (range, column_index, current) = scan.saved();
        let restored = Scan::from_saved(range, column_index, current);
        assert_eq!(restored, scan);
    }
}
