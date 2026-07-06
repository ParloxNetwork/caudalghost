//! Render puro del dashboard sobre un Frame de ratatui (testeable con TestBackend).

use crate::live::state::{frescura, Focus, LiveModel, Mode, Nivel, Vista};
use crate::render::format_units;
use chrono::Datelike;
use ratatui::layout::{Constraint, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph, Row, Sparkline, Table, Wrap};
use ratatui::Frame;

/// Dibuja el panel de apps según la vista activa del modelo (`vista_apps`): el propio
/// `draw_apps`/`draw_hist_apps` maneja su sub-vista de drill (guard por `drill.vista`).
fn draw_apps_segun_vista(frame: &mut Frame, model: &LiveModel, area: Rect) {
    match model.vista_apps {
        Vista::Ahora => draw_apps(frame, model, area),
        Vista::Historico => draw_hist_apps(frame, model, area),
    }
}

/// Dibuja el panel de interfaces según la vista activa del modelo (`vista_ifaces`).
fn draw_ifaces_segun_vista(frame: &mut Frame, model: &LiveModel, area: Rect) {
    match model.vista_ifaces {
        Vista::Ahora => draw_ifaces(frame, model, area),
        Vista::Historico => draw_hist_ifaces(frame, model, area),
    }
}

/// Dibuja el dashboard completo en 3 bandas:
/// ① gráficos ↓/↑ lado a lado · ② `apps | interfaces` (cada panel según su vista,
/// ahora/histórico, alternable con `v`) · ③ servidores (top de todas las apps, ancho
/// completo, también con su propia vista). En terminales bajas se omite primero la
/// banda de servidores, y si sigue sin caber, se compacta a solo gráficos + ②.
pub fn draw(frame: &mut Frame, model: &LiveModel) {
    use ratatui::layout::{Constraint, Direction, Layout};
    let area = frame.area();

    if model.mode == Mode::Zoom {
        let bandas = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(1), // cabecera
                Constraint::Min(3),    // panel enfocado a pantalla completa
                Constraint::Length(1), // pie
            ])
            .split(area);
        draw_header(frame, model, bandas[0]);
        match model.focus {
            Focus::Grafico => draw_charts(frame, model, bandas[1]),
            Focus::Apps => draw_apps_segun_vista(frame, model, bandas[1]),
            Focus::Interfaces => draw_ifaces_segun_vista(frame, model, bandas[1]),
            Focus::Servidores => draw_servidores(frame, model, bandas[1]),
        }
        draw_footer(frame, model, bandas[2]);
        return;
    }

    let con_servidores = area.height >= 30;

    let constraints = if con_servidores {
        vec![
            Constraint::Length(1),  // cabecera
            Constraint::Min(8),     // ① gráficos
            Constraint::Length(11), // ② apps | interfaces
            Constraint::Length(11), // ③ servidores
            Constraint::Length(1),  // pie
        ]
    } else {
        vec![
            Constraint::Length(1),  // cabecera
            Constraint::Min(6),     // ① gráficos
            Constraint::Length(11), // ② apps | interfaces
            Constraint::Length(1),  // pie
        ]
    };
    let bandas = Layout::default()
        .direction(Direction::Vertical)
        .constraints(constraints)
        .split(area);

    draw_header(frame, model, bandas[0]);
    draw_charts(frame, model, bandas[1]);

    // ② Banda apps (izq 55%) | interfaces (der 45%), cada uno según su vista.
    let media = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(55), Constraint::Percentage(45)])
        .split(bandas[2]);
    draw_apps_segun_vista(frame, model, media[0]);
    draw_ifaces_segun_vista(frame, model, media[1]);

    if con_servidores {
        // ③ Banda servidores: ancho completo.
        draw_servidores(frame, model, bandas[3]);
        draw_footer(frame, model, bandas[4]);
    } else {
        draw_footer(frame, model, bandas[3]);
    }

    if model.mode == Mode::Ayuda {
        draw_help_overlay(frame, area);
    }
}

/// Etiqueta del modo activo, primer segmento del pie (§6.1). Para `Zoom` incluye
/// el panel enfocado en minúsculas ("apps", "ifaces", "grafico").
fn etiqueta_modo(mode: Mode, focus: Focus) -> String {
    match mode {
        Mode::Normal => "[NORMAL]".to_string(),
        Mode::Zoom => {
            let panel = match focus {
                Focus::Grafico => "grafico",
                Focus::Apps => "apps",
                Focus::Interfaces => "ifaces",
                Focus::Servidores => "hosts",
            };
            format!("[ZOOM {panel}]")
        }
        Mode::Filtro => "[FILTRO]".to_string(),
        Mode::PausaScrub => "[PAUSA+SCRUB]".to_string(),
        Mode::Ayuda => "[AYUDA]".to_string(),
        Mode::Drilldown => "[HOSTS]".to_string(),
    }
}

/// Segmentos del pie contextual: primero el indicador de modo (`etiqueta_modo`),
/// luego las teclas relevantes al modo/foco actual, y siempre termina en "? ayuda"
/// (Task 11). El texto exacto de las teclas globales (pausa/incluir lo/unidades) lo
/// construye `draw_footer`, que sustituye el segmento correspondiente por el label
/// dinámico de `model.units`; aquí se deja el marcador base para los tests puros.
fn segmentos_pie(mode: Mode, focus: Focus) -> Vec<String> {
    let mut segs = vec![etiqueta_modo(mode, focus)];
    match mode {
        Mode::Filtro => {
            segs.push("Esc cancelar".to_string());
            segs.push("Enter confirmar".to_string());
        }
        Mode::Zoom => {
            segs.push("Esc volver".to_string());
        }
        Mode::PausaScrub => {
            segs.push("←/→ mover cursor".to_string());
            segs.push("Esc reanudar".to_string());
        }
        Mode::Ayuda => {
            segs.push("Esc/? cerrar".to_string());
        }
        Mode::Normal => {
            segs.push("Tab foco".to_string());
            segs.push("Enter zoom".to_string());
            segs.push("v vista".to_string());
        }
        Mode::Drilldown => {
            segs.push("Esc/← volver".to_string());
        }
    }
    segs.push("p pausa · a incluir lo · u unidades".to_string());
    segs.push("? ayuda".to_string());
    segs
}

/// Mapea el `Nivel` lógico (frescura del dato, resaltado por umbral) al color de
/// terminal que lo representa. Verde=Green, Ambar=Yellow. El nivel "alto" NO usa rojo
/// (poco legible como texto y demasiado alarmista): un naranja cálido truecolor que
/// destaca sin gritar. El estado de error real (desconectado) sí sigue en rojo aparte.
fn color_nivel(n: Nivel) -> Color {
    match n {
        Nivel::Verde => Color::Green,
        Nivel::Ambar => Color::Yellow,
        Nivel::Rojo => Color::Rgb(0xF2, 0x8C, 0x28),
    }
}

