// Game-side glue for Ranged Pumps (injected into `crate::game` as
// `rangedpumps_state`; it needs the game, so it is not part of the standalone
// library).
//
// Upstream ticks a pump's block entity: every configured `speed` ticks it
// moves one block down the current column (then the next column of the ring),
// drains a still source block into its one 32-bucket tank, replaces the
// source with stone, and each tick pushes its tank into neighbouring fluid
// handlers. This port keeps the pure scan/tank arithmetic in `runtime.rs`
// and does the world work here: the pin stores a mod block entity's tanks in
// its saved `fluids` compound (API 54), so the tank is read and written in
// that shape.

use horizon_client::containers::{BlockEntity, ModBlockEntity, ModFluidTanks};
use horizon_client::text::Text;
use horizon_core::geometry::Direction;
use horizon_world::nbt::{Compound, JavaString, Tag};

use crate::game::native_server::{NativeCommand, NativeServerPlayer};
use crate::game::rangedpumps::{
    DEFAULT_RANGE, MAX_RANGE, Pos, PumpState, SPEED_TICKS, Scan, TANK_CAPACITY, Tank, source_fluid,
};

/// The block and its item.
const PUMP: &str = "rangedpumps:pump";
/// What a drained source is replaced with (upstream `blockIdToReplaceLiquidsWith`).
const REPLACE_WITH: &str = "minecraft:stone";

fn key(name: &str) -> JavaString {
    JavaString::from(name)
}

fn int(nbt: &Compound, name: &str) -> Option<i32> {
    match nbt.get(&key(name)) {
        Some(Tag::Int(value)) => Some(*value),
        _ => None,
    }
}

fn long(nbt: &Compound, name: &str) -> Option<i64> {
    match nbt.get(&key(name)) {
        Some(Tag::Long(value)) => Some(*value),
        _ => None,
    }
}

/// Java `BlockPos.asLong`, the shape upstream saves `currentPos` in.
fn pack_pos([x, y, z]: Pos) -> i64 {
    ((i64::from(x) & 0x3FF_FFFF) << 38)
        | ((i64::from(z) & 0x3FF_FFFF) << 12)
        | (i64::from(y) & 0xFFF)
}

/// Java `BlockPos.getX/getY/getZ` (sign-extending each field).
fn unpack_pos(value: i64) -> Pos {
    [
        (value >> 38) as i32,
        ((value << 52) >> 52) as i32,
        ((value << 26) >> 38) as i32,
    ]
}

fn read_scan(nbt: &Compound) -> Scan {
    Scan::from_saved(
        int(nbt, "range").unwrap_or(-1),
        int(nbt, "columnIndex").unwrap_or(0),
        long(nbt, "currentPos").map(unpack_pos),
    )
}

fn write_scan(nbt: &mut Compound, scan: &Scan) {
    let (range, column_index, current) = scan.saved();
    nbt.insert(key("range"), Tag::Int(range));
    nbt.insert(key("columnIndex"), Tag::Int(column_index));
    match current {
        Some(pos) => {
            nbt.insert(key("currentPos"), Tag::Long(pack_pos(pos)));
        }
        None => {
            nbt.remove(&key("currentPos"));
        }
    }
}

/// The pump's configured ring radius (upstream `ServerConfig.range`); this
/// port keeps it per block so a pump's range can be set and read in game.
fn configured_range(nbt: &Compound) -> i32 {
    int(nbt, "configuredRange")
        .unwrap_or(DEFAULT_RANGE)
        .clamp(0, MAX_RANGE)
}

/// The entity's first tank, as the pin keeps it under `fluids`.
fn read_tank(nbt: &Compound) -> Tank {
    let Some(Tag::List(list)) = nbt.get(&key("fluids")) else {
        return Tank::EMPTY;
    };
    let Some(Tag::Compound(entry)) = list.first() else {
        return Tank::EMPTY;
    };
    let fluid = match entry.get(&key("Fluid")) {
        Some(Tag::String(fluid)) if fluid.to_string_lossy() != "minecraft:empty" => {
            fluid.to_string_lossy()
        }
        _ => return Tank::EMPTY,
    };
    let amount = match entry.get(&key("Amount")) {
        Some(Tag::Int(amount)) => *amount,
        _ => 0,
    };
    Tank::new(&fluid, amount)
}

/// Write the first tank back in the pin's `fluids` shape.
fn write_tank(nbt: &mut Compound, tank: &Tank) {
    let mut entry = Compound::new();
    entry.insert(
        key("Fluid"),
        Tag::String(JavaString::from(
            tank.fluid.as_deref().unwrap_or("minecraft:empty"),
        )),
    );
    entry.insert(key("Amount"), Tag::Int(tank.amount));
    nbt.insert(key("fluids"), Tag::List(vec![Tag::Compound(entry)]));
}

