# The web build (scripts/build-web.sh) as a container that serves it: a Rust stage runs Trunk and
# precompresses the site, and a Caddy stage serves it on port 8080 as a non-root user, with the
# cache and compression rules from the top of scripts/build-web.sh (deploy/Caddyfile).
#
#   podman build -t aic-modeler --build-arg AIC_SITE_URL=https://example.org/aic .
#   podman run --rm -p 8080:8080 aic-modeler       # then open http://127.0.0.1:8080
#
# `docker` takes the same arguments. AIC_SITE_URL is optional: without it the link preview has no
# thumbnail (scripts/web-assets.sh). BUILD_JOBS caps how many cores the build uses, for a server
# that is short of memory or busy with something else. Written for both Docker (BuildKit) and Podman (Buildah): no heredocs,
# no COPY --chmod, no `# syntax=` line. The cache mounts (which need BuildKit, Docker's default
# builder) only speed a build up: with an empty cache it builds the same site.

FROM docker.io/library/rust:1.99.1-trixie AS builder

# brotli to precompress the site; the image has the C++ compiler the wasm-opt crate needs.
RUN apt-get update \
    && apt-get install -y --no-install-recommends brotli \
    && rm -rf /var/lib/apt/lists/*

# Caps cargo's parallel jobs (and binaryen's C++ compiles under it) when set: fewer jobs, fewer
# busy cores and less memory at once.
ARG BUILD_JOBS=

# The web build's tools at the versions Trunk.toml's [tools] asks for (wasm-bindgen-cli matches
# Cargo.lock; wasm-opt 0.133.0 is binaryen version_133), so Trunk finds them on PATH and downloads
# nothing. A layer of their own, before the source, so a source change doesn't rebuild them.
RUN if [ -n "$BUILD_JOBS" ]; then export CARGO_BUILD_JOBS="$BUILD_JOBS"; fi \
    && rustup target add wasm32-unknown-unknown \
    && cargo install --locked trunk --version 0.22.2 \
    && cargo install --locked wasm-bindgen-cli --version 0.2.130 \
    && cargo install --locked wasm-opt --version 0.133.0 \
    && rm -rf /usr/local/cargo/registry

WORKDIR /src
COPY . .

# Declared here, not above, so a new address rebuilds only the site, never the tools.
ARG AIC_SITE_URL=

# A Windows checkout with core.autocrlf has CRLF scripts, which bash can't run: strip the CRs.
# target/ is a cache that outlives the build, and cargo trusts a file older than its last build
# as unchanged, while COPY keeps the context's times: touching the source has cargo rebuild the
# workspace crates every time, and only the crates from the registry come from the cache.
# The crates come first (cargo fetch); Trunk then runs offline, so a tool at the wrong version
# stops the build rather than being downloaded. Trunk's post-build hook (scripts/web-assets.sh)
# reads AIC_SITE_URL. BUILD_JOBS also caps the two steps cargo's jobs don't reach: the icon
# sheets' AVIF encoder in build.rs (rayon) and wasm-opt (binaryen). The site is copied out of the cached target/ into /site, then each text file
# gets a .br and a .gz beside it for Caddy's `precompressed`.
RUN --mount=type=cache,id=aic-cargo-registry,target=/usr/local/cargo/registry \
    --mount=type=cache,id=aic-target,target=/src/target,sharing=locked \
    sed -i 's/\r$//' scripts/*.sh \
    && find . -path ./target -prune -o -type f -exec touch {} + \
    && if [ -n "$BUILD_JOBS" ]; then \
        export CARGO_BUILD_JOBS="$BUILD_JOBS" RAYON_NUM_THREADS="$BUILD_JOBS" BINARYEN_CORES="$BUILD_JOBS"; \
    fi \
    && cargo fetch --locked \
    && cd crates/aic-app \
    && AIC_SITE_URL="$AIC_SITE_URL" TRUNK_OFFLINE=true trunk build --release --locked \
    && cp -r /src/target/web /site \
    && cd /site \
    && for file in *.wasm *.js *.html *.svg; do \
        brotli --best --keep "$file" && gzip --best --keep --no-name "$file"; \
    done \
    && ls -l /site

FROM docker.io/library/caddy:2.11.9-alpine

# A user of its own, and Caddy's config and data folders in its home, since the image's /config
# and /data belong to root. With no TLS Caddy stores nothing that matters, but it looks there.
RUN adduser -D -u 10001 -s /sbin/nologin web
ENV XDG_CONFIG_HOME=/home/web/config \
    XDG_DATA_HOME=/home/web/data

COPY deploy/Caddyfile /etc/caddy/Caddyfile
COPY --from=builder /site /srv

USER 10001
EXPOSE 8080
# Kept by Docker; Podman's default OCI image format drops it (it keeps it with --format docker).
HEALTHCHECK --interval=30s --timeout=5s --start-period=5s --retries=3 \
    CMD wget -q -O /dev/null http://127.0.0.1:8080/healthz || exit 1
# The image's own command: caddy run --config /etc/caddy/Caddyfile --adapter caddyfile.
