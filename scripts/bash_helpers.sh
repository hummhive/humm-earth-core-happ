#!/usr/bin/env bash
# Shared bash helpers for humm-earth-core-happ dev tooling.
#
# Source this from any other bash script in the repo:
#
#   SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
#   # shellcheck source=scripts/bash_helpers.sh
#   source "$SCRIPT_DIR/bash_helpers.sh"
#
# Curated subset of Mike's broader bash_helpers.sh (battle-tested in his
# personal cmder env). Kept narrow on purpose: only what the in-repo
# scripts actually use, so adding a helper here is a deliberate choice
# rather than dragging the whole 1k-line library along.

# Don't expand `!` in history when sourced; quirky cmder behavior.
set +o histexpand

# ---------------------------------------------------------------------------
# Stacktrace on unexpected errors
# ---------------------------------------------------------------------------
# `bash_traceback` walks BASH_SOURCE / BASH_LINENO / FUNCNAME to print a
# call stack pointing at the failing command. Adapted from Mike's personal
# bash_helpers.sh. Two ways to use it:
#
#   1. As an explicit panic: call directly when you detect a fatal
#      condition that shouldn't normally happen (e.g. a contract
#      violation in a helper). Exits with code 255.
#
#   2. As an ERR trap: `trap 'bash_traceback' ERR` + `set -o errtrace`
#      will fire it on every non-zero exit, but ONLY do this in scripts
#      that are clean under `set -e`. Many of our scripts intentionally
#      run commands expected to fail (e.g. `grep -v` returning 1 when
#      no lines are filtered), so the global trap pattern doesn't fit
#      everywhere.
bash_traceback() {
    local lasterr="$?"
    set +o xtrace
    local bash_command=${BASH_COMMAND}
    echo_stderr "Error in ${BASH_SOURCE[1]:-<unknown>}:${BASH_LINENO[0]:-?} ('$bash_command' exited with status $lasterr)"
    if [ ${#FUNCNAME[@]} -gt 2 ]; then
        echo_stderr "Traceback of ${BASH_SOURCE[1]:-<unknown>} (most recent call last):"
        local i funcname
        for ((i=0; i < ${#FUNCNAME[@]} - 1; i++)); do
            funcname="${FUNCNAME[$i]}"
            [ "$i" -eq "0" ] && funcname=$bash_command
            echo_stderr "  $i: ${BASH_SOURCE[$i+1]:-<unknown>}:${BASH_LINENO[$i]:-?}  $funcname"
        done
    fi
    exit 255
}

# ---------------------------------------------------------------------------
# Colored / timestamped echo
# ---------------------------------------------------------------------------
# Use `/bin/echo` directly (not the bash builtin) because cmder's builtin
# doesn't always honor `-e` consistently across versions.
echo_red() {
    /bin/echo -e "\e[1;31m$*\e[0m"
}
echo_blue() {
    /bin/echo -e "\e[1;34m$*\e[0m"
}
echo_yellow() {
    /bin/echo -e "\e[1;33m$*\e[0m"
}
echo_green() {
    /bin/echo -e "\e[1;32m$*\e[0m"
}
echo_gray() {
    /bin/echo -e "\e[1;90m$*\e[0m"
}

echo_stderr() {
    # shellcheck disable=2068
    >&2 echo $@
}

echo_stderr_red() {
    >&2 echo_red "$@"
}

echo_stderr_yellow() {
    >&2 echo_yellow "$@"
}

# ISO 8601 UTC timestamps for easy log-file alignment.
echo_timestamp() {
    echo "$(TZ=":UTC" date +"%FT%H:%M:%S.%3NZ") $*"
}

echo_stderr_with_timestamp() {
    >&2 echo "$(TZ=":UTC" date +"%FT%H:%M:%S.%3NZ") $*"
}

get_date_safe_for_filenames() {
    date +"%Y%m%d-%H%M%S"
}

# ---------------------------------------------------------------------------
# Environment probes
# ---------------------------------------------------------------------------
is_command_present() {
    type "$1" >/dev/null 2>&1
}

is_linux() {
    [[ "$(uname -s)" = "Linux" ]]
}

is_macos() {
    [[ "$(uname -s)" = "Darwin" ]]
}

is_wsl() {
    is_linux && grep -qi microsoft /proc/version 2>/dev/null
}

# ---------------------------------------------------------------------------
# WSL ↔ Windows-mount patch workflow
# ---------------------------------------------------------------------------
# create_full_patch: stage all working-tree changes, write them to a .patch
# file, then reset the index — leaving the working tree untouched.
#
# Intended use: working in a fast native-Linux copy of the repo
# (~/humm-earth-core-happ) and syncing solidified changes back to the
# Windows-mounted copy for committing.
#
#   # In ~/humm-earth-core-happ (Linux FS):
#   create_full_patch ~/Desktop
#
#   # In /mnt/c/proj/github/hummhive/humm-earth-core-happ (Windows mount):
#   git apply ~/Desktop/humm-earth-core-happ-2026-05-14.patch
#
# Safe for macOS / Linux devs who never use a separate copy: the function is
# only invoked explicitly and has no side effects when not called.
#
# Arguments:
#   $1  destination folder  (default: ~/Desktop)
#   $2  filename            (default: <project>-<date>.patch)
create_full_patch() {
    local -r _projectName=$(basename "$(git rev-parse --show-toplevel)")
    local -r _destinationFolder=${1:-"${HOME}/Desktop"}
    local -r _destinationFilename=${2:-"${_projectName}-$(get_date_safe_for_filenames).patch"}
    local -r _fullDestinationPath="${_destinationFolder}/${_destinationFilename}"

    git add . && git diff --cached > "${_fullDestinationPath}"

    if [[ $? -eq 0 ]]; then
        echo_blue "Successfully created patch: ${_fullDestinationPath}"
        git reset HEAD .
    else
        echo_red "Failed to create patch."
        return 1
    fi

    echo_gray "Current status:"
    git status
}
