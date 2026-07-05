<div align="center">

<picture>
  <source media="(prefers-color-scheme: dark)" srcset="brand/logo/caudalghost-lockup.svg">
  <img alt="CaudalGhost" src="brand/logo/caudalghost-lockup-light.svg" width="380">
</picture>

### El instrumento sereno que mide el caudal de tu red — observa, jamás interfiere.

[![Licencia](https://img.shields.io/badge/licencia-AGPL--3.0-22D3EE)](LICENSE)
[![Versión](https://img.shields.io/badge/versión-0.8.0-0E7490)](CHANGELOG.md)
[![Plataforma](https://img.shields.io/badge/Linux-Debian_·_Ubuntu-5A7079)](#-instalación)
[![Hecho por Parlox Network](https://img.shields.io/badge/hecho_por-Parlox_Network-E3262E)](https://caudalghost.parlox.net)

**Español** · [English](#-english)

</div>

---

**CaudalGhost** te muestra **qué aplicaciones y a qué servidores** consume tu conexión —
medido por interfaz y por aplicación con **eBPF**, con histórico escalonado en **SQLite** y un
panel **en vivo**. Es **telemetría pura**: solo observa, no es un firewall y **nada llama a casa**.

### Tres caras

Un mismo `daemon`, tres formas de mirarlo:

| | | |
|---|---|---|
| **CLI** — `caudalghost` | consultas de un disparo, **scriptables** (`table`/`json`/`csv`/`ndjson`) | para cron, systemd y pipelines con `jq` |
| **TUI** — `caudalghost live` | dashboard interactivo a **pantalla completa** en la terminal | apps · interfaces · servidores, en vivo o histórico |
| **GUI** — `caudalghost-gui` | app de **escritorio** con gráficos fluidos | para el día a día |

```console
$ caudalghost serie hoy
📈 serie · rango: hoy · cubo: hora
▃▁▁▂▅▇█▆▄▃▂▁▂▃▅▃▁▁
┌─────────────┬───────────┬───────────┬───────────┐
│ tiempo      ┆        rx ┆        tx ┆     total │
╞═════════════╪═══════════╪═══════════╪═══════════╡
│ 09:00       ┆  40.9 MiB ┆ 144.3 MiB ┆ 185.1 MiB │
│ 10:00       ┆  12.4 MiB ┆   3.1 MiB ┆  15.5 MiB │
│ 11:00       ┆   4.6 MiB ┆   2.7 MiB ┆   7.3 MiB │
│ Σ total     ┆   1.1 GiB ┆ 402.7 MiB ┆   1.5 GiB │
└─────────────┴───────────┴───────────┴───────────┘

$ caudalghost top hoy -n 3 -o csv --fields app,total
app,total
firefox,820.4 MiB
thunderbird,73.2 MiB
syncthing,41.9 MiB
```

<sub>*(datos de ejemplo)*</sub>

### 📦 Instalación

Dos ediciones `.deb` (Debian/Ubuntu y derivadas):

```bash
# Escritorio — daemon + CLI + GUI
sudo apt install ./caudalghost_0.8.0-1_amd64.deb

# Servidor headless — daemon + CLI (sin GUI)
sudo apt install ./caudalghost-server_0.8.0-1_amd64.deb
```

Descarga la última versión en [**Releases**](../../releases) o en [caudalghost.parlox.net](https://caudalghost.parlox.net).

### 🛡️ Privacidad y seguridad

- **Solo observa** — mide bytes, no inspecciona contenido ni bloquea nada (no es un firewall).
- **Nada llama a casa** — sin telemetría externa; tus datos viven en tu máquina (SQLite local).
- **Sin `sudo` para el servicio** — el `daemon` corre con capacidades acotadas (`CAP_BPF`/`CAP_PERFMON`), no como root.
- **Software libre** — [AGPL-3.0-only](LICENSE).

### 📖 Más

Documentación, capturas de la GUI y el porqué del diseño en **[caudalghost.parlox.net](https://caudalghost.parlox.net)**.

---

<a id="-english"></a>

## 🇬🇧 English

**CaudalGhost** shows you **which apps and which servers** your connection is talking to —
measured per interface and per application with **eBPF**, with tiered history in **SQLite** and a
**live** dashboard. It's **pure telemetry**: it only watches — it is not a firewall and **nothing
phones home**.

### Three faces

One `daemon`, three ways to look at it:

| | | |
|---|---|---|
| **CLI** — `caudalghost` | one-shot, **scriptable** queries (`table`/`json`/`csv`/`ndjson`) | for cron, systemd and `jq` pipelines |
| **TUI** — `caudalghost live` | interactive **full-screen** terminal dashboard | apps · interfaces · servers, live or historical |
| **GUI** — `caudalghost-gui` | **desktop** app with fluid charts | for everyday use |

```console
$ caudalghost serie hoy      # "hoy" = today
📈 serie · rango: hoy · cubo: hora
▃▁▁▂▅▇█▆▄▃▂▁▂▃▅▃▁▁
┌─────────────┬───────────┬───────────┬───────────┐
│ tiempo      ┆        rx ┆        tx ┆     total │
│ 09:00       ┆  40.9 MiB ┆ 144.3 MiB ┆ 185.1 MiB │
│ Σ total     ┆   1.1 GiB ┆ 402.7 MiB ┆   1.5 GiB │
└─────────────┴───────────┴───────────┴───────────┘
```

<sub>*(sample data — the interface is currently Spanish; an English UI is planned)*</sub>

### 📦 Install

Two `.deb` editions (Debian/Ubuntu and derivatives):

```bash
# Desktop — daemon + CLI + GUI
sudo apt install ./caudalghost_0.8.0-1_amd64.deb

# Headless server — daemon + CLI (no GUI)
sudo apt install ./caudalghost-server_0.8.0-1_amd64.deb
```

Grab the latest build from [**Releases**](../../releases) or [caudalghost.parlox.net](https://caudalghost.parlox.net).

### 🛡️ Privacy & security

- **Watches only** — counts bytes, never inspects content or blocks anything (not a firewall).
- **Nothing phones home** — no external telemetry; your data stays on your machine (local SQLite).
- **No `sudo` for the service** — the `daemon` runs with scoped capabilities (`CAP_BPF`/`CAP_PERFMON`), not as root.
- **Free software** — [AGPL-3.0-only](LICENSE).

### 📖 More

Docs, GUI screenshots and the design rationale live at **[caudalghost.parlox.net](https://caudalghost.parlox.net)**.

---

<div align="center">
<sub>Hecho con ❤ por <a href="https://caudalghost.parlox.net"><b>Parlox Network</b></a> · AGPL-3.0-only</sub>
</div>