/// Popup modal centrado con todas las teclas de `keymap_activo()` (fuente única,
/// state.rs). `Clear` limpia el área bajo el overlay para que no se vea el
/// contenido de fondo a través de él.
fn draw_help_overlay(frame: &mut Frame, area: Rect) {
    let ancho = area.width.saturating_sub(10).clamp(20, 60);
    let mapa = crate::live::state::keymap_activo();
    let alto = (mapa.len() as u16 + 2).min(area.height.saturating_sub(4));
    let popup = Rect {
        x: area.x + (area.width.saturating_sub(ancho)) / 2,
        y: area.y + (area.height.saturating_sub(alto)) / 2,
        width: ancho,
        height: alto,
    };
    frame.render_widget(Clear, popup);
    let lineas: Vec<Line> = mapa
        .iter()
        .map(|(tecla, accion)| {
            Line::from(vec![
                Span::styled(
                    format!("{tecla:<16}"),
                    Style::default()
                        .fg(Color::Cyan)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::raw(*accion),
            ])
        })
        .collect();
    let block = Block::default()
        .borders(Borders::ALL)
        .title(" ayuda · teclas ");
    frame.render_widget(Paragraph::new(lineas).block(block), popup);
}

/// Fecha+hora localizada corta con año ("mié 02 jul 2026 · 16:38:57"); si `ancho`
/// no da para la fecha, se acorta solo a la hora ("16:38:57"). `ahora` inyectado
/// para determinismo en tests.
const ANCHO_MIN_FECHA: u16 = 60;

fn formatear_fecha_hora(ahora: chrono::DateTime<chrono::Local>, ancho: u16) -> String {
    let hora = ahora.format("%H:%M:%S").to_string();
    if ancho < ANCHO_MIN_FECHA {
        return hora;
    }
    const DIAS: [&str; 7] = ["lun", "mar", "mié", "jue", "vie", "sáb", "dom"];
    const MESES: [&str; 12] = [
        "ene", "feb", "mar", "abr", "may", "jun", "jul", "ago", "sep", "oct", "nov", "dic",
    ];
    let dia = DIAS[ahora.weekday().num_days_from_monday() as usize];
    let mes = MESES[ahora.month0() as usize];
    format!("{dia} {:02} {mes} {} · {hora}", ahora.day(), ahora.year())
}

/// Cabecera de una línea: título, versión, estado eBPF, hora, conexión.
pub fn draw_header(frame: &mut Frame, model: &LiveModel, area: Rect) {
    let ebpf = if model.ebpf {
        Span::styled("eBPF: activo", Style::default().fg(Color::Green))
    } else {
        Span::styled("eBPF: inactivo", Style::default().fg(Color::Yellow))
    };
    // ÍTEM 1 — Spinner braille animado cuando desconectado (Emil: el único sitio donde
    // animar un TUI ayuda — percepción de velocidad de reconexión).
    const SPINNER: [char; 6] = ['⠋', '⠙', '⠹', '⠸', '⠼', '⠴'];
    let conexion = if model.connected {
        Span::styled("conectado", Style::default().fg(Color::Green))
    } else {
        let sp = SPINNER[(model.frame as usize) % SPINNER.len()];
        Span::styled(
            format!("{sp} desconectado — reintentando"),
            Style::default().fg(Color::Red),
        )
    };
    let ahora = chrono::Local::now();
    let hora = formatear_fecha_hora(ahora, area.width);
    let pausa = if model.paused { "  [PAUSA]" } else { "" };
    let (nivel, texto_frescura) = match model.samples.last() {
        Some(s) => frescura(s.ts, ahora.timestamp()),
        None => (Nivel::Rojo, "sin datos".to_string()),
    };
    let dato = Span::styled(
        format!("  dato: {texto_frescura}"),
        Style::default().fg(color_nivel(nivel)),
    );
    // ÍTEM 4 — versión y hora en DarkGray (secundario) para que eBPF y conexión destaquen.
    let line = Line::from(vec![
        Span::styled(" CaudalGhost ", Style::default().fg(Color::Cyan)),
        Span::styled(
            format!("v{}  ", model.version),
            Style::default().fg(Color::DarkGray),
        ),
        ebpf,
        Span::raw("  "),
        conexion,
        dato,
        Span::styled(
            format!("  {hora}{pausa}"),
            Style::default().fg(Color::DarkGray),
        ),
    ]);
    frame.render_widget(Paragraph::new(line), area);
}

/// Dibuja un sparkline de caudal en `area`: serie + título con tasa actual y pico.
/// Ancla a la derecha (fit_series al ancho interior de SU área).
// Los 8 parámetros son todos distintos (frame, geometría, datos, etiqueta, color, unidades);
// un struct wrapper aquí añadiría peso sin beneficio real en una función privada de render.
#[allow(clippy::too_many_arguments)]
fn draw_chart_one(
    frame: &mut Frame,
    area: Rect,
    serie: &[u64],
    cur: u64,
    peak: u64,
    peak_age_secs: u64,
    flecha: &str,
    color: Color,
    units: crate::render::UnitMode,
    scrub: Option<usize>,
) {
    // Eje X (fila inferior con "hace Nmin"/"ahora"): se reserva 1 fila si hay alto
    // suficiente para no comerse el propio gráfico en terminales muy bajas.
    use ratatui::layout::{Constraint as C, Direction as D, Layout as L};
    let mostrar_x = area.height >= 6;
    let (plot_area, x_area) = if mostrar_x {
        let v = L::default()
            .direction(D::Vertical)
            .constraints([C::Min(3), C::Length(1)])
            .split(area);
        (v[0], Some(v[1]))
    } else {
        (area, None)
    };
    // Eje Y (canalón izquierdo con "máx visible" arriba / "0" abajo): se reserva solo si
    // el plot resultante conserva >=20 cols interiores; si no, el gráfico usa el ancho
    // completo (como en 0.6.0). `gw` (ancho del canalón) depende de `max_visible`, que a su
    // vez depende de la ventana final de `plot` — hay una dependencia circular entre el
    // split y los datos. Se resuelve en dos pasadas:
    //   1) una ESTIMACIÓN de `max_visible` sobre `plot_area.width - 2` (cota superior del
    //      ancho interior real, ya que descontar el canalón solo puede angostarlo más) para
    //      fijar un `gw` provisional y decidir/realizar el split (mostrar_y/plot/inner_w).
    //   2) con `inner_w` YA DEFINITIVO, se recalcula `max_visible` sobre esa misma ventana
    //      (la que `fit_series` usará después para pintar el Sparkline), así la etiqueta de
    //      arriba del canalón siempre corresponde al máximo de las barras realmente
    //      dibujadas (antes: se reutilizaba la estimación del paso 1, calculada sobre una
    //      ventana más ancha que la finalmente dibujada — bug crítico 0.6.1).
    let estimacion = fit_series(serie, plot_area.width.saturating_sub(2) as usize);
    let gw = ancho_canalon_y(estimacion.iter().copied().max().unwrap_or(0), units);
    let mostrar_y = plot_area.width > gw + 22;
    let (canalon, plot) = if mostrar_y {
        let h = L::default()
            .direction(D::Horizontal)
            .constraints([C::Length(gw), C::Min(20)])
            .split(plot_area);
        (Some(h[0]), h[1])
    } else {
        (None, plot_area)
    };
    // La fila del eje X debe alinear sus columnas con `plot` (no con `plot_area`), así
    // que si hay canalón Y le aplicamos el mismo recorte horizontal a `x_area`.
    let x_area = x_area.map(|xa| {
        if mostrar_y {
            let h = L::default()
                .direction(D::Horizontal)
                .constraints([C::Length(gw), C::Min(20)])
                .split(xa);
            h[1]
        } else {
            xa
        }
    });
    let inner_w = plot.width.saturating_sub(2) as usize;
    let max_visible = fit_series(serie, inner_w)
        .iter()
        .copied()
        .max()
        .unwrap_or(0);
    // ÍTEM 2 — Hint "acumulando…" cuando el gráfico aún no se ha llenado.
    // Usamos Line::from(vec![...]) para componer el título con estilos distintos:
    // tasa actual en bold (dato clave del gráfico) y el hint en DarkGray (secundario).
    let pico_txt = if peak_age_secs == 0 {
        format!("(pico {}/s)", format_units(peak, units))
    } else {
        format!(
            "(pico {}/s · hace {})",
            format_units(peak, units),
            edad_relativa(peak_age_secs)
        )
    };
    let base_title = format!(" {flecha} {}/s  {} ", format_units(cur, units), pico_txt);
    let title_line: Line = if let Some(idx) = scrub {
        // Valor exacto de la muestra bajo el cursor + marca relativa, sobre la
        // rebanada visible (los últimos inner_w puntos, igual que fit_series).
        let start = serie.len().saturating_sub(inner_w);
        let visible = &serie[start..];
        let valor = visible.get(idx).copied().unwrap_or(0);
        let n_atras = visible.len().saturating_sub(1).saturating_sub(idx);
        Line::from(vec![
            Span::styled(base_title, Style::default().add_modifier(Modifier::BOLD)),
            Span::styled(
                format!(
                    "· cursor {}/s (hace {n_atras} s) ",
                    format_units(valor, units)
                ),
                Style::default()
                    .fg(Color::White)
                    .add_modifier(Modifier::BOLD),
            ),
        ])
    } else if serie.len() < inner_w {
        // El gráfico aún se está llenando (fit_series rellenó ceros a la izquierda).
        Line::from(vec![
            Span::styled(base_title, Style::default().add_modifier(Modifier::BOLD)),
            Span::styled("· acumulando… ", Style::default().fg(Color::DarkGray)),
        ])
    } else {
        Line::from(Span::styled(
            base_title,
            Style::default().add_modifier(Modifier::BOLD),
        ))
    };
    let fit = fit_series(serie, inner_w);
    // Restraint (Emil): el dato vivo (1 Hz) se mantiene crisp — sin animación;
    // solo el spinner de reconexión y el flash de teclas animan, por ser estados ocasionales.
    let spark = Sparkline::default()
        .block(Block::default().borders(Borders::ALL).title(title_line))
        .data(&fit)
        .style(Style::default().fg(color));
    frame.render_widget(spark, plot);

    // Canalón Y: máximo visible (arriba) y "0" (abajo), alineados a la derecha, en las
    // filas primera/última del bloque del Sparkline (donde caen el borde superior e
    // inferior; con altura >=3 hay al menos una fila de barras entre medio).
    if let Some(cy) = canalon {
        if cy.width > 0 && cy.height > 0 {
            let arriba = format_units(max_visible, units);
            let abajo = format_units(0, units);
            let buf = frame.buffer_mut();
            let escribe_derecha = |buf: &mut ratatui::buffer::Buffer, y: u16, texto: &str| {
                let len = texto.chars().count() as u16;
                let x0 = (cy.x + cy.width).saturating_sub(len);
                for (i, ch) in texto.chars().enumerate() {
                    let x = x0 + i as u16;
                    if x < cy.x {
                        continue;
                    }
                    if let Some(cell) = buf.cell_mut((x, y)) {
                        cell.set_symbol(&ch.to_string());
                        cell.set_fg(Color::DarkGray);
                    }
                }
            };
            escribe_derecha(buf, cy.y, &arriba);
            if cy.height > 1 {
                escribe_derecha(buf, cy.y + cy.height - 1, &abajo);
            }
        }
    }

    // Marcador de scrub: 1 columna resaltada, superpuesta al Sparkline ya dibujado.
    // scrub_idx indexa la rebanada VISIBLE (misma ventana que fit_series calcula).
    if let (Some(idx), true) = (scrub, plot.width > 2 && plot.height > 2) {
        let start = serie.len().saturating_sub(inner_w);
        let visible_len = serie.len() - start;
        // La columna del cursor dentro de `fit` (que ancla a la derecha, con ceros
        // de relleno a la izquierda si la serie visible es más corta que inner_w).
        let pad = inner_w.saturating_sub(visible_len);
        let col = pad + idx.min(visible_len.saturating_sub(1));
        if col < inner_w {
            let x = plot.x + 1 + col as u16;
            let y = plot.y + plot.height.saturating_sub(2); // fila de barras, dentro del borde
            let buf = frame.buffer_mut();
            if let Some(cell) = buf.cell_mut((x, y)) {
                cell.set_fg(Color::White);
                cell.modifier.insert(Modifier::REVERSED);
            }
        }
    }

    // Eje X: marcador ▲ del pico (si su columna cae dentro de la franja visible) y,
    // encima, las marcas de tiempo alineadas a las columnas de datos. Las etiquetas se
    // pintan DESPUÉS del marcador para que "ahora" nunca quede tapada cuando el pico
    // coincide con el borde derecho (peak_age_secs == 0 → misma columna que "ahora").
    if let Some(xa) = x_area {
        if xa.width > 2 {
            let buf = frame.buffer_mut();
            if let Some(pc) = pico_col_visible(peak_age_secs, inner_w) {
                let x = xa.x + 1 + pc as u16;
                if x < xa.x + xa.width {
                    if let Some(cell) = buf.cell_mut((x, xa.y)) {
                        cell.set_symbol("▲");
                        cell.set_fg(color);
                    }
                }
            }
            let right_edge = xa.x + xa.width; // primera columna fuera de la franja
            for (c, etiqueta) in ticks_eje_x(inner_w) {
                let len = etiqueta.chars().count() as u16;
                // Ancla normal: la etiqueta empieza en su columna de dato. Si así se saldría
                // por el borde derecho (típico de "ahora", pegada al último dato), se
                // retrocede lo justo para que quede completa dentro de la franja.
                let deseado = xa.x + 1 + c as u16;
                let x0 = if deseado + len > right_edge {
                    right_edge.saturating_sub(len)
                } else {
                    deseado
                };
                for (i, ch) in etiqueta.chars().enumerate() {
                    let x = x0 + i as u16;
                    if x < xa.x + 1 || x >= right_edge {
                        continue;
                    }
                    if let Some(cell) = buf.cell_mut((x, xa.y)) {
                        cell.set_symbol(&ch.to_string());
                        cell.set_fg(Color::DarkGray);
                    }
                }
            }
        }
    }
}

/// Banda de gráficos: ↓ rx (izquierda, cyan) y ↑ tx (derecha, magenta), lado a lado.
pub fn draw_charts(frame: &mut Frame, model: &LiveModel, area: Rect) {
    use ratatui::layout::{Direction, Layout};
    let (rx, tx) = model.throughput_series();
    let (cur_rx, cur_tx) = model.current_rate();
    let peak_rx = rx.iter().copied().max().unwrap_or(0);
    let peak_tx = tx.iter().copied().max().unwrap_or(0);
    // Edad (en muestras, 1 Hz => segundos) del pico: distancia entre el sample
    // que alcanzó el máximo y el sample más reciente de la serie.
    let edad = |serie: &[u64]| -> u64 {
        match serie.iter().enumerate().max_by_key(|(_, v)| **v) {
            Some((idx, _)) => (serie.len().saturating_sub(1) - idx) as u64,
            None => 0,
        }
    };
    let peak_rx_age = edad(&rx);
    let peak_tx_age = edad(&tx);

    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(area);

    let scrub = if model.mode == Mode::PausaScrub {
        model.scrub_idx
    } else {
        None
    };

    draw_chart_one(
        frame,
        cols[0],
        &rx,
        cur_rx,
        peak_rx,
        peak_rx_age,
        "↓",
        Color::Cyan,
        model.units,
        scrub,
    );
    draw_chart_one(
        frame,
        cols[1],
        &tx,
        cur_tx,
        peak_tx,
        peak_tx_age,
        "↑",
        Color::Magenta,
        model.units,
        scrub,
    );
}

/// Pie de una línea: indicador de modo + ayuda de teclas contextual (vía
/// `segmentos_pie`, fuente única de verdad junto con `keymap_activo`) a la
/// izquierda y, si hay ancho, el tagline de marca a la derecha (corazón en el
/// rojo de Parlox Network, #E3262E).
pub fn draw_footer(frame: &mut Frame, model: &LiveModel, area: Rect) {
    use ratatui::layout::{Alignment, Direction, Layout};
    // ÍTEM 3 — Micro-parpadeo: el segmento de la tecla activa va en blanco+BOLD durante
    // flash_frames redibujos. Sutil — el ojo percibe la acción sin decoración permanente.
    let bold_blanco = Style::default()
        .fg(Color::White)
        .add_modifier(Modifier::BOLD);
    let tenue = Style::default().fg(Color::DarkGray);
    // El pie es de una sola línea y de ancho limitado: de `segmentos_pie` solo se
    // usan el indicador de modo (primero) y las teclas universales + "? ayuda"
    // (últimos dos); los hints específicos de modo (p.ej. "Tab foco", "Esc volver")
    // quedan para el overlay de ayuda completo (`keymap_activo`), que tiene espacio
    // de sobra. El segmento de teclas globales lleva el label de unidades dinámico
    // (p.ej. "u unidades: KiB"); el flash de una tecla resalta el segmento cuyo
    // contenido la nombra en vez de un match manual por segmento fijo (Task 11).
    let segs = segmentos_pie(model.mode, model.focus);
    let modo = segs.first().cloned().unwrap_or_default();
    let ayuda = segs.last().cloned().unwrap_or_default();
    let universal = format!(
        "p pausa · a incluir lo · u unidades: {}",
        model.units.label()
    );
    let mut spans = vec![Span::styled(format!(" {modo}"), tenue)];
    for texto in ["q/Esc salir".to_string(), universal, ayuda] {
        let estilo = if model.flash_frames > 0
            && model
                .flash_key
                .is_some_and(|k| texto.contains(&format!("{k} ")) || texto.starts_with(k))
        {
            bold_blanco
        } else {
            tenue
        };
        spans.push(Span::styled(" · ", tenue));
        spans.push(Span::styled(texto, estilo));
    }
    spans.push(Span::styled(" ", tenue));
    let comandos = Paragraph::new(Line::from(spans));

    // El tagline solo se muestra si la terminal es lo bastante ancha para no
    // pisar los comandos (≈57 cols) + el tagline (32) + respiro; en pantallas
    // más estrechas el pie queda solo con teclas.
    if area.width >= 90 {
        let cols = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Min(20), Constraint::Length(32)])
            .split(area);
        frame.render_widget(comandos, cols[0]);
        let parlox_red = Color::Rgb(0xE3, 0x26, 0x2E);
        let tag = Line::from(vec![
            Span::styled("Hecho con ", Style::default().fg(Color::DarkGray)),
            Span::styled("❤", Style::default().fg(parlox_red)),
            Span::styled(" por Parlox Network ", Style::default().fg(Color::DarkGray)),
        ]);
        frame.render_widget(Paragraph::new(tag).alignment(Alignment::Right), cols[1]);
    } else {
        frame.render_widget(comandos, area);
    }
}

