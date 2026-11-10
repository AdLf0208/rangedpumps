#!/usr/bin/env bash
# In-game check for Ranged Pumps slice 1: the pump drains still source blocks
# in its scan into its tank and pushes the tank into a neighbouring fluid
# handler.
#
# The End arena (stone floor at y=63, pumps at y=64). Every mod command is
# typed with `ctl chat "/cmd"` because `ctl command` runs the console path and
# bypasses the native `perform_command` hook.
#
# Default run:
#  A  (0,64,0),  range 0, water directly below  -> the source becomes stone and
#     `/rangedpumps info` shows water 1000 mB;
#  B  (8,64,0),  range 0, cauldron at (9,64,0)  -> the cauldron becomes a full
#     water cauldron (the tank's output path);
#  C  (16,64,0), range 0, no fluid below        -> the tank stays empty
#     (no-fluid control);
#  D  (24,64,0), range 0, water at (26,63,0)    -> the tank stays empty
#     (out-of-range control: range 0 scans only the 8 neighbours);
#  E  (32,64,0), range 0, waterlogged stair     -> the tank stays empty
#     (wrong-block control: a stair is not a source LiquidBlock).
#
# SELFTEST=1 flips the A assertion (it must FAIL), proving the scenario can
# fail.
set -euo pipefail
export LC_ALL=C

CLONE=${CAMPAIGN_ROOT:-$(cd "$(dirname "$0")/../../.." && pwd)}
PORT=$CLONE/ports/rangedpumps
# EMBEDDED=1 launches with no --dev-mod: the bundle carries its own content.
EMBEDDED=${EMBEDDED:-0}
DEV_MOD=()
[ "$EMBEDDED" = 1 ] || DEV_MOD=(--dev-mod "$PORT/content")

CTL=${CTL:-$PORT/target/horizon-cargo/fast/horizon-ctl}
GAME=${GAME:-$PORT/target/horizon-cargo/fast/horizon-client}
RES=${RES:-${HORIZON_RES:-${HORIZON_RES:-$HOME/.local/horizon-client/26.3}}}
SOCK=${SOCK:-/tmp/opencode/rangedpumps-e2e.sock}
SELFTEST=${SELFTEST:-0}
KEEP=${KEEP:-0}
WORLD=RangedPumpsE2E

[ -x "$CTL" ] || { echo "missing horizon-ctl: $CTL" >&2; exit 1; }
[ -x "$GAME" ] || { echo "missing horizon-client: $GAME" >&2; exit 1; }
[ -d "$RES/assets" ] || { echo "missing prepared resources: $RES" >&2; exit 1; }

SCRATCH=$(mktemp -d /tmp/opencode/rangedpumps-home.XXXXXX)
export HORIZON_HOME=$SCRATCH/game
export HORIZON_LAUNCHER_HOME=$SCRATCH/launcher
export XDG_DATA_HOME=$SCRATCH/data
export XDG_CONFIG_HOME=$SCRATCH/config
export HORIZON_NO_AUDIO=1
mkdir -p "$(dirname "$SOCK")" "$XDG_CONFIG_HOME"
rm -f "$SOCK"

ctl() { "$CTL" --socket "$SOCK" "$@"; }
say() { ctl chat "/$1" >/dev/null; }
fail() { echo "FAIL: $*" >&2; ctl logs --lines 60 >&2 || true; exit 1; }

# The `/rangedpumps info` line for a pump.
info() { # info X Y Z
    say "rangedpumps info $1 $2 $3"
    ctl wait 3 >/dev/null
    ctl messages 10 | jq -r '.chat[]' | grep -m1 'Ranged Pump:' || true
}
expect_info() { # expect_info X Y Z TEXT
    local line
    line=$(info "$1" "$2" "$3")
    echo "  info($1,$2,$3): $line"
    case "$line" in
        *"$4"*) ;;
        *) fail "expected /rangedpumps info to say '$4', got '$line'" ;;
    esac
}

# The block name at a position (the player stays in the End).
block_name() { ctl block "$1" "$2" "$3" | jq -r '.name // .block // empty'; }

cleanup() {
    ctl quit >/dev/null 2>&1 || true
    sleep 1
    if [ "$KEEP" = 1 ]; then
        echo "kept scratch: $SCRATCH" >&2
    else
        rm -rf "$SCRATCH" 2>/dev/null || true
    fi
}
finish() { local rc=$?; cleanup; echo "EXIT:$rc"; return $rc; }
trap finish EXIT

launch() {
    rm -f "$SOCK"
    cd "$SCRATCH"
    "$CTL" --socket "$SOCK" launch \
        --game "$GAME" --world "$WORLD" --seed 20261105 --creative --headless --wait \
        --resources "$RES" "${DEV_MOD[@]}" >/dev/null
    ctl wait-ready 120
    ctl wait 40
}

