#!/usr/bin/env bash
# Package both artifacts, from the port's own sources:
#  - the `.hmod` in content/ via the pin's cargo-horizon-mod (build + check)
#  - a copy at mixins/rangedpumps/data/rangedpumps-content.hmod, which the bundle compiles in
#  - the source-only `.hnmod` in the port root (tools/package.py, deterministic bytes)
# Run inside a build slot (it compiles the packaging tool):
#   ports/tools/build-slot.sh run --job rangedpumps-pack --port ports/rangedpumps \
#       --step package -- env CARGO_INCREMENTAL=0 tools/package.sh
set -euo pipefail

PORT=$(cd "$(dirname "$0")/.." && pwd)
HORIZON=${HORIZON:-${CAMPAIGN_ROOT:-$(cd "$(dirname "$0")/../../.." && pwd)}/ports/_shared/pins/horizon-1d63e0a}
LOGS=$PORT/target/ladder
mkdir -p "$LOGS"
cd "$PORT"

CARGO_TARGET_DIR=$PORT/target/carry-tools
export CARGO_TARGET_DIR
(cd content && cargo run --locked --offline --quiet \
    --manifest-path "$HORIZON/crates/cargo-horizon-mod/Cargo.toml" -- horizon-mod build) \
    2>&1 | tee "$LOGS/package-hmod.log"
cp content/target/horizon-mod/rangedpumps.hmod content/rangedpumps.hmod
(cd content && cargo run --locked --offline --quiet \
    --manifest-path "$HORIZON/crates/cargo-horizon-mod/Cargo.toml" -- horizon-mod check rangedpumps.hmod) \
    2>&1 | tee -a "$LOGS/package-hmod.log"

# The bundle's data/ carries the built content; package.py refuses a stale copy.
mkdir -p mixins/rangedpumps/data
cp content/rangedpumps.hmod mixins/rangedpumps/data/rangedpumps-content.hmod
python3 tools/package.py | tee "$LOGS/package-hnmod.log"

echo "== artifacts =="
for f in content/rangedpumps.hmod rangedpumps.hnmod; do
    [ -f "$f" ] || { echo "missing $f"; exit 1; }
    size=$(stat -c %s "$f")
    sha=$(sha256sum "$f" | cut -d' ' -f1)
    entries=$(python3 -c "import zipfile,sys; print(len(zipfile.ZipFile(sys.argv[1]).namelist()))" "$f")
    echo "$f size=$size sha256=$sha entries=$entries"
done