/// Título estándar de panel: ` nombre · ámbito ` y, si `extra` trae algo
/// (p.ej. `filtro: xyz`), lo añade como ` · extra ` antes del espacio final.
/// Fuente única de verdad para los títulos de panel (apps/interfaces, vivo/histórico).
fn titulo_panel(nombre: &str, ambito: &str, extra: Option<&str>) -> String {
    match extra {
        Some(e) if !e.is_empty() => format!(" {nombre} · {ambito} · {e} "),
        _ => format!(" {nombre} · {ambito} "),
    }
}

/// Reconstruye la serie reciente (rx+tx por muestra, ventana `PANEL_WINDOW_SECS`) de una
/// app dada, para poder calcular su `nivel_umbral`. `top_apps()` no trae serie por app.
fn serie_app(model: &LiveModel, nombre: &str) -> Vec<u64> {
    let start = model
        .samples
        .len()
        .saturating_sub(crate::live::state::PANEL_WINDOW_SECS);
    model.samples[start..]
        .iter()
        .map(|s| {
            s.apps
                .iter()
                .find(|a| a.app == nombre)
                .map(|a| a.rx + a.tx)
                .unwrap_or(0)
        })
        .collect()
}

/// Panel de top apps con barras inline. Si eBPF inactivo, aviso honesto.
pub fn draw_apps(frame: &mut Frame, model: &LiveModel, area: Rect) {
    let filtro_activo = !model.filtro.is_empty() && model.focus == Focus::Apps;
    let extra = filtro_activo.then(|| format!("filtro: {}", model.filtro));
    let titulo = titulo_panel("top apps", "ahora", extra.as_deref());
    let block = Block::default().borders(Borders::ALL).title(titulo);
    if !model.ebpf {
        let aviso = Paragraph::new(Line::from(Span::styled(
            "eBPF inactivo — corre el daemon con sudo para ver apps",
            Style::default().fg(Color::Yellow),
        )))
        .wrap(Wrap { trim: true })
        .block(block);
        frame.render_widget(aviso, area);
        return;
    }
    if let Some(d) = &model.drill {
        if model.mode == Mode::Drilldown && d.vista == Vista::Ahora {
            let items = model.hosts_de_app(&d.app);
            let titulo = titulo_panel(&format!("hosts de {}", d.app), "ahora", None);
            draw_hist_panel(frame, area, &titulo, "host", &items, model.units, None);
            return;
        }
    }
    let apps: Vec<(String, u64, u64)> = model
        .top_apps()
        .into_iter()
        .filter(|(app, _, _)| {
            !filtro_activo || crate::live::state::nombre_coincide(app, &model.filtro)
        })
        .collect();
    let secs = model
        .samples
        .len()
        .clamp(1, crate::live::state::PANEL_WINDOW_SECS) as u64;
    let max = apps.iter().map(|(_, rx, tx)| rx + tx).max().unwrap_or(0);
    let enfocado = model.focus == Focus::Apps;
    // Aviso de filas ocultas: si hay más apps que espacio visible, se reserva la
    // última fila visible para "+N más — Enter para expandir" (Task 11).
    // -2 por los bordes, -1 por la fila de encabezado ("app ↓ ↑").
    let visibles = area.height.saturating_sub(3) as usize;
    let total_apps = apps.len();
    let ocultas = total_apps.saturating_sub(visibles);
    let filas_a_mostrar = if ocultas > 0 {
        visibles.saturating_sub(1)
    } else {
        visibles
    };
    // ÍTEM 4 — Fila #1 (mayor tráfico) en BOLD para guiar el ojo. Sutil — solo la primera.
    // Si el panel tiene el foco, la fila seleccionada (selected_row) se resalta en reverse.
    let mut rows: Vec<Row> = apps
        .iter()
        .take(filas_a_mostrar)
        .enumerate()
        .map(|(i, (app, rx, tx))| {
            let total = rx + tx;
            let barra = barra_inline(total, max, 10);
            let row = Row::new(vec![
                app.clone(),
                crate::render::format_rate(rx / secs, model.units),
                crate::render::format_rate(tx / secs, model.units),
                barra,
            ]);
            let serie = serie_app(model, app);
            let suave = crate::live::state::serie_suavizada(
                &serie,
                crate::live::state::SUAVIZADO_UMBRAL_SECS,
            );
            let actual = suave.last().copied().unwrap_or(0);
            let nivel = crate::live::state::nivel_umbral(&suave, actual);
            let mut row_style = if i == 0 {
                Style::default().add_modifier(Modifier::BOLD)
            } else {
                Style::default()
            };
            if nivel != Nivel::Verde {
                row_style = row_style.fg(color_nivel(nivel));
            }
            if enfocado && i == model.selected_row {
                row_style = row_style.add_modifier(Modifier::REVERSED);
            }
            row.style(row_style)
        })
        .collect();
    if ocultas > 0 {
        rows.push(
            Row::new(vec![
                format!("+{ocultas} más — Enter para expandir"),
                String::new(),
                String::new(),
                String::new(),
            ])
            .style(Style::default().fg(Color::DarkGray)),
        );
    }
    let table = Table::new(
        rows,
        [
            Constraint::Min(20),
            Constraint::Length(11),
            Constraint::Length(11),
            Constraint::Length(11),
        ],
    )
    .header(Row::new(vec!["app", "↓", "↑", ""]).style(Style::default().fg(Color::DarkGray)))
    .block(block);
    frame.render_widget(table, area);
}

/// Paleta de colores para las entidades del histórico (franja + nombres).
/// Deliberadamente SIN cyan ni magenta: esos son los colores del dato EN VIVO
/// (gráficos ↓ cyan / ↑ magenta), y compartirlos daría la falsa impresión de que
/// lo vivo alimenta directamente a una entidad del histórico. Índices 0..6 →
/// colores distintos; 6+ → DarkGray ("otros").
fn color_entidad(i: usize) -> Color {
    const PALETA: [Color; 6] = [
        Color::Green,
        Color::Yellow,
        Color::Blue,
        Color::Red,
        Color::LightGreen,
        Color::LightRed,
    ];
    if i < PALETA.len() {
        PALETA[i]
    } else {
        Color::DarkGray
    }
}

/// Calcula los anchos (en columnas) de los segmentos de la franja para
/// los valores dados sobre un ancho total. Devuelve (ancho, índice de color).
/// El índice de color = posición en la lista (≥6 cae a gris vía color_entidad).
fn segmentos_franja(valores: &[u64], ancho: usize) -> Vec<(usize, usize)> {
    let total: u64 = valores.iter().sum();
    if total == 0 || ancho == 0 {
        return Vec::new();
    }
    // Clamp acumulado: la suma de segmentos nunca excede `ancho` (el redondeo
    // por separado podría sumar 1-2 de más con muchas entidades casi iguales).
    let mut restante = ancho;
    valores
        .iter()
        .enumerate()
        .map(|(i, &v)| {
            let w = (((v as f64 / total as f64) * ancho as f64).round() as usize).min(restante);
            restante -= w;
            (w, i)
        })
        .collect()
}

/// Dibuja un panel de histórico: Block con franja de proporción arriba y
/// tabla (nombre coloreado / ↓ / ↑ / total / %) abajo. `items` ya ordenado desc.
/// `seleccionado` = índice de la fila resaltada en reverse (Some solo si el panel
/// tiene el foco de navegación); None => sin resaltado.
fn draw_hist_panel(
    frame: &mut Frame,
    area: Rect,
    titulo: &str,
    col_nombre: &str,
    items: &[(String, u64, u64)], // (nombre, rx, tx), ordenado desc por rx+tx
    units: crate::render::UnitMode,
    seleccionado: Option<usize>,
) {
    use ratatui::layout::{Direction, Layout};
    let block = Block::default()
        .borders(Borders::ALL)
        .title(titulo.to_string());
    let inner = block.inner(area);
    frame.render_widget(block, area);
    if inner.height == 0 {
        return;
    }

    let partes = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(1), Constraint::Min(0)])
        .split(inner);

    // Franja de proporción
    let total: u64 = items.iter().map(|(_, rx, tx)| rx + tx).sum();
    let valores: Vec<u64> = items.iter().map(|(_, rx, tx)| rx + tx).collect();
    let segs = segmentos_franja(&valores, partes[0].width as usize);
    let spans: Vec<Span> = segs
        .iter()
        .filter(|(w, _)| *w > 0)
        .map(|(w, i)| Span::styled("█".repeat(*w), Style::default().fg(color_entidad(*i))))
        .collect();
    frame.render_widget(Paragraph::new(Line::from(spans)), partes[0]);

    // Tabla con % y nombre coloreado. Aviso de filas ocultas: si hay más items que
    // espacio visible, se reserva la última fila visible para "+N más — Enter para
    // expandir" (Task 11).
    let visibles = partes[1].height.saturating_sub(1) as usize; // -1 por el header
    let total_items = items.len();
    let ocultas = total_items.saturating_sub(visibles);
    let filas_a_mostrar = if ocultas > 0 {
        visibles.saturating_sub(1)
    } else {
        visibles
    };
    let mut rows: Vec<Row> = items
        .iter()
        .take(filas_a_mostrar)
        .enumerate()
        .map(|(i, (nombre, rx, tx))| {
            let t = rx + tx;
            let pct = if total == 0 {
                0.0_f64
            } else {
                t as f64 / total as f64 * 100.0
            };
            let row = Row::new(vec![
                ratatui::widgets::Cell::from(Span::styled(
                    nombre.clone(),
                    Style::default().fg(color_entidad(i)),
                )),
                ratatui::widgets::Cell::from(format_units(*rx, units)),
                ratatui::widgets::Cell::from(format_units(*tx, units)),
                ratatui::widgets::Cell::from(format_units(t, units)),
                ratatui::widgets::Cell::from(format!("{pct:.1}%")),
            ]);
            if seleccionado == Some(i) {
                row.style(Style::default().add_modifier(Modifier::REVERSED))
            } else {
                row
            }
        })
        .collect();
    if ocultas > 0 {
        rows.push(
            Row::new(vec![
                ratatui::widgets::Cell::from(format!("+{ocultas} más — Enter para expandir")),
                ratatui::widgets::Cell::from(""),
                ratatui::widgets::Cell::from(""),
                ratatui::widgets::Cell::from(""),
                ratatui::widgets::Cell::from(""),
            ])
            .style(Style::default().fg(Color::DarkGray)),
        );
    }
    let table = Table::new(
        rows,
        [
            Constraint::Min(8),
            Constraint::Length(10),
            Constraint::Length(10),
            Constraint::Length(10),
            Constraint::Length(6),
        ],
    )
    .header(
        Row::new(vec![col_nombre, "↓ recib.", "↑ env.", "total", "%"])
            .style(Style::default().fg(Color::DarkGray)),
    );
    frame.render_widget(table, partes[1]);
}

/// Rango histórico activo, como texto para `titulo_panel`, o `None` si es "todo"
/// (el caso por defecto no necesita recordatorio en el título).
fn rango_titulo(model: &LiveModel) -> Option<&'static str> {
    let tok = crate::live::state::RANGOS[model.hist_range_idx];
    (tok != "todo").then_some(tok)
}

