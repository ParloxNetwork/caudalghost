#!/bin/sh
set -e

# Añade al usuario que instala al grupo `caudalghost` para alcanzar el socket
# 0660 del daemon sin sudo. Necesita re-login para tomar el grupo.
if [ "$1" = "configure" ] || [ "$1" = "" ]; then
    target_user="${SUDO_USER:-$(logname 2>/dev/null || true)}"
    if [ -n "$target_user" ] && getent group caudalghost >/dev/null 2>&1; then
        usermod -aG caudalghost "$target_user" || true
        echo "CaudalGhost: '$target_user' añadido al grupo 'caudalghost'. Cierra sesión y vuelve a entrar para que la GUI conecte (sin sudo)."
    fi
fi

exit 0
