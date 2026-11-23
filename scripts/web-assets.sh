#!/usr/bin/env bash
# Trunk's post-build hook (crates/aic-app/Trunk.toml), run by `trunk build` and `trunk serve`:
#
# 1. Copies the icon sheets that crates/aic-app/build.rs packed for this wasm build into the site:
#    AVIF, which the loading page decodes (PLAN.md, M13 step 5), each with a hash of its content
#    in its name (icons-0-<hash>.avif), so a new upload is never served stale from a cache.
# 2. Writes into the page the .wasm's and the sheets' names and sizes, which the loading
#    page counts its bar against: a compressed download doesn't say its length (PLAN.md, M13).
#
# Runs from crates/aic-app with Trunk's TRUNK_PROFILE, TRUNK_HTML_FILE and TRUNK_STAGING_DIR. Needs
# bash, cargo, sha256sum and sed; scripts/build-web.sh runs Trunk from Git Bash on Windows.
set -euo pipefail

staging="${TRUNK_STAGING_DIR:?run by Trunk}"
page="$staging/index.html"

# The same cargo build Trunk ran, a no-op now, tells where aic-app's build script wrote.
# Its profile and features are the page's, as Trunk read them: other features would be another
# OUT_DIR, and without `avif-sheets` build.rs writes no sheet for the web.
profile_args=()
cargo_features="$(sed -n 's/.*data-cargo-features="\([^"]*\)".*/\1/p' "${TRUNK_HTML_FILE:?}" | head -n 1)"
if [ -n "$cargo_features" ]; then
    profile_args=(--features "$cargo_features")
fi
cargo_profile="$(sed -n 's/.*data-cargo-profile="\([^"]*\)".*/\1/p' "${TRUNK_HTML_FILE:?}" | head -n 1)"
if [ -n "$cargo_profile" ]; then
    profile_args+=(--profile "$cargo_profile")
elif [ "${TRUNK_PROFILE:-debug}" = release ]; then
    profile_args+=(--release)
fi
out_dir="$(
    cargo build --target=wasm32-unknown-unknown --manifest-path Cargo.toml --bin aic-app \
        "${profile_args[@]}" --message-format=json 2>/dev/null |
        grep '"reason":"build-script-executed"' |
        grep -E '"package_id":"[^"]*aic-app[#@]' |
        sed -n 's/.*"out_dir":"\([^"]*\)".*/\1/p' |
        tail -n 1
)"
out_dir="${out_dir//\\\\/\/}"
if [ ! -f "$out_dir/icons-0.avif" ]; then
    echo "web-assets.sh: no AVIF icon sheet in aic-app's OUT_DIR ('$out_dir');" \
        "is avif-sheets in index.html's data-cargo-features?" >&2
    exit 1
fi

sheets=""
for sheet in "$out_dir"/icons-*.avif; do
    hash="$(sha256sum "$sheet" | cut -c1-16)"
    name="$(basename "$sheet" .avif)-$hash.avif"
    cp "$sheet" "$staging/$name"
    sheets="$sheets${sheets:+ }$name:$(wc -c < "$sheet" | tr -d ' ')"
done

wasm="$(ls "$staging"/*_bg.wasm)"
wasm="$(basename "$wasm"):$(wc -c < "$wasm" | tr -d ' ')"
sed -i \
    -e "s|data-wasm=\"[^\"]*\"|data-wasm=\"$wasm\"|" \
    -e "s|data-sheets=\"[^\"]*\"|data-sheets=\"$sheets\"|" \
    "$page"
echo "web-assets.sh: .wasm $wasm; sheets $sheets"