/// Compone el `extra` de `titulo_panel` combinando la selección (`sel N/M`) con el
/// rango histórico activo cuando ambos aplican.
fn extra_con_rango(seleccion: Option<String>, rango: Option<&str>) -> Option<String> {
    match (seleccion, rango) {
        (Some(s), Some(r)) => Some(format!("{s} · {r}")),
        (Some(s), None) => Some(s),
        (None, Some(r)) => Some(r.to_string()),
        (None, None) => None,
    }
}

/// Panel de servidores (top de todas las apps): Ahora=top_hosts_vivo / Historico=servidores_hist.
/// Resalta `selected_row` cuando el panel tiene el foco de navegación.
fn draw_servidores(frame: &mut Frame, model: &LiveModel, area: Rect) {
    type Items = Vec<(String, u64, u64)>;
    if !model.ebpf {
        let block =
            Block::default()
                .borders(Borders::ALL)
                .title(titulo_panel("hosts", "ahora", None));
        let aviso = Paragraph::new(Line::from(Span::styled(
            "eBPF inactivo — corre el daemon con sudo para ver hosts",
            Style::default().fg(Color::Yellow),
        )))
        .wrap(Wrap { trim: true })
        .block(block);
        frame.render_widget(aviso, area);
        return;
    }
    let (items, ambito, extra): (Items, &str, Option<String>) = match model.vista_servidores {
        Vista::Ahora => (model.top_hosts_vivo(), "ahora", None),
        Vista::Historico => {
            let tok = crate::live::state::RANGOS[model.hist_range_idx];
            let items = model
                .servidores_hist
                .iter()
                .map(|r| (r.host.clone(), r.rx, r.tx))
                .collect();
            (items, "histórico", (tok != "todo").then(|| tok.to_string()))
        }
    };
    let titulo = titulo_panel("hosts", ambito, extra.as_deref());
    let sel = (model.focus == Focus::Servidores).then_some(model.selected_row);
    draw_hist_panel(frame, area, &titulo, "host", &items, model.units, sel);
}

/// Panel de histórico de apps: totales acumulados, orden desc. Aviso si eBPF inactivo.
/// Si hay un drill activo desde este panel (`Mode::Drilldown`), dibuja en su lugar la
/// sub-vista de hosts remotos de la app seleccionada (reutiliza `draw_hist_panel`).
pub fn draw_hist_apps(frame: &mut Frame, model: &LiveModel, area: Rect) {
    let rango = rango_titulo(model);
    if !model.ebpf {
        let block =
            Block::default()
                .borders(Borders::ALL)
                .title(titulo_panel("apps", "histórico", rango));
        let aviso = Paragraph::new(Line::from(Span::styled(
            "eBPF inactivo — corre el daemon con sudo para ver apps",
            Style::default().fg(Color::Yellow),
        )))
        .wrap(Wrap { trim: true })
        .block(block);
        frame.render_widget(aviso, area);
        return;
    }
    if let Some(d) = &model.drill {
        if model.mode == Mode::Drilldown && d.vista == Vista::Historico {
            let items: Vec<(String, u64, u64)> = model
                .drill_hosts
                .iter()
                .map(|r| (r.host.clone(), r.rx, r.tx))
                .collect();
            let titulo = titulo_panel(&format!("hosts de {}", d.app), "histórico", rango);
            draw_hist_panel(frame, area, &titulo, "host", &items, model.units, None);
            return;
        }
    }
    let filtro_activo = !model.filtro.is_empty() && model.focus == Focus::Apps;
    let mut items: Vec<(String, u64, u64)> = model
        .hist_apps
        .iter()
        .map(|r| (r.app.clone(), r.rx, r.tx))
        .filter(|(app, _, _)| {
            !filtro_activo || crate::live::state::nombre_coincide(app, &model.filtro)
        })
        .collect();
    items.sort_by_key(|(_, rx, tx)| std::cmp::Reverse(rx + tx));
    let enfocado = model.focus == Focus::Apps;
    let seleccionado = enfocado.then_some(model.selected_row);
    let seleccion = (enfocado && !items.is_empty())
        .then(|| format!("sel {}/{}", model.selected_row + 1, items.len()));
    let extra = extra_con_rango(seleccion, rango);
    let titulo = titulo_panel("apps", "histórico", extra.as_deref());
    draw_hist_panel(
        frame,
        area,
        &titulo,
        "app",
        &items,
        model.units,
        seleccionado,
    );
}

/// Panel de histórico de interfaces: totales acumulados, orden desc. Oculta lo salvo show_all.
pub fn draw_hist_ifaces(frame: &mut Frame, model: &LiveModel, area: Rect) {
    let mut items: Vec<(String, u64, u64)> = model
        .hist_ifaces
        .iter()
        .filter(|r| model.show_all || r.iface != "lo")
        .map(|r| (r.iface.clone(), r.rx, r.tx))
        .collect();
    items.sort_by_key(|(_, rx, tx)| std::cmp::Reverse(rx + tx));
    let enfocado = model.focus == Focus::Interfaces;
    let seleccionado = enfocado.then_some(model.selected_row);
    let seleccion = (enfocado && !items.is_empty())
        .then(|| format!("sel {}/{}", model.selected_row + 1, items.len()));
    let extra = extra_con_rango(seleccion, rango_titulo(model));
    let titulo = titulo_panel("interfaces", "histórico", extra.as_deref());
    draw_hist_panel(
        frame,
        area,
        &titulo,
        "iface",
        &items,
        model.units,
        seleccionado,
    );
}

/// Panel de interfaces con tasas, sparkline y (si existe) IP en gris indentada.
pub fn draw_ifaces(frame: &mut Frame, model: &LiveModel, area: Rect) {
    let filtro_activo = !model.filtro.is_empty() && model.focus == Focus::Interfaces;
    let extra = filtro_activo.then(|| format!("filtro: {}", model.filtro));
    let titulo = titulo_panel("interfaces", "ahora", extra.as_deref());
    let block = Block::default().borders(Borders::ALL).title(titulo);
    let secs = model
        .samples
        .len()
        .clamp(1, crate::live::state::PANEL_WINDOW_SECS) as u64;
    let max_lines = area.height.saturating_sub(2) as usize; // dentro de los bordes
    let mut lines: Vec<Line> = Vec::new();
    let enfocado = model.focus == Focus::Interfaces;
    let filas: Vec<_> = model
        .iface_rows()
        .into_iter()
        .filter(|(iface, _, _, _)| {
            !filtro_activo || crate::live::state::nombre_coincide(iface, &model.filtro)
        })
        .collect();
    // Aviso de filas ocultas: cada interfaz ocupa 1 línea (o 2 si tiene IP). Si al
    // menos una no cabe en el espacio visible, se reserva la última línea visible
    // para "+N más — Enter para expandir" (Task 11, mismo patrón que draw_apps/
    // draw_hist_panel) en vez de recortar en silencio.
    let lineas_por_fila: Vec<usize> = filas
        .iter()
        .map(|(iface, _, _, _)| 1 + usize::from(model.iface_ips.contains_key(iface)))
        .collect();
    let cabe_todo = lineas_por_fila.iter().sum::<usize>() <= max_lines;
    let limite_lineas = if cabe_todo {
        max_lines
    } else {
        max_lines.saturating_sub(1)
    };
    let mut visibles = 0usize;
    let mut acumulado = 0usize;
    for &n in &lineas_por_fila {
        if acumulado + n > limite_lineas {
            break;
        }
        acumulado += n;
        visibles += 1;
    }
    let ocultas = filas.len() - visibles;
    // ÍTEM 4 — Fila #1 (mayor tráfico) en BOLD para guiar el ojo.
    // Si el panel tiene el foco, la fila seleccionada (selected_row) se resalta en reverse.
    for (row_idx, (iface, rx, tx, serie)) in filas.iter().enumerate() {
        if lines.len() >= limite_lineas {
            break;
        }
        let spark = crate::render::sparkline(&fit_series(serie, 8));
        let suave =
            crate::live::state::serie_suavizada(serie, crate::live::state::SUAVIZADO_UMBRAL_SECS);
        let actual = suave.last().copied().unwrap_or(0);
        let nivel = crate::live::state::nivel_umbral(&suave, actual);
        let mut row_style = if row_idx == 0 {
            Style::default().add_modifier(Modifier::BOLD)
        } else {
            Style::default()
        };
        if nivel != Nivel::Verde {
            row_style = row_style.fg(color_nivel(nivel));
        }
        if enfocado && row_idx == model.selected_row {
            row_style = row_style.add_modifier(Modifier::REVERSED);
        }
        let datos = Line::from(vec![
            Span::styled(format!("{:<8} ", recorta(iface, 8)), row_style),
            Span::styled(
                format!(
                    "{:>10} ",
                    crate::render::format_rate(rx / secs, model.units)
                ),
                row_style,
            ),
            Span::styled(
                format!(
                    "{:>10} ",
                    crate::render::format_rate(tx / secs, model.units)
                ),
                row_style,
            ),
            Span::styled(spark, Style::default().fg(Color::Cyan)),
        ]);
        lines.push(datos);
        if let Some(ip) = model.iface_ips.get(iface) {
            if lines.len() < limite_lineas {
                lines.push(Line::from(Span::styled(
                    format!("   {ip}"),
                    Style::default().fg(Color::DarkGray),
                )));
            }
        }
    }
    if ocultas > 0 {
        lines.push(Line::from(Span::styled(
            format!("+{ocultas} más — Enter para expandir"),
            Style::default().fg(Color::DarkGray),
        )));
    }
    let para = Paragraph::new(lines).block(block);
    frame.render_widget(para, area);
}

/// Recorta un nombre a `n` caracteres (para que no desborde la columna).
fn recorta(s: &str, n: usize) -> String {
    if s.chars().count() <= n {
        s.to_string()
    } else {
        s.chars().take(n).collect()
    }
}

/// Ajusta una serie a exactamente `width` puntos anclando a la DERECHA:
/// los últimos `width` valores, rellenando con ceros a la izquierda si hay menos
/// (el dato más reciente queda en el borde derecho).
fn fit_series(series: &[u64], width: usize) -> Vec<u64> {
    if width == 0 {
        return Vec::new();
    }
    if series.len() >= width {
        series[series.len() - width..].to_vec()
    } else {
        let mut v = vec![0u64; width - series.len()];
        v.extend_from_slice(series);
        v
    }
}

/// Edad relativa compacta: 0 -> "ahora"; <60s -> "{s}s"; >=60s -> "{m}min" (redondeo a minuto).
fn edad_relativa(secs: u64) -> String {
    if secs == 0 {
        "ahora".to_string()
    } else if secs < 60 {
        format!("{secs}s")
    } else {
        format!("{}min", (secs + 30) / 60)
    }
}

/// Marcas del eje X: ~4 columnas (izq, 1/3, 2/3, der) con su edad; la de la derecha = "ahora".
/// Cada columna = 1 s; edad de la columna c = (inner_w-1 - c).
fn ticks_eje_x(inner_w: usize) -> Vec<(usize, String)> {
    if inner_w == 0 {
        return Vec::new();
    }
    let last = inner_w - 1;
    let mut cols = vec![0usize];
    if inner_w >= 3 {
        cols.push(last / 3);
        cols.push(2 * last / 3);
    }
    cols.push(last);
    cols.sort_unstable();
    cols.dedup();
    cols.into_iter()
        .map(|c| (c, edad_relativa((last - c) as u64)))
        .collect()
}

/// Columna (0..inner_w) del pico si cae dentro de la franja visible; None si quedó fuera (izquierda).
fn pico_col_visible(peak_age_secs: u64, inner_w: usize) -> Option<usize> {
    if inner_w == 0 {
        return None;
    }
    let last = inner_w - 1;
    if (peak_age_secs as usize) <= last {
        Some(last - peak_age_secs as usize)
    } else {
        None
    }
}

/// Ancho del canalón del eje Y = max(len de la etiqueta de arriba, len de "0 B") + 1, clamp(6,10).
fn ancho_canalon_y(max_visible: u64, units: crate::render::UnitMode) -> u16 {
    let top = format_units(max_visible, units);
    let bot = format_units(0, units);
    let w = top.chars().count().max(bot.chars().count()) as u16 + 1;
    w.clamp(6, 10)
}

