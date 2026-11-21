#!/usr/bin/env sh

cd "${CLAUDE_PROJECT_DIR:-.}" || exit 0
[ "$(git config --get core.hooksPath)" = .githooks ] && exit 0

# RIN_AUTO_GITHOOKS is an environment variable set by developer
if [ "${RIN_AUTO_GITHOOKS:-}" = 1 ]; then
    git config core.hooksPath .githooks && exit 0
fi

msg='Git hooks are off in this clone, so commits skip scripts/verify. Enable them with: git config core.hooksPath .githooks'
printf '{"systemMessage":"%s", "hookSpecificOutput":{"hookEventName":"SessionStart","additionalContext":"%s Enabling them is up to the user: the agent tells them once and leaves git config alone."}}\n' "$msg" "$msg"
