#!/usr/bin/env bash
# Load browser-extension/.env into the CURRENT shell for the signing recipes.
# Sourced (`. packaging/load-ext-env.sh`), never executed — the exports must
# reach the recipe's own shell so `web-ext sign` inherits them.
#
# Parsing is deliberately dumb and safe: plain KEY=VALUE lines only. Values
# are exported literally (the expansion is never re-parsed by the shell, so
# nothing in the file can execute), unknown keys and comments are ignored,
# and a variable already exported by the caller wins (dotenv precedence — an
# explicit export is never silently overridden by the file).

env_file="${EXT_ENV_FILE:-browser-extension/.env}"
if [ ! -f "$env_file" ]; then
    return 0 2>/dev/null || exit 0
fi

# The file must not be world-readable — refuse to proceed otherwise, rather
# than sign with credentials that other local users could have read.
perms="$(stat -c '%a' "$env_file")"
if [ "$perms" != "600" ] && [ "$perms" != "400" ]; then
    echo "error: $env_file must be chmod 600 (currently $perms)" >&2
    return 1 2>/dev/null || exit 1
fi

while IFS='=' read -r key value; do
    case "$key" in
        '' | '#'*) continue ;;
    esac
    # Strip a trailing CR left by Windows line endings (CRLF files otherwise
    # leak \r into the value and produce confusing 401s at AMO).
    cr="$(printf '\r')"
    value="${value%"$cr"}"
    # Setness test, not emptiness: an explicitly exported empty value still
    # wins over the file (dotenv precedence) and makes the creds check fail —
    # it must not be silently backfilled from .env. The guard is an `if`
    # condition (POSIX-exempt from `set -e`) rather than an `&&` list: a
    # failed `&&` guard inside the while body returns 1, and under strict
    # errexit shells (dash, Ubuntu CI's /bin/sh) that could abort the whole
    # recipe mid-file.
    case "$key" in
        WEB_EXT_API_KEY)
            if [ "${WEB_EXT_API_KEY+x}" != "x" ]; then export WEB_EXT_API_KEY="$value"; fi
            ;;
        WEB_EXT_API_SECRET)
            if [ "${WEB_EXT_API_SECRET+x}" != "x" ]; then export WEB_EXT_API_SECRET="$value"; fi
            ;;
        EXT_UPDATE_BASE_URL)
            if [ "${EXT_UPDATE_BASE_URL+x}" != "x" ]; then export EXT_UPDATE_BASE_URL="$value"; fi
            ;;
        EXT_SIGN_TIMEOUT_MS)
            if [ "${EXT_SIGN_TIMEOUT_MS+x}" != "x" ]; then export EXT_SIGN_TIMEOUT_MS="$value"; fi
            ;;
    esac
done < "$env_file"

# The loop's exit status mirrors its last case arm (1 when the setness
# precedence guard skipped an already-set variable) — always return success
# so a `set -e` recipe shell never aborts on that.
return 0 2>/dev/null || exit 0