/// Barra unicode proporcional al máximo (para el panel de apps).
fn barra_inline(v: u64, max: u64, width: usize) -> String {
    if max == 0 {
        return String::new();
    }
    let filled = ((v as f64 / max as f64) * width as f64).round() as usize;
    let filled = filled.min(width);
    format!("{}{}", "█".repeat(filled), "░".repeat(width - filled))
}

#[cfg(test)]
mod tests {
    use super::{
        draw, draw_apps, draw_charts, draw_footer, draw_header, draw_hist_apps, draw_hist_ifaces,
        draw_ifaces,
    };
    use crate::live::state::LiveModel;
    use ratatui::{
        backend::TestBackend,
        layout::Rect,
        style::{Color, Modifier},
        Terminal,
    };

    fn render_area(
        model: &LiveModel,
        w: u16,
        h: u16,
        f: impl Fn(&mut ratatui::Frame, &LiveModel, Rect),
    ) -> String {
        let backend = TestBackend::new(w, h);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal
            .draw(|frame| {
                let area = frame.area();
                f(frame, model, area);
            })
            .unwrap();
        let buf = terminal.backend().buffer().clone();
        buf.content().iter().map(|c| c.symbol()).collect()
    }

    #[test]
    fn cabecera_muestra_version_y_ebpf_activo() {
        let m = LiveModel {
            version: "0.1.0".into(),
            ebpf: true,
            connected: true,
            ..Default::default()
        };
        let s = render_area(&m, 80, 1, draw_header);
        assert!(s.contains("0.1.0"), "debe mostrar versión: {s:?}");
        assert!(s.contains("eBPF"), "debe mencionar eBPF: {s:?}");
        assert!(s.contains("activo"));
    }

    #[test]
    fn cabecera_muestra_desconectado() {
        let m = LiveModel {
            connected: false,
            ..Default::default()
        };
        let s = render_area(&m, 80, 1, draw_header);
        assert!(s.contains("desconectado"), "{s:?}");
    }

    #[test]
    fn pie_muestra_teclas() {
        let m = LiveModel::default();
        let s = render_area(&m, 80, 1, draw_footer);
        assert!(s.contains('q') && s.to_lowercase().contains("salir"));
    }

    #[test]
    fn pie_muestra_tagline_parlox_en_ancho() {
        let m = LiveModel::default();
        let s = render_area(&m, 110, 1, draw_footer);
        assert!(
            s.to_lowercase().contains("salir"),
            "comandos presentes: {s:?}"
        );
        assert!(s.contains("Parlox Network"), "tagline presente: {s:?}");
        assert!(s.contains('❤'), "corazón presente: {s:?}");
    }

    #[test]
    fn pie_sin_tagline_en_ancho_estrecho() {
        let m = LiveModel::default();
        let s = render_area(&m, 50, 1, draw_footer);
        // los comandos siempre están; el tagline puede no caber en estrecho
        assert!(s.to_lowercase().contains("salir"), "{s:?}");
    }

    #[test]
    fn pie_no_trunca_comandos_en_franja_intermedia() {
        // a 85 cols (entre 80 y 89) NO debe aparecer el tagline ni truncarse el
        // modo de unidades: el pie queda solo con comandos completos.
        let m = LiveModel::default();
        let s = render_area(&m, 85, 1, draw_footer);
        assert!(
            s.contains("unidades: KiB"),
            "comandos completos sin truncar: {s:?}"
        );
        assert!(
            !s.contains("Parlox"),
            "sin tagline en franja intermedia: {s:?}"
        );
    }

    #[test]
    fn titulo_panel_con_y_sin_extra() {
        use super::titulo_panel;
        assert_eq!(titulo_panel("apps", "ahora", None).trim(), "apps · ahora");
        assert_eq!(
            titulo_panel("apps", "ahora", Some("filtro: fire")).trim(),
            "apps · ahora · filtro: fire"
        );
    }

    #[test]
    fn segmentos_pie_termina_en_ayuda() {
        use super::segmentos_pie;
        use crate::live::state::{Focus, Mode};
        let segs = segmentos_pie(Mode::Normal, Focus::Grafico);
        assert_eq!(segs.last().map(String::as_str), Some("? ayuda"));
        assert!(segs.iter().any(|s| s.contains("[NORMAL]")), "{segs:?}");
    }

    #[test]
    fn pie_muestra_indicador_de_modo_normal() {
        let m = LiveModel::default();
        let s = render_area(&m, 80, 1, draw_footer);
        assert!(s.contains("[NORMAL]"), "{s:?}");
    }

    #[test]
    fn apps_avisa_filas_ocultas_cuando_hay_mas_de_las_visibles() {
        let mut m = LiveModel {
            ebpf: true,
            ..Default::default()
        };
        let apps: Vec<(&str, u64, u64)> = (0..15)
            .map(|i| {
                (
                    Box::leak(format!("app{i}").into_boxed_str()) as &str,
                    100,
                    10,
                )
            })
            .collect();
        m.samples = vec![crate::live::state::tests_support::sample_apps(0, &apps)];
        // área baja: solo caben ~8 filas de las 15
        let s = render_area(&m, 50, 10, draw_apps);
        assert!(s.contains("más"), "debe avisar filas ocultas: {s:?}");
        assert!(s.contains("Enter"), "debe indicar cómo expandir: {s:?}");
    }

    #[test]
    fn hist_apps_avisa_filas_ocultas() {
        use caudalghost_core::proto::AppRow;
        let m = LiveModel {
            ebpf: true,
            hist_apps: (0..15)
                .map(|i| AppRow {
                    app: format!("app{i}"),
                    rx: (15 - i) as u64 * 1000,
                    tx: 10,
                })
                .collect(),
            ..Default::default()
        };
        let s = render_area(&m, 50, 8, draw_hist_apps);
        assert!(s.contains("más"), "debe avisar filas ocultas: {s:?}");
    }

    #[test]
    fn charts_lado_a_lado_muestran_ambas_flechas() {
        let m = LiveModel {
            show_all: false,
            samples: vec![
                crate::live::state::tests_support::sample(0, &[("eth0", 1024, 512)]),
                crate::live::state::tests_support::sample(1, &[("eth0", 2048, 1024)]),
            ],
            ..Default::default()
        };
        let s = render_area(&m, 100, 10, draw_charts);
        assert!(s.contains('↓'), "debe mostrar ↓ (rx): {s:?}");
        assert!(s.contains('↑'), "debe mostrar ↑ (tx): {s:?}");
        assert!(s.contains("KiB"), "debe mostrar unidades: {s:?}");
    }

    #[test]
    fn grafico_vacio_no_panica() {
        let m = LiveModel::default();
        let s = render_area(&m, 80, 10, draw_charts);
        assert!(!s.is_empty());
    }

    #[test]
    fn edad_relativa_formatea() {
        assert_eq!(super::edad_relativa(0), "ahora");
        assert_eq!(super::edad_relativa(45), "45s");
        assert_eq!(super::edad_relativa(60), "1min");
        assert_eq!(super::edad_relativa(420), "7min");
    }

    #[test]
    fn ticks_eje_x_valido_y_ultima_ahora() {
        let t = super::ticks_eje_x(40);
        assert!(!t.is_empty());
        assert!(
            t.iter().all(|(c, _)| *c < 40),
            "columnas dentro de rango: {t:?}"
        );
        assert_eq!(t.last().unwrap().1, "ahora");
        assert!(super::ticks_eje_x(0).is_empty());
    }

    #[test]
    fn pico_col_visible_dentro_y_fuera() {
        assert_eq!(super::pico_col_visible(3, 10), Some(6)); // inner 10: last=9, col=9-3
        assert_eq!(super::pico_col_visible(0, 10), Some(9)); // ahora = borde derecho
        assert_eq!(super::pico_col_visible(20, 10), None); // fuera de la franja
        assert_eq!(super::pico_col_visible(0, 0), None);
    }

    #[test]
    fn ancho_canalon_y_clamp() {
        let w = super::ancho_canalon_y(95_000, crate::render::UnitMode::Binary);
        assert!((6..=10).contains(&w), "w={w}");
    }

    #[test]
    fn chart_titulo_muestra_edad_del_pico() {
        // pico en el sample más antiguo (edad alta), varios samples posteriores menores
        let mut samples = vec![crate::live::state::tests_support::sample(
            0,
            &[("eth0", 500_000, 0)],
        )];
        for i in 1..90 {
            samples.push(crate::live::state::tests_support::sample(
                i,
                &[("eth0", 1000, 0)],
            ));
        }
        let m = LiveModel {
            samples,
            ..Default::default()
        };
        let s = render_area(&m, 100, 12, draw_charts);
        assert!(
            s.contains("hace"),
            "el título debe indicar la antigüedad del pico: {s:?}"
        );
    }

    #[test]
    fn chart_dibuja_eje_x_con_ahora() {
        let m = LiveModel {
            samples: vec![
                crate::live::state::tests_support::sample(0, &[("eth0", 1024, 0)]),
                crate::live::state::tests_support::sample(1, &[("eth0", 2048, 0)]),
            ],
            ..Default::default()
        };
        let s = render_area(&m, 100, 12, draw_charts);
        assert!(s.contains("ahora"), "el eje X debe marcar 'ahora': {s:?}");
    }

    #[test]
    fn chart_sin_eje_x_en_alto_muy_bajo_no_panica() {
        let m = LiveModel {
            samples: vec![crate::live::state::tests_support::sample(
                0,
                &[("eth0", 1024, 0)],
            )],
            ..Default::default()
        };
        let s = render_area(&m, 100, 5, draw_charts); // height por gráfico < 6
        assert!(!s.is_empty());
        assert!(!s.contains("ahora"), "sin eje X en alto <6: {s:?}");
    }

    #[test]
    fn chart_dibuja_canalon_y_con_cero() {
        let m = LiveModel {
            samples: vec![crate::live::state::tests_support::sample(
                0,
                &[("eth0", 3_000_000, 0)],
            )],
            ..Default::default()
        };
        let s = render_area(&m, 100, 12, draw_charts);
        assert!(s.contains("0 B"), "el canalón Y debe mostrar el 0: {s:?}");
    }

    #[test]
    fn chart_canalon_y_arriba_coincide_con_el_maximo_dibujado() {
        // Repro del hallazgo crítico 0.6.1: un valor alto lejos en el pasado (fuera de la
        // ventana que `fit_series` va a dibujar, pero dentro de una estimación más ancha que
        // no descuenta aún el canalón Y) NO debe aparecer como la etiqueta "arriba" del eje Y.
        // area.width = 40 → gw = ancho_canalon_y(...) = 8 (ver comentario del hallazgo) →
        // plot.width = 32 → inner_w = 30. Colocamos el valor gigante en la columna 35 desde
        // el final: cae dentro de la estimación (ventana 38 = plot_area.width-2) pero FUERA
        // de la ventana final (30) que realmente se dibuja.
        let mut serie = vec![10u64; 50];
        let len = serie.len();
        serie[len - 36] = 99_999; // 35 posiciones antes del último índice (0-based)
        let ultimo = len - 1;
        serie[ultimo] = 100; // máximo real dentro de la ventana visible final
        let backend = ratatui::backend::TestBackend::new(40, 12);
        let mut terminal = ratatui::Terminal::new(backend).unwrap();
        terminal
            .draw(|f| {
                super::draw_chart_one(
                    f,
                    f.area(),
                    &serie,
                    100,
                    99_999,
                    35,
                    "↓",
                    Color::Cyan,
                    crate::render::UnitMode::Binary,
                    None,
                )
            })
            .unwrap();
        let buf = terminal.backend().buffer().clone();
        let s: String = buf.content().iter().map(|c| c.symbol()).collect();
        assert!(
            !s.contains("99999 B") && !s.contains("100.0 KiB"),
            "el canalón Y no debe mostrar un máximo fuera de la ventana dibujada: {s:?}"
        );
        assert!(
            s.contains("100 B"),
            "el canalón Y debe mostrar el máximo real de las barras visibles (100 B): {s:?}"
        );
    }

    #[test]
    fn chart_sin_canalon_y_en_ancho_estrecho_no_panica() {
        let m = LiveModel {
            samples: vec![crate::live::state::tests_support::sample(
                0,
                &[("eth0", 1024, 0)],
            )],
            ..Default::default()
        };
        let s = render_area(&m, 30, 12, draw_charts); // ~15 cols por gráfico -> sin canalón
        assert!(!s.is_empty());
    }

