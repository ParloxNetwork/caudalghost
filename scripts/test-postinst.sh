#!/usr/bin/env bash
# Test del guard de seguridad de debian/postinst (H-5): $SUDO_USER (influenciable
# por quien instala, y postinst corre como root) debe sanearse antes de pasarse a
# `usermod`, sin inyección de opciones, y un usuario válido debe seguir añadiéndose
# al grupo. Usa stubs de usermod/getent en PATH para capturar las invocaciones.
set -u
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
POSTINST="$ROOT/crates/caudalghostd/debian/postinst"

# Ejecuta el postinst real con un SUDO_USER dado; imprime lo que recibió usermod.
run_case() {
  local sudo_user="$1"
  local tmp log
  tmp="$(mktemp -d)"
  log="$tmp/usermod.log"
  cat > "$tmp/usermod" <<EOF
#!/bin/sh
printf '%s\n' "\$*" >> "$log"
EOF
  cat > "$tmp/getent" <<'EOF'
#!/bin/sh
exit 0
EOF
  # logname vacío: aísla el caso SUDO_USER='' del entorno real (sin fallback).
  cat > "$tmp/logname" <<'EOF'
#!/bin/sh
exit 1
EOF
  chmod +x "$tmp/usermod" "$tmp/getent" "$tmp/logname"
  : > "$log"
  SUDO_USER="$sudo_user" PATH="$tmp:$PATH" sh "$POSTINST" configure >/dev/null 2>&1
  cat "$log"
  rm -rf "$tmp"
}

fail=0
check_empty() { # $1=descripcion $2=salida
  if [ -z "$2" ]; then echo "OK   $1"; else echo "FAIL $1 — usermod recibió: [$2]"; fail=1; fi
}
check_has() {   # $1=descripcion $2=salida $3=patron
  if printf '%s' "$2" | grep -q -- "$3"; then echo "OK   $1"; else echo "FAIL $1 — usermod recibió: [$2]"; fail=1; fi
}

check_has   "usuario válido se añade con guard '--'"        "$(run_case 'alice')"        '-aG caudalghost -- alice'
check_has   "usuario empresarial @ (Kerberos/SSSD) aceptado" "$(run_case 'jdoe@corp.example.com')" '-- jdoe@corp.example.com'
check_empty "SUDO_USER con pinta de opción (-p) rechazado"  "$(run_case '-p toor root')"
check_empty "SUDO_USER con metacaracteres rechazado"        "$(run_case 'a;b$(id)')"
check_empty "SUDO_USER vacío no invoca usermod"             "$(run_case '')"

[ "$fail" -eq 0 ] && echo "== test-postinst: TODO VERDE ==" || echo "== test-postinst: FALLOS =="
exit "$fail"