arena() {
    ctl command "execute in minecraft:the_end run tp @s 0.5 64 3.5"
    local loaded=false
    for _ in $(seq 1 120); do
        ctl wait 5
        if [ "$(ctl block 0 63 0 | jq -r '.loaded // false')" = true ]; then
            loaded=true
            break
        fi
    done
    [ "$loaded" = true ] || fail "the End arena chunk never loaded"
    ctl command "execute in minecraft:the_end run fill -8 63 -8 48 63 8 minecraft:stone"
    ctl command "execute in minecraft:the_end run fill -8 64 -8 48 74 8 minecraft:air"
    ctl command "execute in minecraft:the_end run kill @e[type=!player]"
    ctl wait 20
}

# An enclosed still-source cell at (x,63,z): stone below and on all four sides
# at y=63, with the pump placed at (x,64,z) closing the top.
pool() { # pool X Z BLOCK
    ctl command "execute in minecraft:the_end run fill $(( $1 - 1 )) 62 $(( $2 - 1 )) $(( $1 + 1 )) 62 $(( $2 + 1 )) minecraft:stone"
    ctl command "execute in minecraft:the_end run fill $(( $1 - 1 )) 63 $(( $2 - 1 )) $(( $1 + 1 )) 63 $(( $2 + 1 )) minecraft:stone"
    ctl command "execute in minecraft:the_end run setblock $1 63 $2 $3"
}

echo "== launch (The End arena, content + native bundle) =="
launch
arena

echo "== build the enclosed source cells =="
pool 0 0 minecraft:water
pool 8 0 minecraft:water
# C has no fluid: leave the stone floor. D's source is two blocks away, not
# in the range-0 eight-neighbour ring.
pool 26 0 minecraft:water
pool 32 0 minecraft:oak_stairs[waterlogged=true,facing=north,half=bottom,shape=straight]
ctl wait 4

echo "== place the pumps and set range 0 =="
for x in 0 8 16 24 32; do
    ctl command "execute in minecraft:the_end run setblock $x 64 0 rangedpumps:pump"
done
ctl wait 4
for x in 0 8 16 24 32; do
    say "rangedpumps range $x 64 0 0"
done
ctl wait 4
expect_info 0 64 0 "range 0 of 0"
expect_info 8 64 0 "range 0 of 0"
expect_info 16 64 0 "range 0 of 0"
expect_info 24 64 0 "range 0 of 0"
expect_info 32 64 0 "range 0 of 0"

echo "== A: a still water source below is drained into the tank =="
# The pump's first scan target is directly below.
ctl wait 20
state=$(block_name 0 63 0)
[ "$state" = "minecraft:stone" ] \
    || fail "the drained source at 0,63,0 is not stone (got '$state')"
echo "  source 0,63,0 -> $state"
if [ "$SELFTEST" = 1 ]; then
    echo "(selftest: asserting A's tank is empty)"
    expect_info 0 64 0 "empty"
else
    expect_info 0 64 0 "minecraft:water 1000 / 32000 mB"
fi

echo "== B: the tank is pushed into the adjacent cauldron (output) =="
ctl command "execute in minecraft:the_end run setblock 9 64 0 minecraft:cauldron"
# B was placed before its cauldron existed, so it may already hold the bucket.
ctl wait 20
cauldron=$(block_name 9 64 0)
[ "$cauldron" = "minecraft:water_cauldron" ] \
    || fail "the pump did not fill the neighbouring cauldron (got '$cauldron')"
echo "  cauldron 9,64,0 -> $cauldron"
expect_info 8 64 0 "empty"

echo "== C: no fluid below leaves the tank empty (control) =="
expect_info 16 64 0 "empty"

echo "== D: a source outside the range-0 ring is not drained (control) =="
# D is at 24,64,0; the water at 26,63,0 is two blocks away, outside the
# eight-neighbour ring, so range 0 never scans it.
expect_info 24 64 0 "empty"
ctl wait 20
expect_info 24 64 0 "empty"

echo "== E: a waterlogged stair is not a source (control) =="
expect_info 32 64 0 "empty"
stair=$(block_name 32 63 0)
case "$stair" in
    minecraft:oak_stairs*) ;;
    *) fail "the wrong-block control changed: got '$stair'" ;;
esac
echo "  stair 32,63,0 still $stair"

echo "== range: a larger range re-scans from the block below =="
say "rangedpumps range 16 64 0 2"
ctl wait 6
expect_info 16 64 0 "range 0 of 2"

echo "PASS: a still water source below was drained into the 32-bucket tank and became stone, a neighbouring cauldron took the tank's bucket, no-fluid / out-of-range / waterlogged-stair controls stayed empty, and the range command re-scanned"
