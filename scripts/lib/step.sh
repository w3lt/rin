#!/usr/bin/env bash

step() {
    local name="$1"
    shift
    if "$@" >"$log" 2>&1; then
        echo "ok    $name"
    else
        printf 'FAIL %s:' "$name"
        printf ' %q' "$@"
        echo
        local half=$((lines / 2))
        head -n "$half" "$log"
        tail -n "$half" "$log"
        local total
        total="$(wc -l <"$log" | tr -d ' ')"
        if [ "$total" -gt "$lines" ]; then
            echo "... ($((total - lines)) more lines; rerun the command above for all of it)"
        fi
        exit 1
    fi
}
