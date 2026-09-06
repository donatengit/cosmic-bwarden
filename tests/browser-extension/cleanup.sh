#!/bin/bash
# Shared teardown helpers for the browser-extension test scripts.
#
# Sourced (not executed) by run-e2e.sh, run-chrome-e2e.sh and minimal_verify.sh
# so the list of directories a test profile owns lives in exactly one place.
#
# SAFETY: cleanup_profile() deletes directories. Every path it removes is
# reconstructed from a profile name that must pass the guards below — it never
# takes a path from the caller. The live `cosmic-bwarden` profile has no
# `-<name>` suffix, so it is unreachable by construction, and the `test-`
# prefix check plus the separator/traversal checks make it unreachable by
# accident too. See docs/test_cleanup_plan.md.

# Remove the XDG directories belonging to one test profile.
#
# Usage: cleanup_profile test-extension-e2e
cleanup_profile() {
    local profile="$1"
    local uid target
    uid=$(id -u)

    # --- Guards. Any failure is a hard refusal, never a best-effort rm. ---
    if [ -z "$profile" ]; then
        echo "cleanup_profile: refusing to run with an empty profile name" >&2
        return 1
    fi
    case "$profile" in
        test-*) ;;
        *)
            echo "cleanup_profile: refusing to remove non-test profile '$profile'" \
                 "(test profiles must be named test-*)" >&2
            return 1
            ;;
    esac
    case "$profile" in
        */*|*..*)
            echo "cleanup_profile: refusing profile name with a path separator" \
                 "or traversal: '$profile'" >&2
            return 1
            ;;
    esac

    for target in \
        "${XDG_CONFIG_HOME:-$HOME/.config}/cosmic-bwarden-$profile" \
        "${XDG_CACHE_HOME:-$HOME/.cache}/cosmic-bwarden-$profile" \
        "${XDG_DATA_HOME:-$HOME/.local/share}/cosmic-bwarden-$profile" \
        "${XDG_RUNTIME_DIR:-/run/user/$uid}/cosmic-bwarden-$profile" \
        "${TMPDIR:-/tmp}/cosmic-bwarden-$profile-$uid"
    do
        # Belt and braces: never touch the live profile even if the guards
        # above were somehow bypassed.
        case "$(basename "$target")" in
            cosmic-bwarden|cosmic-bwarden-)
                echo "cleanup_profile: refusing to remove live profile dir $target" >&2
                continue
                ;;
        esac
        if [ -d "$target" ]; then
            rm -rf -- "$target"
        fi
    done
}

# Back up a file so it can be restored in a cleanup trap. Records the absence
# of the file too, so a file the test *creates* is removed rather than left.
#
# Usage: backup_file "$HOME/.mozilla/native-messaging-hosts/wrapper.sh"
backup_file() {
    local path="$1"
    [ -n "$path" ] || return 1
    if [ -e "$path" ]; then
        cp -p -- "$path" "$path.cbw-test-backup"
    else
        # Marker: the file did not exist before the test.
        : > "$path.cbw-test-absent"
    fi
}

# Restore what backup_file recorded.
restore_file() {
    local path="$1"
    [ -n "$path" ] || return 1
    if [ -e "$path.cbw-test-backup" ]; then
        mv -f -- "$path.cbw-test-backup" "$path"
    elif [ -e "$path.cbw-test-absent" ]; then
        rm -f -- "$path" "$path.cbw-test-absent"
    fi
}
