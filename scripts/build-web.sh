#!/usr/bin/env bash
# Builds the web version into target/web/: index.html (with its loading page), the JS glue, the
# .wasm (which holds the game data and the names, ADR 0012), the icon sheet, as AVIF, which the
# page decodes (the desktop builds in a PNG; PLAN.md, M13 step 5), and the favicons and link
# preview thumbnail from crates/aic-app/assets/web/. Serve that folder from any static web server.
#
#   scripts/build-web.sh          # release build into target/web/
#   scripts/build-web.sh --serve  # debug build, served at http://127.0.0.1:8080 with live reload
#   AIC_SITE_URL=https://example.org/aic scripts/build-web.sh  # with the site's address
#
# AIC_SITE_URL is the address the site will be served from. Link previews (chat apps, social
# sites) need absolute URLs, so scripts/web-assets.sh writes it into the page's og:url, og:image
# and twitter:image. Without it the build still works from any folder, but those tags are left
# out and the preview has no thumbnail. A value that isn't a plain http(s) address stops the build.
#
# Needs `rustup target add wasm32-unknown-unknown`, `cargo install --locked trunk` and
# `cargo install --locked wasm-bindgen-cli --version 0.2.130` (the version in Cargo.lock). Trunk
# runs scripts/web-assets.sh after each build (Trunk.toml), which needs Git Bash's sh and bash on
# Windows: run this script from Git Bash. `cargo install --locked wasm-opt` too (binaryen 133,
# Trunk.toml's [tools]), for the release build's wasm-opt pass; the build downloads no tools.
#
# Deploying (PLAN.md, M13 step 5): upload everything in target/web/ (the page, the JS glue, the
# .wasm, icons-<n>-<hash>.avif, the favicons AppIcon-Windows-<hash>.svg and favicon-<hash>.ico,
# apple-touch-icon-<hash>.png and the link-preview thumbnail og-image-<hash>.png), replacing the
# old files; the old hashed ones may be deleted after. Every file but index.html has a hash of its content in
# its name, so:
#   - index.html must not be cached for long (or revalidated each time), so a new upload is seen;
#   - the other files can be cached for good (`Cache-Control: public, max-age=31536000,
#     immutable`), since a changed file has a new name.
# The server must send the .wasm as `application/wasm`, or the browser won't compile it as it
# downloads. Behind Cloudflare, check that it caches the .wasm (it may need a Cache Rule: .wasm may
# not be among the extensions it caches by default) and compresses it (brotli or gzip on the
# `application/wasm` type), and that the AVIF sheet is cached (it is compressed already: no need to
# compress it again). The build compresses nothing itself.
set -euo pipefail

cd "$(dirname "$0")/../crates/aic-app"

case "${1:-}" in
    --serve) exec trunk serve ;;
    "") exec trunk build --release ;;
    *) echo "unknown option: $1" >&2; exit 2 ;;
esac
