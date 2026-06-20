# dist/ — Artefactos de despliegue de CaudalGhost (F5)

Ficheros de empaquetado/servicio (entregables, van en el repo). NO son código.

| Fichero | Se instala en | Qué hace |
|---|---|---|
| `systemd/caudalghostd.service` | `/usr/lib/systemd/system/` | Corre el daemon como servicio de sistema, usuario `caudalghost`, con `CAP_BPF`/`CAP_PERFMON` vía `AmbientCapabilities` (sin sudo), endurecido y compatible con eBPF. `Type=notify` + `WatchdogSec`. |
| `sysusers.d/caudalghost.conf` | `/usr/lib/sysusers.d/` | Crea el usuario/grupo de sistema `caudalghost` al instalar (vía `systemd-sysusers`). |
| `polkit/10-caudalghost-resolved.rules` | `/usr/share/polkit-1/rules.d/` | Autoriza al usuario `caudalghost` la acción `org.freedesktop.resolve1.subscribe-query-results` para que F4 resuelva hostnames sin sudo. |

La instalación (crear usuario, copiar unit, `daemon-reload`, `enable`, añadir el
usuario de escritorio al grupo `caudalghost`) la hacen los paquetes de los
Planes 3 (.deb) y 4 (Arch/AUR). El runtime no usa `sudo`; root solo se necesita
una vez, al instalar el paquete.

## Validación
- `systemd-analyze verify systemd/caudalghostd.service` (sin errores; el aviso de `ExecStart` no instalado es esperado fuera del sistema empaquetado).
- `systemd-analyze security --offline=true systemd/caudalghostd.service` (puntaje de exposición).
- `node --check polkit/10-caudalghost-resolved.rules` (sintaxis JS).
