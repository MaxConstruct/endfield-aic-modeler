#!/usr/bin/env bash
# Fails when a crate has a dependency it must not have (ADR 0009). The rules are the table in
# docs/TASK_GUI_ICED.md section 3:
#
#   aic-data       no other aic crate, no GUI crate
#   aic-core       no aic-editor, no UI crate, no GUI crate
#   aic-editor     no UI crate, no GUI crate (it stays wasm-clean and toolkit-free)
#   aic-app-egui   no iced
#   aic-app-iced   no egui, no eframe
#
# "GUI crate" is egui*, eframe, iced*, wgpu, winit and rfd. Normal, build and dev dependencies all
# count, so a test-only egui_kittest in aic-editor fails too, and so do dependencies of other
# targets (wasm32) and of optional features. A crate that doesn't exist yet
# (aic-app-iced, before task B1) is skipped.
#
#   scripts/check-frontends.sh
set -euo pipefail
# The forbidden-name patterns are globs to match against names, never against files.
set -f

cd "$(dirname "$0")/.."

# Crates no lower layer may depend on, and the toolkits.
ui_crates='aic-editor aic-app-egui aic-app-iced'
egui_crates='egui egui-* egui_* eframe'
iced_crates='iced iced_* iced-*'
gui_crates="$egui_crates $iced_crates wgpu winit rfd"

# The crates the named crate must not depend on.
forbidden_for() {
    case "$1" in
        aic-data) echo "aic-core $ui_crates $gui_crates" ;;
        aic-core) echo "$ui_crates $gui_crates" ;;
        aic-editor) echo "aic-app-egui aic-app-iced $gui_crates" ;;
        aic-app-egui) echo "aic-app-iced $iced_crates" ;;
        aic-app-iced) echo "aic-app-egui $egui_crates" ;;
    esac
}

failed=0
for crate in aic-data aic-core aic-editor aic-app-egui aic-app-iced; do
    if [ "$crate" = aic-app-iced ] && [ ! -d crates/aic-app-iced ]; then
        echo "skip  $crate (not created yet)"
        continue
    fi
    tree="$(cargo tree -p "$crate" -e normal,dev,build --target all --all-features --prefix none --color never)"
    # The crate names, one per line: the first word of each line of the tree.
    names="$(printf '%s\n' "$tree" | awk '{print $1}' | sort -u)"
    bad=""
    for pattern in $(forbidden_for "$crate"); do
        # shellcheck disable=SC2254  # the patterns are globs on purpose
        for name in $names; do
            case "$name" in
                $pattern) bad="$bad $name" ;;
            esac
        done
    done
    if [ -n "$bad" ]; then
        for name in $(printf '%s\n' $bad | sort -u); do
            echo "FAIL  $crate depends on $name" >&2
        done
        failed=1
    else
        echo "ok    $crate"
    fi
done
exit "$failed"