    #[test]
    fn panel_apps_lista_y_ordena() {
        let mut m = LiveModel {
            ebpf: true,
            ..Default::default()
        };
        m.samples = vec![crate::live::state::tests_support::sample_apps(
            0,
            &[("firefox", 100, 10), ("curl", 5, 5)],
        )];
        let s = render_area(&m, 50, 10, draw_apps);
        assert!(s.contains("firefox") && s.contains("curl"));
    }

    #[test]
    fn panel_apps_filtrado_muestra_solo_coincidencias() {
        use crate::live::state::Focus;
        let mut m = LiveModel {
            ebpf: true,
            focus: Focus::Apps,
            filtro: "fire".into(),
            ..Default::default()
        };
        m.samples = vec![crate::live::state::tests_support::sample_apps(
            0,
            &[("firefox", 100, 10), ("curl", 5, 5)],
        )];
        let s = render_area(&m, 50, 10, draw_apps);
        assert!(s.contains("firefox"), "coincide con el filtro: {s:?}");
        assert!(!s.contains("curl"), "no coincide, debe quedar fuera: {s:?}");
        assert!(
            s.contains("filtro: fire"),
            "título muestra el filtro: {s:?}"
        );
    }

    #[test]
    fn panel_apps_avisa_si_ebpf_inactivo() {
        let m = LiveModel {
            ebpf: false,
            ..Default::default()
        };
        let s = render_area(&m, 50, 10, draw_apps);
        assert!(
            s.to_lowercase().contains("ebpf") && s.to_lowercase().contains("sudo"),
            "debe avisar honestamente: {s:?}"
        );
    }

    #[test]
    fn aviso_ebpf_completo_a_ancho_estrecho() {
        let m = LiveModel {
            ebpf: false,
            ..Default::default()
        };
        // 45 cols = ancho real del panel apps con layout 55/45 de T7
        let s = render_area(&m, 45, 10, draw_apps);
        // con wrap, la parte accionable NO se pierde
        assert!(
            s.to_lowercase().contains("sudo"),
            "el aviso debe conservar 'sudo' al envolver: {s:?}"
        );
        assert!(
            s.to_lowercase().contains("ver apps") || s.to_lowercase().contains("apps"),
            "{s:?}"
        );
    }

    #[test]
    fn panel_ifaces_lista_interfaces() {
        let m = LiveModel {
            show_all: false,
            samples: vec![crate::live::state::tests_support::sample(
                0,
                &[("eth0", 1000, 100)],
            )],
            ..Default::default()
        };
        let s = render_area(&m, 50, 10, draw_ifaces);
        assert!(s.contains("eth0"));
    }

    #[test]
    fn ifaces_avisa_filas_ocultas_cuando_hay_mas_de_las_visibles() {
        let ifaces: Vec<(&str, u64, u64)> = (0..15)
            .map(|i| {
                (
                    Box::leak(format!("eth{i}").into_boxed_str()) as &str,
                    100,
                    10,
                )
            })
            .collect();
        let m = LiveModel {
            show_all: true,
            samples: vec![crate::live::state::tests_support::sample(0, &ifaces)],
            ..Default::default()
        };
        // área baja: solo caben unas pocas filas de las 15 interfaces
        let s = render_area(&m, 50, 10, draw_ifaces);
        assert!(s.contains("más"), "debe avisar filas ocultas: {s:?}");
        assert!(s.contains("Enter"), "debe indicar cómo expandir: {s:?}");
    }

    #[test]
    fn ifaces_respeta_modo_decimal() {
        let mut m = LiveModel {
            show_all: false,
            samples: vec![crate::live::state::tests_support::sample(
                0,
                &[("eth0", 1_500_000, 0)],
            )],
            ..Default::default()
        };
        // binario por defecto
        let s = render_area(&m, 60, 6, draw_ifaces);
        assert!(s.contains("MiB"), "binario debe mostrar MiB: {s:?}");
        // decimal
        m.units = crate::render::UnitMode::Decimal;
        let s = render_area(&m, 60, 6, draw_ifaces);
        assert!(
            s.contains("MB") && !s.contains("MiB"),
            "decimal debe mostrar MB: {s:?}"
        );
    }

    #[test]
    fn fit_series_ancla_a_la_derecha() {
        assert_eq!(super::fit_series(&[1, 2, 3], 5), vec![0, 0, 1, 2, 3]);
        assert_eq!(super::fit_series(&[1, 2, 3, 4, 5], 3), vec![3, 4, 5]);
        assert_eq!(super::fit_series(&[], 3), vec![0, 0, 0]);
        assert_eq!(super::fit_series(&[1, 2, 3], 0), Vec::<u64>::new());
    }

    #[test]
    fn ifaces_muestra_tasa_por_segundo_no_suma() {
        // 3 samples, eth0 suma 3000 bytes → tasa = 1000 B/s, NO 3000
        let m = LiveModel {
            show_all: false,
            samples: vec![
                crate::live::state::tests_support::sample(0, &[("eth0", 1000, 0)]),
                crate::live::state::tests_support::sample(1, &[("eth0", 1000, 0)]),
                crate::live::state::tests_support::sample(2, &[("eth0", 1000, 0)]),
            ],
            ..Default::default()
        };
        let s = render_area(&m, 60, 6, draw_ifaces);
        // tasa por segundo = 3000/3 = 1000 B/s (binario: "1000 B/s"), NO "2.9 KiB/s" (que sería la suma 3000)
        assert!(
            s.contains("1000 B/s"),
            "debe mostrar tasa/s (1000 B/s), no la suma: {s:?}"
        );
    }

    #[test]
    fn ifaces_muestra_ip_indentada() {
        use std::collections::HashMap;
        let mut ips = HashMap::new();
        ips.insert("eth0".to_string(), "192.168.1.42".to_string());
        let m = LiveModel {
            show_all: false,
            samples: vec![crate::live::state::tests_support::sample(
                0,
                &[("eth0", 1000, 100)],
            )],
            iface_ips: ips,
            ..Default::default()
        };
        let s = render_area(&m, 50, 8, draw_ifaces);
        assert!(s.contains("eth0"), "debe mostrar la interfaz: {s:?}");
        assert!(s.contains("192.168.1.42"), "debe mostrar la IP: {s:?}");
        // la IP va indentada: aparece precedida de espacios (no pegada al borde)
        assert!(
            s.contains("   192.168.1.42"),
            "la IP debe ir indentada: {s:?}"
        );
    }

    #[test]
    fn ifaces_sin_ip_no_rompe() {
        let m = LiveModel {
            show_all: false,
            samples: vec![crate::live::state::tests_support::sample(
                0,
                &[("eth0", 1000, 100)],
            )],
            ..Default::default() // iface_ips vacío
        };
        let s = render_area(&m, 50, 8, draw_ifaces);
        assert!(s.contains("eth0"));
        // sin IP no debe haber línea indentada con dígitos de IP
    }

    #[test]
    fn hist_apps_lista_totales_ordenados() {
        use caudalghost_core::proto::AppRow;
        let m = LiveModel {
            ebpf: true,
            hist_apps: vec![
                AppRow {
                    app: "claude".into(),
                    rx: 1_000_000,
                    tx: 100,
                },
                AppRow {
                    app: "megasync".into(),
                    rx: 50_000_000,
                    tx: 2000,
                },
            ],
            ..Default::default()
        };
        let s = render_area(&m, 50, 8, draw_hist_apps);
        assert!(s.contains("megasync") && s.contains("claude"));
        // megasync (mayor total) va primero: aparece en una fila más arriba que claude
        let pos_mega = s.find("megasync").unwrap();
        let pos_claude = s.find("claude").unwrap();
        assert!(
            pos_mega < pos_claude,
            "megasync (mayor) debe ir primero: {s:?}"
        );
    }

    #[test]
    fn hist_apps_titulo_muestra_seleccion_via_titulo_panel_cuando_enfocado() {
        use crate::live::state::Focus;
        use caudalghost_core::proto::AppRow;
        let m = LiveModel {
            ebpf: true,
            focus: Focus::Apps,
            selected_row: 1,
            hist_apps: vec![
                AppRow {
                    app: "claude".into(),
                    rx: 1_000_000,
                    tx: 100,
                },
                AppRow {
                    app: "megasync".into(),
                    rx: 50_000_000,
                    tx: 2000,
                },
            ],
            ..Default::default()
        };
        let s = render_area(&m, 50, 8, draw_hist_apps);
        assert!(
            s.contains("sel 2/2"),
            "titulo_panel debe exponer sel N/M: {s:?}"
        );
        // sin foco, no debe aparecer el hueco de selección
        let m2 = LiveModel {
            ebpf: true,
            focus: Focus::Grafico,
            selected_row: 1,
            hist_apps: m.hist_apps.clone(),
            ..Default::default()
        };
        let s2 = render_area(&m2, 50, 8, draw_hist_apps);
        assert!(!s2.contains("sel "), "sin foco no hay sel N/M: {s2:?}");
    }

    #[test]
    fn hist_titulo_muestra_rango_cuando_no_es_todo() {
        use caudalghost_core::proto::TotalRow;
        let mut m = LiveModel {
            hist_ifaces: vec![TotalRow {
                iface: "eth0".into(),
                rx: 10,
                tx: 1,
            }],
            ..Default::default()
        };
        m.hist_range_idx = 3; // "7d"
        let s = render_area(&m, 60, 12, draw_hist_ifaces);
        assert!(
            s.contains("7d"),
            "el título del histórico debe mostrar el rango: {s:?}"
        );
    }

    #[test]
    fn hist_apps_drill_muestra_hosts() {
        use crate::live::state::{Drill, Focus, Mode, Vista};
        use caudalghost_core::proto::{AppRow, RemoteRow};
        let m = LiveModel {
            ebpf: true,
            focus: Focus::Apps,
            vista_apps: Vista::Historico,
            mode: Mode::Drilldown,
            drill: Some(Drill {
                app: "firefox".into(),
                vista: Vista::Historico,
            }),
            drill_hosts: vec![RemoteRow {
                host: "github.com".into(),
                rx: 100,
                tx: 1,
            }],
            hist_apps: vec![AppRow {
                app: "firefox".into(),
                rx: 100,
                tx: 1,
            }],
            ..Default::default()
        };
        let s = render_area(&m, 60, 12, draw_hist_apps);
        assert!(
            s.contains("github.com"),
            "drill histórico muestra los hosts: {s:?}"
        );
        assert!(
            s.to_lowercase().contains("firefox"),
            "título con la app: {s:?}"
        );
    }

    #[test]
    fn apps_vivo_drill_muestra_hosts_de_la_app() {
        use crate::live::state::{Drill, Focus, Mode, Vista};
        use caudalghost_core::types::{AppRemoteDelta, Sample};
        let m = LiveModel {
            ebpf: true,
            focus: Focus::Apps,
            mode: Mode::Drilldown,
            drill: Some(Drill {
                app: "firefox".into(),
                vista: Vista::Ahora,
            }),
            samples: vec![Sample {
                ts: 0,
                deltas: vec![],
                apps: vec![],
                flows: vec![AppRemoteDelta {
                    app: "firefox".into(),
                    host: "scdn.co".into(),
                    rx: 50,
                    tx: 5,
                }],
            }],
            ..Default::default()
        };
        let s = render_area(&m, 50, 12, draw_apps);
        assert!(
            s.contains("scdn.co"),
            "drill en vivo muestra los hosts de la app: {s:?}"
        );
    }

    #[test]
    fn hist_ifaces_titulo_muestra_seleccion_via_titulo_panel_cuando_enfocado() {
        use crate::live::state::Focus;
        use caudalghost_core::proto::TotalRow;
        let m = LiveModel {
            show_all: true,
            focus: Focus::Interfaces,
            selected_row: 0,
            hist_ifaces: vec![
                TotalRow {
                    iface: "wlp2s0".into(),
                    rx: 45_000_000_000,
                    tx: 600_000,
                },
                TotalRow {
                    iface: "lo".into(),
                    rx: 999,
                    tx: 999,
                },
            ],
            ..Default::default()
        };
        let s = render_area(&m, 50, 8, draw_hist_ifaces);
        assert!(
            s.contains("sel 1/2"),
            "titulo_panel debe exponer sel N/M: {s:?}"
        );
    }

