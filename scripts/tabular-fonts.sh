#!/usr/bin/env bash
# Makes the "Inter Display Tabular" cuts the app sets numbers in: Inter Display with its tabular
# figures (the `tnum` feature) frozen in as the default digits. egui shapes text without OpenType
# features, so the font itself has to carry them. Run it again after replacing an Inter Display file
# in crates/aic-editor/fonts/, and commit what it writes; the build only reads the result.
#
# Needs uv (https://docs.astral.sh/uv/), which fetches fonttools and opentype-feature-freezer for
# this one run.
set -euo pipefail

cd "$(dirname "$0")/../crates/aic-editor/fonts"

for weight in Light SemiBold; do
    uv run --quiet --with opentype-feature-freezer \
        pyftfeatfreeze --features tnum --suffix --usesuffix Tabular \
        "InterDisplay-$weight.ttf" "InterDisplayTabular-$weight.ttf"
    echo "fonts/InterDisplayTabular-$weight.ttf"
done
