# Modelo de amenazas · CaudalGhost

> Complementa [`SECURITY.md`](../SECURITY.md). Documento técnico para auditores y quien contribuye.
> CaudalGhost es un monitor de red **observacional** de un solo equipo: mide, nunca interfiere.

## Activos a proteger

1. **La posición privilegiada del daemon.** `caudalghostd` puede leer del kernel vía eBPF (`CAP_BPF`+`CAP_PERFMON`).
   Un compromiso del daemon es el peor caso; todo el diseño busca acotar su poder y su blast-radius.
2. **La telemetría.** Qué app/interfaz consumió cuánto y hacia qué host. Es información sensible de la actividad
   del equipo; debe quedar accesible solo a quien corresponde y **nunca salir de la máquina**.
3. **La base de datos** SQLite (`/var/lib/caudalghost/`) y **el socket** de control.

## Fronteras de confianza

```
kernel (eBPF)  ──►  caudalghostd (CAP_BPF/CAP_PERFMON, NO root, sandbox systemd)
                          │  socket Unix 0660 + ACL POSIX  (frontera principal)
                          ▼
             clientes SIN privilegios: CLI · TUI · GUI (corren como el usuario)
```
- **Frontera principal:** el daemon (privilegiado) vs. los clientes (sin privilegios), mediada por el socket
  Unix con permisos + ACL. Una petición de cliente jamás debe elevar privilegios ni tumbar el daemon.
- **La GUI** añade un motor WebKit, pero **no carga contenido remoto**: renderiza solo assets locales bajo una
  CSP restrictiva y habla con el daemon por IPC de Tauri (5 comandos de solo lectura).

## Actores y mitigaciones

| Actor / amenaza | Mitigación |
|---|---|
| **Usuario local sin acceso** intenta leer el socket | Socket `0660` + **ACL POSIX por UID de sesión activa**; edición servidor restringe a miembros del grupo `caudalghost` (opt-in explícito para todos). |
| **Cliente con acceso** manda peticiones malformadas/adversariales | Parser acotado (`MAX_LINE_BYTES` 64 KiB), tope de conexiones (128) + timeouts, `dispatch` exhaustivo → solo `Response::Error`, **SQL siempre parametrizado**. Un fallo cierra esa conexión, nunca el daemon. |
| **Peer no confiable** responde en el socket a un cliente | El cliente acota la respuesta (`MAX_RESPONSE_BYTES`) → no crece en memoria sin límite. |
| **Proceso** busca ser mal-atribuido | Atribución cgroup→app *best-effort*, funciones puras sin panic; ventana de reutilización de PID **aceptada y documentada**. |
| **Exfiltración / telemetría** desde el daemon | `RestrictAddressFamilies=AF_UNIX AF_NETLINK`: el daemon **no puede** abrir un socket de red. Cero egress. |
| **Cadena de suministro** (dependencia vulnerable) | `cargo-audit`/`cargo-deny` en CI, procedencia crates.io, dependencias mínimas; GUI aislada en su propio lock. |
| **GUI / WebKit** (XSS, contenido remoto) | Sin contenido remoto, CSP estricta, sin plugins nativos, 5 comandos de solo lectura, `innerHTML`/`eval` erradicados. |
| **Escalada tras comprometer el daemon** | `NoNewPrivileges`, `CapabilityBoundingSet` mínimo, sandbox systemd endurecido (ProtectSystem=strict, PrivateDevices, RemoveIPC…). El daemon comprometido gana "leer eBPF", no root. |

## Fuera de alcance (no-objetivos)

- **Compromiso de root o del kernel.** Si el atacante ya es root, CaudalGhost no es una defensa; asumimos un
  kernel y un instalador confiables.
- **Acceso físico** a la máquina.
- **No es un firewall ni un IDS/IPS:** solo observa; no bloquea, corta ni modela tráfico.
- **Precisión perfecta de atribución:** es *best-effort* (ver ventana de reutilización de PID en `SECURITY.md`).

## Supuestos

- La instalación se hace una vez con privilegios (sudo) de forma confiable.
- El equipo es de un solo host; no hay recolección distribuida ni servidor central.
- El kernel soporta eBPF y no está comprometido.