    #[test]
    fn hist_apps_avisa_si_ebpf_inactivo() {
        let m = LiveModel {
            ebpf: false,
            ..Default::default()
        };
        let s = render_area(&m, 50, 8, draw_hist_apps);
        assert!(
            s.to_lowercase().contains("ebpf") && s.to_lowercase().contains("sudo"),
            "{s:?}"
        );
    }

    #[test]
    fn hist_ifaces_oculta_lo_y_muestra_totales() {
        use caudalghost_core::proto::TotalRow;
        let m = LiveModel {
            show_all: false,
            hist_ifaces: vec![
                TotalRow {
                    iface: "wlp2s0".into(),
                    rx: 45_000_000_000,
                    tx: 600_000,
                },
                TotalRow {
                    iface: "lo".into(),
                    rx: 999,
                    tx: 999,
                },
            ],
            ..Default::default()
        };
        let s = render_area(&m, 50, 8, draw_hist_ifaces);
        assert!(s.contains("wlp2s0"), "{s:?}");
        assert!(!s.contains("lo "), "lo debe ocultarse sin show_all: {s:?}");
        assert!(s.contains("GiB"), "totales grandes en GiB: {s:?}");
    }

    #[test]
    fn draw_completo_compone_todas_las_regiones() {
        let mut m = LiveModel {
            version: "0.1.0".into(),
            ebpf: true,
            connected: true,
            ..Default::default()
        };
        m.samples = vec![crate::live::state::tests_support::sample(
            0,
            &[("eth0", 1024, 512)],
        )];
        m.samples
            .push(crate::live::state::tests_support::sample_apps(
                1,
                &[("firefox", 2048, 256)],
            ));
        let backend = ratatui::backend::TestBackend::new(100, 40);
        let mut terminal = ratatui::Terminal::new(backend).unwrap();
        terminal.draw(|f| draw(f, &m)).unwrap();
        let buf = terminal.backend().buffer().clone();
        let s: String = buf.content().iter().map(|c| c.symbol()).collect();
        assert!(s.contains("CaudalGhost"), "cabecera");
        assert!(s.contains("eth0"), "panel ifaces");
        assert!(s.contains("firefox"), "panel apps");
        assert!(s.contains("salir"), "pie");
    }

    #[test]
    fn draw_completo_tres_bandas() {
        use crate::live::state::Vista;
        use caudalghost_core::proto::{AppRow, TotalRow};
        let mut m = LiveModel {
            version: "0.1.0".into(),
            ebpf: true,
            connected: true,
            vista_apps: Vista::Historico,
            ..Default::default()
        };
        m.samples = vec![
            crate::live::state::tests_support::sample(0, &[("eth0", 1024, 512)]),
            crate::live::state::tests_support::sample_apps(1, &[("firefox", 2048, 256)]),
        ];
        m.hist_apps = vec![AppRow {
            app: "megasync".into(),
            rx: 9_000_000,
            tx: 100,
        }];
        m.hist_ifaces = vec![TotalRow {
            iface: "eth0".into(),
            rx: 9_000_000,
            tx: 100,
        }];
        let backend = ratatui::backend::TestBackend::new(120, 40); // alto → con banda de servidores
        let mut terminal = ratatui::Terminal::new(backend).unwrap();
        terminal.draw(|f| draw(f, &m)).unwrap();
        let buf = terminal.backend().buffer().clone();
        let s: String = buf.content().iter().map(|c| c.symbol()).collect();
        assert!(s.contains("CaudalGhost"), "cabecera");
        assert!(s.contains("ahora"), "banda interfaces (título · ahora)");
        assert!(
            s.contains("histórico"),
            "banda apps en histórico (vista_apps)"
        );
        assert!(s.contains("megasync"), "dato histórico");
        assert!(s.contains("hosts"), "banda hosts");
        assert!(s.contains("salir"), "pie");
    }

    #[test]
    fn charts_scrub_sigue_funcionando_con_ejes() {
        // en pausa+scrub, el gráfico con ejes no entra en pánico y respeta el cursor
        let mut samples = Vec::new();
        for i in 0..30 {
            samples.push(crate::live::state::tests_support::sample(
                i,
                &[("eth0", (i as u64 + 1) * 1000, 0)],
            ));
        }
        let m = LiveModel {
            mode: crate::live::state::Mode::PausaScrub,
            paused: true,
            scrub_idx: Some(5),
            samples,
            ..Default::default()
        };
        let s = render_area(&m, 100, 12, draw_charts);
        assert!(s.contains("ahora"));
    }

    #[test]
    fn draw_completo_con_ejes_no_panica() {
        let mut m = LiveModel {
            version: "0.6.1".into(),
            ebpf: true,
            connected: true,
            ..Default::default()
        };
        m.samples = vec![
            crate::live::state::tests_support::sample(0, &[("eth0", 1024, 512)]),
            crate::live::state::tests_support::sample_apps(1, &[("firefox", 2048, 256)]),
        ];
        let backend = ratatui::backend::TestBackend::new(120, 40);
        let mut terminal = ratatui::Terminal::new(backend).unwrap();
        terminal.draw(|f| draw(f, &m)).unwrap();
        let buf = terminal.backend().buffer().clone();
        let s: String = buf.content().iter().map(|c| c.symbol()).collect();
        assert!(s.contains("CaudalGhost") && s.contains("ahora") && s.contains("salir"));
    }

    #[test]
    fn draw_sin_historico_en_altura_baja() {
        let m = LiveModel {
            version: "0.1.0".into(),
            connected: true,
            ..Default::default()
        };
        let backend = ratatui::backend::TestBackend::new(120, 20); // bajo → sin histórico
        let mut terminal = ratatui::Terminal::new(backend).unwrap();
        terminal.draw(|f| draw(f, &m)).unwrap();
        let buf = terminal.backend().buffer().clone();
        let s: String = buf.content().iter().map(|c| c.symbol()).collect();
        assert!(s.contains("CaudalGhost") && s.contains("salir"));
        assert!(
            !s.contains("histórico"),
            "sin banda histórico en altura baja: {s:?}"
        );
    }

    #[test]
    fn color_paleta_cicla_y_gris_para_resto() {
        use super::{color_entidad, Color};
        // top 6 índices → colores distintos; ≥6 → gris (otros).
        // El primero es Green (NO cyan/magenta, reservados al dato en vivo).
        assert_eq!(color_entidad(0), Color::Green);
        assert_ne!(
            color_entidad(0),
            Color::Cyan,
            "histórico no usa el cyan del vivo"
        );
        assert_ne!(
            color_entidad(1),
            Color::Magenta,
            "histórico no usa el magenta del vivo"
        );
        assert_ne!(color_entidad(0), color_entidad(1));
        assert_eq!(color_entidad(6), Color::DarkGray); // 7º en adelante = otros
        assert_eq!(color_entidad(99), Color::DarkGray);
    }

    #[test]
    fn segmentos_proporcionales_suman_el_ancho() {
        use super::segmentos_franja;
        // valores 78 y 22 sobre ancho 100 → segmentos ~78 y ~22
        let segs = segmentos_franja(&[78, 22], 100);
        let suma: usize = segs.iter().map(|(w, _)| *w).sum();
        assert!(suma <= 100, "no desborda: {suma}");
        assert!(
            segs[0].0 >= 70 && segs[0].0 <= 80,
            "primer segmento ~78: {:?}",
            segs[0]
        );
    }

    #[test]
    fn segmentos_total_cero_no_panica() {
        use super::segmentos_franja;
        let segs = segmentos_franja(&[0, 0], 50);
        let suma: usize = segs.iter().map(|(w, _)| *w).sum();
        assert!(suma == 0 || suma <= 50);
    }

    #[test]
    fn segmentos_clamp_nunca_excede_el_ancho() {
        use super::segmentos_franja;
        // 8 entidades casi iguales: sin clamp el redondeo sumaría >ancho.
        let segs = segmentos_franja(&[1, 1, 1, 1, 1, 1, 1, 1], 12);
        let suma: usize = segs.iter().map(|(w, _)| *w).sum();
        assert!(
            suma <= 12,
            "el clamp impide desbordar el ancho: suma={suma}"
        );
    }

    // ──────────────────────────────────────────────────────────────
    // Tests de Emil-T2: spinner, hint acumulando, flash de teclas
    // ──────────────────────────────────────────────────────────────

    #[test]
    fn header_muestra_spinner_si_desconectado() {
        let m = LiveModel {
            connected: false,
            frame: 0,
            ..Default::default()
        };
        let s = render_area(&m, 80, 1, draw_header);
        assert!(s.contains('⠋'), "frame 0 → primer spinner: {s:?}");
        let m2 = LiveModel {
            connected: false,
            frame: 1,
            ..Default::default()
        };
        let s2 = render_area(&m2, 80, 1, draw_header);
        assert!(s2.contains('⠙'), "frame 1 → segundo spinner: {s2:?}");
    }

    #[test]
    fn chart_muestra_hint_acumulando_si_se_llena() {
        // serie corta (3 puntos) en ancho grande (60) → fit rellena → hint
        let serie = vec![1u64, 2, 3];
        let backend = ratatui::backend::TestBackend::new(60, 6);
        let mut terminal = ratatui::Terminal::new(backend).unwrap();
        terminal
            .draw(|f| {
                super::draw_chart_one(
                    f,
                    f.area(),
                    &serie,
                    3,
                    3,
                    0,
                    "↓",
                    Color::Cyan,
                    crate::render::UnitMode::Binary,
                    None,
                )
            })
            .unwrap();
        let buf = terminal.backend().buffer().clone();
        let s: String = buf.content().iter().map(|c| c.symbol()).collect();
        assert!(s.contains("acumulando"), "serie corta → hint: {s:?}");
    }

    #[test]
    fn chart_sin_hint_si_lleno() {
        // serie larga (80 puntos) en ancho 60 → no rellena → sin hint
        let serie: Vec<u64> = (0..80).map(|i| i as u64).collect();
        let backend = ratatui::backend::TestBackend::new(60, 6);
        let mut terminal = ratatui::Terminal::new(backend).unwrap();
        terminal
            .draw(|f| {
                super::draw_chart_one(
                    f,
                    f.area(),
                    &serie,
                    3,
                    3,
                    0,
                    "↓",
                    Color::Cyan,
                    crate::render::UnitMode::Binary,
                    None,
                )
            })
            .unwrap();
        let buf = terminal.backend().buffer().clone();
        let s: String = buf.content().iter().map(|c| c.symbol()).collect();
        assert!(!s.contains("acumulando"), "serie larga → sin hint: {s:?}");
    }

    #[test]
    fn chart_con_scrub_pinta_marcador_de_una_columna() {
        // serie de 5 puntos, ancho interior sobra de sobra (no rellena) → 1 columna por muestra
        let serie = vec![1u64, 2, 3, 4, 5];
        let backend = ratatui::backend::TestBackend::new(20, 6);
        let mut terminal = ratatui::Terminal::new(backend).unwrap();
        terminal
            .draw(|f| {
                let area = f.area();
                super::draw_chart_one(
                    f,
                    area,
                    &serie,
                    5,
                    5,
                    0,
                    "↓",
                    Color::Cyan,
                    crate::render::UnitMode::Binary,
                    Some(2), // cursor en la muestra del medio
                )
            })
            .unwrap();
        let buf = terminal.backend().buffer().clone();
        // inner_w = area.width - 2 = 18; serie de 5 puntos → fit_series rellena 13 ceros a la
        // izquierda; el índice visible 2 (de 0..5) cae en la columna 13 + 2 = 15 del inner,
        // es decir x = area.x + 1 (borde) + 15 = 16 dentro del buffer.
        // Con height=6 se reserva 1 fila para el eje X (Task 3): el plot del Sparkline
        // queda con height=5 (Constraint::Min(3) de 6 - Length(1) del eje X).
        let area = ratatui::layout::Rect::new(0, 0, 20, 6);
        let x_cursor = area.x + 1 + 15;
        let plot_height = 5; // area.height(6) - 1 (fila del eje X)
        let y_dato = area.y + plot_height - 2; // fila de datos del sparkline (dentro del borde)
        let celda = buf.cell((x_cursor, y_dato)).unwrap();
        assert!(
            celda.style().fg == Some(Color::White) || celda.modifier.contains(Modifier::REVERSED),
            "la columna del cursor debe resaltarse (blanco o reverse): {:?}",
            celda.style()
        );
    }

