#!/usr/bin/env bash
# Trunk's post-build hook (crates/aic-app/Trunk.toml), run by `trunk build` and `trunk serve`:
#
# 1. Copies the icon sheets that crates/aic-app/build.rs packed for this wasm build into the site:
#    AVIF, which the loading page decodes (PLAN.md, M13 step 5), each with a hash of its content
#    in its name (icons-0-<hash>.avif), so a new upload is never served stale from a cache.
# 2. Writes into the page the .wasm's and the sheets' names and sizes, which the loading
#    page counts its bar against: a compressed download doesn't say its length (PLAN.md, M13).
# 3. Copies the link-preview thumbnail and the apple-touch-icon (crates/aic-app/assets/web/),
#    which Trunk has no pipeline for, under hashed names too, and writes those names into the page.
#    The preview's crawlers need absolute URLs, so with AIC_SITE_URL (the site's address, such as
#    https://example.org/aic) the page's og:url, og:image and twitter:image are filled in from
#    it; without it those tags are taken out and the card asked for is the small one, with no
#    thumbnail. A value that isn't a plain http(s) address stops the build.
#
# Runs from crates/aic-app with Trunk's TRUNK_PROFILE, TRUNK_HTML_FILE and TRUNK_STAGING_DIR, and
# the environment Trunk was started with (AIC_SITE_URL). Needs bash, cargo, sha256sum and sed;
# scripts/build-web.sh runs Trunk from Git Bash on Windows.
set -euo pipefail

staging="${TRUNK_STAGING_DIR:?run by Trunk}"
page="$staging/index.html"

# Checked first, so a bad address stops the hook before it writes anything (Trunk then leaves the
# last good build in place). Only characters that are safe
# both in the sed replacements below (which use `|`) and in a double-quoted HTML attribute: no
# quotes, `&`, `<`, `>`, `|`, backslash or whitespace.
site="${AIC_SITE_URL:-}"
if [ -n "$site" ]; then
    url_pattern='^https?://[A-Za-z0-9.-]+(:[0-9]+)?(/[A-Za-z0-9._~:/?#@!$()*+,;=%-]*)?$'
    if ! [[ "$site" =~ $url_pattern ]]; then
        echo "web-assets.sh: AIC_SITE_URL must be an http:// or https:// address made of letters," \
            "digits and ._~:/?#@!\$()*+,;=%- only (no quotes, &, <, >, |, backslash or spaces);" \
            "got '$site'" >&2
        exit 1
    fi
    while [[ "$site" == */ ]]; do
        site="${site%/}"
    done
fi

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

# Hashed like the sheets, and named the same way: og-image-<hash>.png.
hashed_copy() {
    local hash name
    hash="$(sha256sum "$1" | cut -c1-16)"
    name="$(basename "$1" .png)-$hash.png"
    cp "$1" "$staging/$name"
    echo "$name"
}
touch_icon="$(hashed_copy assets/web/apple-touch-icon.png)"
thumbnail="$(hashed_copy assets/web/og-image.png)"
preview=(
    -e "s|<link rel=\"apple-touch-icon\" href=\"[^\"]*\"|<link rel=\"apple-touch-icon\" href=\"$touch_icon\"|"
    # Trunk hashes the two icons but drops their `sizes` and `type`. Without them a browser can't
    # tell which icon it can use: the size has Chrome take the SVG over the .ico, and the type
    # lets a browser without SVG icons skip the SVG.
    -e 's|\(<link rel="icon" href="[^"]*\.ico"\)|\1 sizes="48x48"|'
    -e 's|\(<link rel="icon" href="[^"]*\.svg"\)|\1 type="image/svg+xml"|'
)
if [ -n "$site" ]; then
    preview+=(
        -e "s|\(<meta property=\"og:url\" content=\"\)[^\"]*\"|\1$site/\"|"
        -e "s|\(<meta property=\"og:image\" content=\"\)[^\"]*\"|\1$site/$thumbnail\"|"
        -e "s|\(<meta name=\"twitter:image\" content=\"\)[^\"]*\"|\1$site/$thumbnail\"|"
    )
    echo "web-assets.sh: link preview for $site/, thumbnail $thumbnail"
else
    # Each of these tags is on a line of its own in index.html.
    preview+=(
        -e '/<meta property="og:url"/d'
        -e '/<meta property="og:image/d'
        -e '/<meta name="twitter:image/d'
        -e 's|\(<meta name="twitter:card" content="\)[^"]*"|\1summary"|'
    )
    echo "web-assets.sh: no thumbnail in the link preview: AIC_SITE_URL is not set"
fi
sed -i "${preview[@]}" "$page"
