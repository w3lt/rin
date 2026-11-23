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
        total="$(wc -l <"$log" | tr -d ' ')"
        if [ "$total" -le "$lines" ]; then
            cat "$log"
        else
            head -n "$half" "$log"
            echo "... ($((total - 2 * half)) lines omitted; rerun the command above for all of it)"
            tail -n "$half" "$log"
        fi
        exit 1
    fi
}
