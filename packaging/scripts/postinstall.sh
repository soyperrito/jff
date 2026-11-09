#!/bin/sh
# Initialize ~/.jff for existing users and /etc/skel for future users.
# Package scripts run as root; user data is never overwritten.

set -u

SYSTEM_THEMES="/usr/local/share/jff/themes"
DEFAULT_CONFIG="/usr/local/share/jff/config.default.json"

init_user() {
    user="$1"
    home="$2"

    [ -n "$user" ] || return 0
    [ -n "$home" ] || return 0
    [ "$home" != "/" ] || return 0
    [ -d "$home" ] || return 0
    [ "$user" = "nobody" ] && return 0

    jff_dir="$home/.jff"
    mkdir -p "$jff_dir/themes" 2>/dev/null || return 0

    if [ ! -f "$jff_dir/config.json" ] && [ -f "$DEFAULT_CONFIG" ]; then
        cp "$DEFAULT_CONFIG" "$jff_dir/config.json" 2>/dev/null || true
    fi

    if [ -d "$SYSTEM_THEMES" ]; then
        for theme in "$SYSTEM_THEMES"/*.json; do
            [ -e "$theme" ] || continue
            name=${theme##*/}

            if [ ! -f "$jff_dir/themes/$name" ]; then
                cp "$theme" "$jff_dir/themes/$name" 2>/dev/null || true
            fi
        done
    fi

    group=$(id -gn "$user" 2>/dev/null || echo "$user")
    chown -R "$user:$group" "$jff_dir" 2>/dev/null \
        || chown -R "$user" "$jff_dir" 2>/dev/null \
        || true
}

# The user that invoked the package manager, when available.
if [ "${SUDO_USER:-}" ] && [ "$SUDO_USER" != "root" ]; then
    home=$(getent passwd "$SUDO_USER" 2>/dev/null | cut -d: -f6)
    init_user "$SUDO_USER" "$home"
elif [ "$(id -u)" -eq 0 ]; then
    init_user root /root
fi

# Existing regular users.
if command -v getent >/dev/null 2>&1; then
    getent passwd 2>/dev/null | while IFS=: read -r user _ uid _ _ home shell; do
        [ "$uid" -ge 1000 ] 2>/dev/null || continue
        case "$shell" in
            */nologin|*/false) continue ;;
        esac
        init_user "$user" "$home"
    done
fi

# Skeleton for users created after package installation.
if [ -d /etc/skel ]; then
    mkdir -p /etc/skel/.jff/themes 2>/dev/null || true

    if [ ! -f /etc/skel/.jff/config.json ] && [ -f "$DEFAULT_CONFIG" ]; then
        cp "$DEFAULT_CONFIG" /etc/skel/.jff/config.json 2>/dev/null || true
    fi

    for theme in "$SYSTEM_THEMES"/*.json; do
        [ -e "$theme" ] || continue
        name=${theme##*/}

        if [ ! -f "/etc/skel/.jff/themes/$name" ]; then
            cp "$theme" "/etc/skel/.jff/themes/$name" 2>/dev/null || true
        fi
    done
fi

exit 0