    #[test]
    fn chart_con_scrub_titulo_muestra_valor_bajo_el_cursor() {
        let serie = vec![10u64, 20, 30];
        let backend = ratatui::backend::TestBackend::new(60, 6);
        let mut terminal = ratatui::Terminal::new(backend).unwrap();
        terminal
            .draw(|f| {
                super::draw_chart_one(
                    f,
                    f.area(),
                    &serie,
                    30,
                    30,
                    0,
                    "↓",
                    Color::Cyan,
                    crate::render::UnitMode::Binary,
                    Some(1), // cursor en el valor 20 (visible, ya que serie < inner_w rellena ceros)
                )
            })
            .unwrap();
        let buf = terminal.backend().buffer().clone();
        let s: String = buf.content().iter().map(|c| c.symbol()).collect();
        assert!(
            s.contains("20 B"),
            "el título debe mostrar el valor exacto bajo el cursor: {s:?}"
        );
        assert!(
            s.contains("hace"),
            "el título debe mostrar la marca relativa: {s:?}"
        );
    }

    #[test]
    fn chart_sin_scrub_no_pinta_marcador_ni_cambia_titulo() {
        let serie = vec![1u64, 2, 3, 4, 5];
        let s = {
            let backend = ratatui::backend::TestBackend::new(20, 6);
            let mut terminal = ratatui::Terminal::new(backend).unwrap();
            terminal
                .draw(|f| {
                    super::draw_chart_one(
                        f,
                        f.area(),
                        &serie,
                        5,
                        5,
                        0,
                        "↓",
                        Color::Cyan,
                        crate::render::UnitMode::Binary,
                        None,
                    )
                })
                .unwrap();
            let buf = terminal.backend().buffer().clone();
            buf.content().iter().map(|c| c.symbol()).collect::<String>()
        };
        assert!(
            !s.contains("hace"),
            "sin scrub, sin marca relativa en el título: {s:?}"
        );
    }

    #[test]
    fn footer_resalta_tecla_en_flash() {
        // sin flash: el pie es uniforme; con flash en 'u', el segmento de unidades va en blanco/bold.
        // Verificamos vía estilo del buffer.
        use ratatui::style::Modifier;
        let m = LiveModel {
            flash_key: Some('u'),
            flash_frames: 2,
            ..Default::default()
        };
        let backend = ratatui::backend::TestBackend::new(100, 1);
        let mut terminal = ratatui::Terminal::new(backend).unwrap();
        terminal.draw(|f| draw_footer(f, &m, f.area())).unwrap();
        let buf = terminal.backend().buffer().clone();
        // busca alguna celda con BOLD (el flash) en la zona de "unidades"
        let hay_bold = buf
            .content()
            .iter()
            .any(|c| c.modifier.contains(Modifier::BOLD));
        assert!(hay_bold, "flash en 'u' debe poner algún segmento en bold");
    }

    #[test]
    fn etiqueta_modo_por_cada_modo() {
        use super::etiqueta_modo;
        use crate::live::state::{Focus, Mode};
        assert_eq!(etiqueta_modo(Mode::Normal, Focus::Grafico), "[NORMAL]");
        assert_eq!(etiqueta_modo(Mode::Zoom, Focus::Apps), "[ZOOM apps]");
        assert_eq!(etiqueta_modo(Mode::Filtro, Focus::Grafico), "[FILTRO]");
        assert_eq!(
            etiqueta_modo(Mode::PausaScrub, Focus::Grafico),
            "[PAUSA+SCRUB]"
        );
        assert_eq!(etiqueta_modo(Mode::Ayuda, Focus::Grafico), "[AYUDA]");
    }

    #[test]
    fn overlay_ayuda_renderiza_sin_panico_y_lista_teclas() {
        use crate::live::state::{keymap_activo, Mode};
        let m = LiveModel {
            mode: Mode::Ayuda,
            connected: true,
            ..Default::default()
        };
        let backend = ratatui::backend::TestBackend::new(100, 40);
        let mut terminal = ratatui::Terminal::new(backend).unwrap();
        terminal.draw(|f| draw(f, &m)).unwrap();
        let buf = terminal.backend().buffer().clone();
        let s: String = buf.content().iter().map(|c| c.symbol()).collect();
        for (tecla, _accion) in keymap_activo() {
            assert!(
                s.contains(tecla),
                "el overlay debe listar la tecla {tecla:?}: {s:?}"
            );
        }
    }

    #[test]
    fn hist_muestra_columna_porcentaje_y_franja() {
        use caudalghost_core::proto::TotalRow;
        let m = LiveModel {
            show_all: false,
            hist_ifaces: vec![
                TotalRow {
                    iface: "wlp2s0".into(),
                    rx: 780,
                    tx: 0,
                },
                TotalRow {
                    iface: "enp3s0".into(),
                    rx: 220,
                    tx: 0,
                },
            ],
            ..Default::default()
        };
        let s = render_area(&m, 60, 8, draw_hist_ifaces);
        assert!(s.contains('%'), "debe haber columna de %: {s:?}");
        assert!(s.contains("78.0%"), "wlp2s0 = 78%: {s:?}");
        assert!(s.contains('█'), "debe haber franja de proporción: {s:?}");
    }

    #[test]
    fn smoke_render_zoom_no_panica_en_los_cinco_focos_y_omite_lo_no_enfocado() {
        use crate::live::state::{Focus, Mode};
        use caudalghost_core::types::AppRemoteDelta;
        let focos = [
            Focus::Grafico,
            Focus::Apps,
            Focus::Interfaces,
            Focus::Servidores,
        ];
        for f in focos {
            let mut m = LiveModel {
                version: "0.6.0".into(),
                ebpf: true,
                connected: true,
                mode: Mode::Zoom,
                focus: f,
                ..Default::default()
            };
            m.samples = vec![crate::live::state::tests_support::sample(
                0,
                &[("eth0", 1024, 512)],
            )];
            m.samples[0].flows.push(AppRemoteDelta {
                app: "firefox".into(),
                host: "github.com".into(),
                rx: 1024,
                tx: 512,
            });
            m.hist_apps = vec![caudalghost_core::proto::AppRow {
                app: "megasync".into(),
                rx: 9_000_000,
                tx: 100,
            }];
            m.hist_ifaces = vec![caudalghost_core::proto::TotalRow {
                iface: "eth0".into(),
                rx: 9_000_000,
                tx: 100,
            }];
            let backend = ratatui::backend::TestBackend::new(100, 40);
            let mut terminal = ratatui::Terminal::new(backend).unwrap();
            terminal.draw(|frame| super::draw(frame, &m)).unwrap();
            let buf = terminal.backend().buffer().clone();
            let s: String = buf.content().iter().map(|c| c.symbol()).collect();
            assert!(
                s.contains("CaudalGhost"),
                "cabecera siempre visible: {f:?} {s:?}"
            );
            match f {
                Focus::Grafico => assert!(
                    s.contains("acumulando") || s.contains("KiB") || s.contains('↓'),
                    "{f:?} {s:?}"
                ),
                Focus::Apps => assert!(s.to_lowercase().contains("app"), "{f:?} {s:?}"),
                Focus::Interfaces => assert!(s.contains("eth0"), "{f:?} {s:?}"),
                Focus::Servidores => {
                    assert!(s.contains("github.com"), "{f:?} {s:?}")
                }
            }
        }
    }

    #[test]
    fn draw_en_pausa_scrub_con_indice_fijo_no_panica() {
        use crate::live::state::Mode;
        let m = LiveModel {
            version: "0.1.0".into(),
            ebpf: true,
            connected: true,
            mode: Mode::PausaScrub,
            paused: true,
            scrub_idx: Some(0),
            samples: vec![
                crate::live::state::tests_support::sample(0, &[("eth0", 1024, 512)]),
                crate::live::state::tests_support::sample(1, &[("eth0", 2048, 1024)]),
            ],
            ..Default::default()
        };
        let backend = ratatui::backend::TestBackend::new(100, 40);
        let mut terminal = ratatui::Terminal::new(backend).unwrap();
        terminal.draw(|f| draw(f, &m)).unwrap();
        let buf = terminal.backend().buffer().clone();
        let s: String = buf.content().iter().map(|c| c.symbol()).collect();
        assert!(s.contains("CaudalGhost"), "cabecera presente: {s:?}");
        assert!(
            s.contains("cursor"),
            "título del gráfico muestra el cursor: {s:?}"
        );
    }

    #[test]
    fn formatear_fecha_hora_ancho_holgado_incluye_fecha_con_año() {
        use chrono::TimeZone;
        let ahora = chrono::Local
            .with_ymd_and_hms(2026, 7, 2, 16, 38, 57)
            .unwrap();
        let s = super::formatear_fecha_hora(ahora, 80);
        // 2026-07-02 es jueves (verificado con `date -d 2026-07-02 +%A`); el plan
        // original decía "mié", que no corresponde a esa fecha real.
        assert_eq!(s, "jue 02 jul 2026 · 16:38:57");
    }

    #[test]
    fn formatear_fecha_hora_ancho_estrecho_solo_hora() {
        use chrono::TimeZone;
        let ahora = chrono::Local
            .with_ymd_and_hms(2026, 7, 2, 16, 38, 57)
            .unwrap();
        let s = super::formatear_fecha_hora(ahora, 40);
        assert_eq!(s, "16:38:57");
    }

    #[test]
    fn cabecera_muestra_fecha_con_año_en_ancho_holgado() {
        let m = LiveModel {
            version: "0.1.0".into(),
            ebpf: true,
            connected: true,
            ..Default::default()
        };
        let s = render_area(&m, 100, 1, draw_header);
        assert!(
            s.contains("2026"),
            "debe incluir el año en ancho holgado: {s:?}"
        );
    }

    #[test]
    fn keymap_incluye_drill_y_rango() {
        let k = crate::live::state::keymap_activo();
        assert!(
            k.iter().any(|(t, _)| t.contains('→') || t.contains('l')),
            "drill en el keymap: {k:?}"
        );
        assert!(
            k.iter().any(|(t, _)| t.contains('r')),
            "rango en el keymap: {k:?}"
        );
    }

    #[test]
    fn keymap_incluye_vista() {
        assert!(
            crate::live::state::keymap_activo()
                .iter()
                .any(|(t, _)| t.contains('v')),
            "'v' en el keymap"
        );
    }

    #[test]
    fn dashboard_por_defecto_muestra_hosts() {
        let mut m = LiveModel {
            version: "0.6.4".into(),
            ebpf: true,
            connected: true,
            ..Default::default()
        };
        m.samples = vec![crate::live::state::tests_support::sample_apps(
            0,
            &[("firefox", 2048, 256)],
        )];
        let backend = ratatui::backend::TestBackend::new(120, 40);
        let mut term = ratatui::Terminal::new(backend).unwrap();
        term.draw(|f| draw(f, &m)).unwrap();
        let buf = term.backend().buffer().clone();
        let s: String = buf.content().iter().map(|c| c.symbol()).collect();
        assert!(
            s.contains("hosts"),
            "la vista por defecto muestra la banda de hosts: {s:?}"
        );
        assert!(s.contains("CaudalGhost") && s.contains("salir"));
    }

    #[test]
    fn drill_no_rompe_draw_completo() {
        let mut m = LiveModel {
            version: "0.6.3".into(),
            ebpf: true,
            connected: true,
            ..Default::default()
        };
        m.samples = vec![
            crate::live::state::tests_support::sample(0, &[("eth0", 1024, 512)]),
            crate::live::state::tests_support::sample_apps(1, &[("firefox", 2048, 256)]),
        ];
        let backend = ratatui::backend::TestBackend::new(120, 40);
        let mut terminal = ratatui::Terminal::new(backend).unwrap();
        terminal.draw(|f| draw(f, &m)).unwrap();
        let buf = terminal.backend().buffer().clone();
        let s: String = buf.content().iter().map(|c| c.symbol()).collect();
        assert!(s.contains("CaudalGhost") && s.contains("ahora") && s.contains("salir"));
    }
}
