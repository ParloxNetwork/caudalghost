# Changelog — CaudalGhost

Formato basado en [Keep a Changelog](https://keepachangelog.com/es/). Versionado semántico.

## [0.3.0] — sin publicar (rama `security/0.3.0-hardening`)

### Seguridad / Hardening

Auditoría de ciberseguridad de extremo a extremo (2026-06-20) seguida de un hardening completo con **TDD**
(test rojo → verde por cada arreglo) y un **segundo barrido adversarial** que revisó el propio diff buscando
regresiones y vulnerabilidades nuevas. Modelo de amenazas: **local** (el daemon no expone red; la superficie es
el socket Unix, los datos no confiables que llegan al daemon —DNS, `/proc`—, el webview y la instalación).
Nada remotamente explotable. Detalle completo en `_local/plans/2026-06-20-security-audit-0.3.0.md`.

> La app ya partía de una postura sólida (usuario de sistema `nologin`, solo `CAP_BPF`/`CAP_PERFMON`,
> `systemd` muy endurecido, sin red, sin ejecución de subprocesos, SQL parametrizado). Estos cambios cierran
> robustez/DoS local, defensa en profundidad y endurecimiento de instalación.

#### Alta

- **H-1 · Exposición de tráfico entre usuarios en servidor multiusuario.**
  *Identificado:* la concesión automática de ACL del socket por UID (feature D11) daba acceso a **todo** usuario
  con sesión logind (UID≥1000). En un servidor, cada login SSH crea sesión → cualquier usuario podía leer los
  flujos de red de **toda la máquina** (`access.rs`, CWE-284/200).
  *Arreglado:* palanca de política `grant_active_users` (flag `--no-acl-active-users` o
  `CAUDALGHOST_ACL_ACTIVE_USERS=0`); **el default preserva el comportamiento de escritorio** («instalar→abrir→
  funciona»). Desactivada, el socket queda solo bajo el grupo `caudalghost`. (commits `a48d8a3`, `e58be07`)

#### Media

- **H-2 · DoS local del socket IPC.**
  *Identificado:* un hilo por conexión sin límite, sin timeout de lectura y sin tope de longitud de línea →
  agotamiento de hilos/heap por un cliente local con acceso al socket (`api.rs`, CWE-400/770).
  *Arreglado:* timeout de E/S (30 s), lectura de línea acotada a 64 KiB (línea enorme → `Error`, no OOM) y
  `ConnLimiter` (máx. 128 conexiones concurrentes). Mitigación complementaria en systemd (H-9). (commit `8323958`)

#### Baja

- **H-3/H-4 · Webview sin CSP + `innerHTML`.**
  *Identificado:* `csp: null` en Tauri (sin defensa en profundidad) y un `innerHTML` latente en el tooltip del
  chart (`tauri.conf.json`, `Chart.svelte`, CWE-79).
  *Arreglado:* CSP estricta (`default-src 'self'`, `object-src 'none'`, `frame-ancestors 'none'`, `connect-src`
  solo IPC) y tooltip con DOM seguro (`textContent`). (commit `cdd6ea0`) — *ver «Pendiente» (smoke-test).*
- **H-5 · `$SUDO_USER` sin sanear en `postinst` (root).**
  *Identificado:* `usermod -aG caudalghost $SUDO_USER` con valor influenciable → inyección de opciones (CWE-88).
  *Arreglado:* `usermod ... -- "$user"` + guard de charset (permite usuarios de empresa Kerberos/Samba, rechaza
  metacaracteres y pinta de opción). Test: `scripts/test-postinst.sh`. (commits `31c3251`, `4dade72`)
- **H-6 · Plugin `opener` sin uso.**
  *Identificado:* `tauri-plugin-opener` exponía `open_url`/`open_path` al webview sin que el frontend lo use
  (CWE-272). *Arreglado:* eliminado de Rust, capabilities y npm (`package.json`); de paso podó ~85 deps
  transitivas del lock. (commits `35b483e`, `ad29b13`)
- **H-7 · Strings no confiables sin acotar.**
  *Identificado:* etiquetas de app (`/proc/{cgroup,comm}`, fijables por un proceso) y hostnames DNS se
  almacenaban sin tope de longitud ni saneo de control chars (`appmap.rs`, `resolve.rs`, CWE-400).
  *Arreglado:* `sanitize_label` (cap 128 + sin control chars), `is_acceptable_hostname` (drop vacío/>253/control),
  tope defensivo de 1 MiB en el buffer del monitor. (commit `e7f293c`)

#### Info

- **H-8 · `query_top_remotes` sin `LIMIT`.** Top de remotos acotado a 500 filas (top-N legítimo que acota la
  respuesta IPC; cubre un escritorio realista sin truncar). (commits `5afdee4`, `e58be07`)
- **H-9 · `systemd` sin techo de recursos.** `TasksMax=256` + `MemoryMax=256M` como control compensatorio del DoS
  (H-2), sin afectar a eBPF. (commit `c33b4c6`)
- **H-9b · Símbolos en release.** `[profile.release] strip=true` (binarios más pequeños, sin tabla de símbolos).
  (commit `65b448a`)

#### Correcciones del review adversarial del propio diff

10 hallazgos (todos Low/Info): 8 corregidos en código (regresión del `LIMIT` global con muchas apps; parsing
case-insensitive del env; `postinst` que rechazaba usuarios de empresa; rustdoc desplazado; visibilidad y
ordenación atómica de `ConnLimiter`; remanente npm del opener) y 2 documentados (el `'unsafe-inline'` de la CSP es
necesario por la hidratación inline de SvelteKit). (commits `e58be07`, `4dade72`, `ad29b13`)

### Verificación

178 resultados de test verdes · `cargo clippy --all-targets` limpio · `cargo fmt --check` limpio ·
`cargo build --release` OK · `scripts/test-postinst.sh` 5/5 · `svelte-check` 0 errores + `vite build` OK.

### Pendiente antes de publicar 0.3.0

- **Smoke-test visual de la GUI**: confirmar que la CSP nueva no rompe el render (no verificable headless).
- **Política del `.deb` servidor**: decidir si la edición server arranca con el gate de acceso OFF (H-1).

### Rollback

`git checkout main` (o `git reset --hard v0.2.0-baseline`) devuelve el árbol 0.2.0 intacto.
