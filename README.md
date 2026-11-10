# rangedpumps (Horizon port)

A Horizon port of [rangedpumps](refinedmods/rangedpumps) — MIT.

**Status: first slices.** What is below is what the port actually does today.

## Implemented so far

- The pure arithmetic is `runtime.rs`; the world work is `support.rs`.
- Scan.** `range` starts at `-1` (the single column directly **below** the
- pump); each later range is the **perimeter** of a `(2r+3)²` square, i.e. the
- , 16, 24… block ring at horizontal distance `r+1`. Within a column the pump
- goes down one block per **8 ticks** (upstream `speed`) to the dimension's
- minimum Y, then starts the next column. The interior of a ring is not
- scanned (upstream behaviour).
- Drain.** Only `minecraft:water`/`minecraft:lava` with `level=0` (a still

## Not yet ported

- Implemented: the pump block, per-block range setting, the scan, still-source
- drain, the internal tank and its fluid capability, adjacent output, stone
- replacement, the readout, commands, and the carried item/recipe/loot/lang/
- models/textures.
- No energy (FE) gating.** The pin has API 142, but this campaign has no FE
- source and the slice brief permits energy as a gap; the pump runs as if
- `useEnergy = false`.
- No redstone pause.** A neighbour-signal read needs `LevelAccess`/`Ticks`,

## Install

Build the bundle with a native-mod capable launcher, or add the file from the
registry once it is listed. Pinned to Horizon `1d63e0a` and horizon-mixins
`5a64af93` for game 26.3.

## Build from source

```sh
cargo test
python3 tools/horizon.py check --horizon <horizon checkout>
python3 tools/horizon.py build --horizon <horizon checkout>
python3 tools/package.py
```

## Credits and licence

Original mod: refinedmods/rangedpumps. Port: AdLf0208. Licence: MIT — see [LICENSE](LICENSE);
the original notices are preserved.