/// The pump block entity at `pos`, if it is ours.
fn pump_at<'a>(game: &'a crate::game::Game<'_>, pos: Pos) -> Option<&'a ModBlockEntity> {
    match game.world.block_entities.get(&(pos[0], pos[1], pos[2])) {
        Some(BlockEntity::Mod(entity)) if entity.id == PUMP => Some(entity),
        _ => None,
    }
}

/// The pump declares its tank and ticks (API 15/54): one 32-bucket tank, every
/// face reaches it, only output is allowed (upstream's tank refuses external
/// insertion and lets a neighbour drain it).
pub fn on_block_entity_fluids(block: &str, fluids: &mut Option<ModFluidTanks>) {
    if block == PUMP {
        let all = vec![0usize];
        *fluids = Some(ModFluidTanks {
            tanks: vec![TANK_CAPACITY],
            faces: [
                all.clone(),
                all.clone(),
                all.clone(),
                all.clone(),
                all.clone(),
                all,
            ],
            inputs: Vec::new(),
            outputs: vec![0],
        });
    }
}

/// The pump ticks on the server (`native_block_entity_ticks`).
pub fn on_block_entity_ticks(block: &str, ticks: &mut bool) {
    if block == PUMP {
        *ticks = true;
    }
}

/// One server tick of a pump: every `SPEED_TICKS`, advance the scan and drain
/// a still source block into the tank.
pub(super) fn on_block_entity_tick(
    game: &mut crate::game::Game<'_>,
    pos: Pos,
    entity: &mut ModBlockEntity,
) {
    if entity.id != PUMP {
        return;
    }
    let dimension = game.native_dimension().to_owned();
    let max_range = configured_range(&entity.nbt);
    let ticks = int(&entity.nbt, "ticks").unwrap_or(0).max(0) as u32;
    if ticks % SPEED_TICKS == 0 {
        let mut scan = read_scan(&entity.nbt);
        if let Some(target) = scan.advance(pos, game.world.bottom_y(), max_range) {
            let block = game.native_block(target);
            let level = game.native_block_property(target, "level");
            if let Some(fluid) = source_fluid(&block, level.as_deref()) {
                let mut tank = read_tank(&entity.nbt);
                if tank.insert_bucket(fluid) {
                    write_tank(&mut entity.nbt, &tank);
                    replace_source(game, &dimension, target);
                    game.world.modified.insert((pos[0] >> 4, pos[2] >> 4));
                }
            }
        }
        write_scan(&mut entity.nbt, &scan);
    }
    entity.nbt.insert(key("ticks"), Tag::Int((ticks as i32).wrapping_add(1)));
}

/// Upstream `drainFluidAt(false)` + `tryReplaceLiquidWithBlock`: air first, then
/// the configured block (stone by default), so nearby fluid updates see the
/// removal.
fn replace_source(game: &mut crate::game::Game<'_>, dimension: &str, pos: Pos) {
    if let Some(air) = game.pack.assets.registry.default_state("minecraft:air") {
        game.native_set_block(dimension, pos, air, 11);
    }
    if let Some(stone) = game.pack.assets.registry.default_state(REPLACE_WITH) {
        game.native_set_block(dimension, pos, stone, 3);
    }
}

/// Upstream `fillNeighbors`: after the level's tick, push each pump's tank into
/// the neighbouring fluid handlers, an equal share to each.
pub(super) fn on_level_tick_post(game: &mut crate::game::Game<'_>, dimension: &str) {
    let pumps: Vec<Pos> = game
        .world
        .block_entities
        .iter()
        .filter_map(|(pos, entity)| match entity {
            BlockEntity::Mod(entity) if entity.id == PUMP => Some([pos.0, pos.1, pos.2]),
            _ => None,
        })
        .collect();
    for pos in pumps {
        let Some(tank) = pump_at(game, pos).map(|entity| read_tank(&entity.nbt)) else {
            continue;
        };
        if tank.is_empty() {
            continue;
        }
        let mut targets = Vec::new();
        for face in Direction::ALL {
            let [dx, dy, dz] = face.normal();
            let neighbour = [pos[0] + dx, pos[1] + dy, pos[2] + dz];
            let opposite = face.opposite();
            let Some(tanks) = game.native_fluid_tanks(dimension, neighbour, opposite) else {
                continue;
            };
            if tanks.iter().any(|target| {
                target.can_fill
                    && target.amount < target.capacity
                    && (target.fluid.is_none()
                        || target.fluid.as_deref() == tank.fluid.as_deref())
            }) {
                targets.push((face, neighbour, opposite));
            }
        }
        if targets.is_empty() {
            continue;
        }
        let share = tank.amount / targets.len() as i32;
        if share <= 0 {
            continue;
        }
        for (face, neighbour, opposite) in targets {
            game.native_move_fluids(dimension, pos, face, neighbour, opposite, share);
        }
    }
}

