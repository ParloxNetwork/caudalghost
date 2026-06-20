# CaudalGhost — Kit de marca

Assets reutilizables de la identidad de CaudalGhost: para la app, el sitio web y materiales promocionales.

> **CaudalGhost** es un producto con identidad propia. **Parlox Network** es la marca paraguas que solo *firma* (modelo Microsoft ↔ Microsoft 365). El rojo Parlox `#E3262E` se reserva **exclusivamente** a la firma "Hecho con ❤ por Parlox Network"; nunca como color de producto.

## Qué hay aquí

| Archivo | Para qué |
|---|---|
| [`BRAND.md`](BRAND.md) | Guía de marca: paleta, tipografía, voz, uso del logo, do/don't |
| [`tokens.css`](tokens.css) | Design tokens como CSS custom properties (`--cg-*`) |
| [`tokens.json`](tokens.json) | Los mismos tokens en JSON (para herramientas/build) |
| [`logo/caudalghost-mark.svg`](logo/caudalghost-mark.svg) | La gota-centinela a color (sobre oscuro) |
| [`logo/caudalghost-mark-mono.svg`](logo/caudalghost-mark-mono.svg) | Monocromo (`currentColor`) |
| [`logo/caudalghost-lockup.svg`](logo/caudalghost-lockup.svg) | Marca + wordmark "CaudalGhost" |
| [`logo/favicon.svg`](logo/favicon.svg) | Favicon (marca simplificada, legible a 16 px) |
| [`logo/app-icon.svg`](logo/app-icon.svg) | Ícono de app cuadrado 512px (marca sobre fondo agua-profunda redondeado) — fuente para `cargo tauri icon` |

### Íconos de app (generados)

El set multi-tamaño se genera desde `app-icon.svg`:
```bash
cd crates/caudalghost-gui && cargo tauri icon ../../brand/logo/app-icon.svg
```
Produce `src-tauri/icons/` (32/64/128/128@2x PNG, `icon.icns` macOS, `icon.ico` Windows, logos de Store, mipmaps Android). Re-ejecutar tras cualquier cambio del logo.

## Fuentes (auto-alojadas, OFL)

- **Space Grotesk** — wordmark y títulos
- **Inter** — texto de UI
- **JetBrains Mono** — números (tabulares)

Todas con licencia OFL. En la GUI se vendorizan vía los paquetes npm `@fontsource/*` (auto-alojadas, nada llama a casa). Para web/promos, descárgalas de Google Fonts o usa `@fontsource`.

## Fuente de verdad del diseño

El sistema completo (decisiones, razones, listón premium del logo) vive en el spec:
`_local/specs/2026-06-12-caudalghost-marca-identidad.md`

Los bocetos exploratorios del brainstorming están en
`_local/.superpowers/brainstorm/890183-1781297966/content/` (HTML con SVG embebidos).

## Nota sobre el logo

Dirección *gota-centinela* ("la gota que observa": el brillo especular = el ojo). Los SVG llevan una **pasada de pulido premium**: gradiente de agua radial con profundidad, luz de borde (rim-light), ojo con bloom + núcleo + catchlight, y marcas de aforo. Set de íconos multi-tamaño generado (`.icns`/`.ico`/PNG). Un diseñador profesional podría afinar aún más la geometría, pero esta es una v1 sólida y ownable, lista para producto, web y promos.

## Licencia

Parte del proyecto CaudalGhost (AGPL-3.0-only). El rojo Parlox y el nombre "Parlox Network" son de la marca Parlox.
