# CaudalGhost — Guía de marca

## Esencia

*El instrumento sereno que mide el caudal de tu red y lo archiva por décadas — observa, jamás interfiere.*

El nombre funde **caudal** (el flujo medido de un río; la estación de aforo) con **ghost** (la presencia que lo ve todo y nunca toca el agua).

**Personalidad (jerarquizada):**
1. **La estación de aforo** *(lidera)* — sereno, preciso, fiable, medido, de fiar.
2. **El espíritu del río** *(acento)* — etéreo, fluido, luminoso; sobre todo en la vista en vivo.
3. **El vigía invisible** *(valores)* — discreto, honesto, protector. No toca, no llama a casa.

## Relación con Parlox Network

**CaudalGhost** = producto con identidad propia. **Parlox Network** = marca paraguas que solo *firma* (modelo Microsoft ↔ Microsoft 365).
- Firma: **"Hecho con ❤ por Parlox Network"**.
- El rojo Parlox `#E3262E` se usa **solo** en ese corazón. Nunca como acento, dato ni estado del producto.

## Logo — la gota-centinela

Idea ownable: *el brillo especular que tiene toda gota de agua es el ojo que observa.* Una gota (agua) con una voluta fantasmal sutil (ghost), marcas de aforo discretas (medición) y un punto luminoso (el ojo del vigía).

**Variantes:** [`logo/caudalghost-mark.svg`](logo/caudalghost-mark.svg) (color), [`-mono.svg`](logo/caudalghost-mark-mono.svg) (monocromo, `currentColor`), [`caudalghost-lockup.svg`](logo/caudalghost-lockup.svg) (marca + wordmark), [`favicon.svg`](logo/favicon.svg).

**Uso:**
- Espacio de respiro mínimo alrededor = la altura del "ojo".
- No deformar, no rotar, no recolorear fuera de la paleta agua.
- Sobre oscuro por defecto; sobre claro usar el mono o invertir el texto del wordmark.

## Color — "Agua profunda"

Valores completos en [`tokens.css`](tokens.css) / [`tokens.json`](tokens.json).

| Rol | Hex |
|---|---|
| Fondo base | `#0A1014` |
| Superficie | `#111C22` |
| Borde | `#1D2D34` |
| Texto / secundario / terciario | `#E6EDF3` / `#7C93A0` / `#5A7079` |
| Acento agua (cyan) | `#22D3EE` (bright `#67E8F9`, deep `#0E7490`) |
| ↓ rx (descarga) | `#22D3EE` |
| ↑ tx (subida) | `#F59E0B` |
| rx en vivo (brillo subido) | `#2DD4BF` |
| Categórico | `#818CF8` `#A78BFA` `#E879F9` `#FB7185` `#A3E635` · otros `#64748B` |
| ok / warn / error | `#34D399` / `#FACC15` / `#F87171` |
| Firma Parlox (solo el ❤) | `#E3262E` |

El par **rx cyan / tx ámbar** se lee al instante como bajar/subir sin caer en verde/rojo. La **vista en vivo sube el brillo** (bioluminiscente); el resto de la app es sereno.

## Tipografía

| Rol | Fuente | Notas |
|---|---|---|
| Wordmark / títulos | **Space Grotesk** | carácter técnico-cálido, distintivo |
| UI / cuerpo | **Inter** | legibilidad de trabajo |
| Números | **JetBrains Mono** | **tabular** obligatorio (las cifras cuadran) |

Todas OFL, auto-alojadas (nada llama a casa). El wordmark: `Caudal` en texto primario + `Ghost` en acento agua.

## Voz editorial — "sereno y claro"

Honesto y preciso como el instrumento; cálido sin azúcar. La metáfora del río es **sal, no plato principal**. Nunca alarmista: explica **qué pasa** y **qué va a pasar**. Español, **tú** informal. (Versión ES/EN: futura.)

**Reglas de terminología (siempre):**
- Recolector → **`daemon`** (o `caudalghostd`). **Nunca "demonio".**
- Pedir elevación → **"con privilegios (sudo)"** (no "con privilegios" ni "con sudo" a secas).

**Microcopy de referencia:**
- Sin conexión: *"Sin contacto con el daemon. Sigo reintentando — en cuanto vuelva, el caudal aparece solo."*
- Sin eBPF: *"Sin eBPF no puedo ver qué app consume. Arranca el daemon con privilegios (sudo) y volverán a aparecer."*
- Histórico vacío: *"Aún no hay caudal registrado en este rango. En cuanto fluya algo, quedará aquí para siempre."*
- Firma: *"Hecho con ❤ por Parlox Network"*

## Do / Don't

✅ Acento agua sobre oscuro · números tabulares · glow con moderación (sobre todo en el live) · la firma Parlox discreta.
🚫 El rojo Parlox como color de producto · "demonio" · interpolar huecos de datos · alarmismo · deformar el logo.