/// A right click reports the state and the tank, as upstream's
/// `useWithoutItem` does.
pub(super) fn on_right_click_block(
    game: &mut crate::game::Game<'_>,
    pos: Pos,
    _face: [i32; 3],
    canceled: &mut bool,
) {
    if game.native_block(pos) != PUMP {
        return;
    }
    let Some(entity) = pump_at(game, pos) else {
        return;
    };
    let scan = read_scan(&entity.nbt);
    let max_range = configured_range(&entity.nbt);
    let tank = read_tank(&entity.nbt);
    let state = PumpState::of(&scan, max_range, &tank);
    let state_line = match state {
        PumpState::Working => {
            let at = scan.current.unwrap_or([pos[0], pos[1] - 1, pos[2]]);
            format!(
                "Scanning at {}, {}, {} (range: {} of {}).",
                at[0],
                at[1],
                at[2],
                scan.range.max(0),
                max_range
            )
        }
        PumpState::Full => "Internal tank is full.".to_string(),
        PumpState::Done => "Done.".to_string(),
    };
    let tank_line = format!(
        "{} {} / {} mB",
        tank.fluid.as_deref().unwrap_or("empty"),
        tank.amount,
        TANK_CAPACITY
    );
    game.native_feedback(Text::literal(state_line));
    game.native_feedback(Text::literal(tank_line));
    *canceled = true;
}

/// Declare `/rangedpumps info` and `/rangedpumps range`.
pub fn on_register_commands(commands: &mut Vec<NativeCommand>) {
    commands.push(NativeCommand::new("rangedpumps info <pos:block_pos>"));
    commands.push(NativeCommand::op(
        "rangedpumps range <pos:block_pos> <range:integer>",
        2,
    ));
}

/// Run the `/rangedpumps` commands a player types.
pub(super) fn on_perform_command(
    game: &mut crate::game::Game<'_>,
    _player: &NativeServerPlayer,
    command: &str,
    ran: &mut bool,
) {
    let words: Vec<&str> = command.split_whitespace().collect();
    match words.as_slice() {
        ["rangedpumps", "info", x, y, z] => {
            *ran = true;
            let Some(pos) = parse_pos(&[*x, *y, *z]) else {
                game.native_feedback(Text::literal("Usage: /rangedpumps info <x> <y> <z>"));
                return;
            };
            let Some(entity) = pump_at(game, pos) else {
                game.native_feedback(Text::literal("No Ranged Pump there."));
                return;
            };
            let scan = read_scan(&entity.nbt);
            let max_range = configured_range(&entity.nbt);
            let tank = read_tank(&entity.nbt);
            let state = PumpState::of(&scan, max_range, &tank);
            let label = match state {
                PumpState::Working => "working",
                PumpState::Full => "full",
                PumpState::Done => "done",
            };
            let at = scan.current.unwrap_or([pos[0], pos[1] - 1, pos[2]]);
            game.native_feedback(Text::literal(format!(
                "Ranged Pump: {label}, range {} of {}, scan {}, {}, {}, {} {} / {} mB",
                scan.range.max(0),
                max_range,
                at[0],
                at[1],
                at[2],
                tank.fluid.as_deref().unwrap_or("empty"),
                tank.amount,
                TANK_CAPACITY
            )));
        }
        ["rangedpumps", "range", x, y, z, range] => {
            *ran = true;
            let Some(pos) = parse_pos(&[*x, *y, *z]) else {
                game.native_feedback(Text::literal(
                    "Usage: /rangedpumps range <x> <y> <z> <range>",
                ));
                return;
            };
            let Ok(range) = range.parse::<i32>() else {
                game.native_feedback(Text::literal("The range must be a whole number."));
                return;
            };
            if pump_at(game, pos).is_none() {
                game.native_feedback(Text::literal("No Ranged Pump there."));
                return;
            }
            let clamped = range.clamp(0, MAX_RANGE);
            let entity = match game.world.block_entities.get_mut(&(pos[0], pos[1], pos[2])) {
                Some(BlockEntity::Mod(entity)) if entity.id == PUMP => entity,
                _ => {
                    game.native_feedback(Text::literal("No Ranged Pump there."));
                    return;
                }
            };
            entity
                .nbt
                .insert(key("configuredRange"), Tag::Int(clamped));
            // Re-scan from the block below with the new maximum.
            write_scan(&mut entity.nbt, &Scan::new());
            entity.nbt.insert(key("ticks"), Tag::Int(0));
            game.world.modified.insert((pos[0] >> 4, pos[2] >> 4));
            game.native_feedback(Text::literal(format!("Ranged Pump range set to {clamped}.")));
        }
        _ => {}
    }
}

fn parse_pos(words: &[&str; 3]) -> Option<Pos> {
    Some([
        words[0].parse().ok()?,
        words[1].parse().ok()?,
        words[2].parse().ok()?,
    ])
}
