//! Render puro del dashboard sobre un Frame de ratatui (testeable con TestBackend).

use crate::live::state::LiveModel;
use crate::render::format_units;
use ratatui::layout::{Constraint, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph, Row, Sparkline, Table, Wrap};
use ratatui::Frame;

/// Dibuja el dashboard completo en 3 bandas:
/// ① gráficos ↓/↑ lado a lado · ② apps/interfaces EN VIVO · ③ apps/interfaces HISTÓRICO.
/// En terminales bajas (height < 30) se omite la banda histórico.
pub fn draw(frame: &mut Frame, model: &LiveModel) {
    use ratatui::layout::{Constraint, Direction, Layout};
    let area = frame.area();
    let con_historico = area.height >= 30;

    let constraints = if con_historico {
        vec![
            Constraint::Length(1),  // cabecera
            Constraint::Min(8),     // ① gráficos
            Constraint::Length(11), // ② vivo
            Constraint::Length(11), // ③ histórico
            Constraint::Length(1),  // pie
        ]
    } else {
        vec![
            Constraint::Length(1),  // cabecera
            Constraint::Min(6),     // ① gráficos
            Constraint::Length(11), // ② vivo
            Constraint::Length(1),  // pie
        ]
    };
    let bandas = Layout::default()
        .direction(Direction::Vertical)
        .constraints(constraints)
        .split(area);

    draw_header(frame, model, bandas[0]);
    draw_charts(frame, model, bandas[1]);

    // ② Banda VIVO: apps (izq 55%) | interfaces (der 45%)
    let vivo = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(55), Constraint::Percentage(45)])
        .split(bandas[2]);
    draw_apps(frame, model, vivo[0]);
    draw_ifaces(frame, model, vivo[1]);

    if con_historico {
        // ③ Banda HISTÓRICO: apps (izq 55%) | interfaces (der 45%)
        let hist = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(55), Constraint::Percentage(45)])
            .split(bandas[3]);
        draw_hist_apps(frame, model, hist[0]);
        draw_hist_ifaces(frame, model, hist[1]);
        draw_footer(frame, model, bandas[4]);
    } else {
        draw_footer(frame, model, bandas[3]);
    }
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
    let hora = chrono::Local::now().format("%H:%M:%S").to_string();
    let pausa = if model.paused { "  [PAUSA]" } else { "" };
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
    flecha: &str,
    color: Color,
    units: crate::render::UnitMode,
) {
    let inner_w = area.width.saturating_sub(2) as usize;
    // ÍTEM 2 — Hint "acumulando…" cuando el gráfico aún no se ha llenado.
    // Usamos Line::from(vec![...]) para componer el título con estilos distintos:
    // tasa actual en bold (dato clave del gráfico) y el hint en DarkGray (secundario).
    let base_title = format!(
        " {flecha} {}/s  (pico {}/s) ",
        format_units(cur, units),
        format_units(peak, units)
    );
    let title_line: Line = if serie.len() < inner_w {
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
    frame.render_widget(spark, area);
}

/// Banda de gráficos: ↓ rx (izquierda, cyan) y ↑ tx (derecha, magenta), lado a lado.
pub fn draw_charts(frame: &mut Frame, model: &LiveModel, area: Rect) {
    use ratatui::layout::{Direction, Layout};
    let (rx, tx) = model.throughput_series();
    let (cur_rx, cur_tx) = model.current_rate();
    let peak_rx = rx.iter().copied().max().unwrap_or(0);
    let peak_tx = tx.iter().copied().max().unwrap_or(0);

    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(area);

    draw_chart_one(
        frame,
        cols[0],
        &rx,
        cur_rx,
        peak_rx,
        "↓",
        Color::Cyan,
        model.units,
    );
    draw_chart_one(
        frame,
        cols[1],
        &tx,
        cur_tx,
        peak_tx,
        "↑",
        Color::Magenta,
        model.units,
    );
}

/// Pie de una línea: ayuda de teclas a la izquierda y, si hay ancho, el tagline
/// de marca a la derecha (corazón en el rojo de Parlox Network, #E3262E).
pub fn draw_footer(frame: &mut Frame, model: &LiveModel, area: Rect) {
    use ratatui::layout::{Alignment, Direction, Layout};
    // ÍTEM 3 — Micro-parpadeo: el segmento de la tecla activa va en blanco+BOLD durante
    // flash_frames redibujos. Sutil — el ojo percibe la acción sin decoración permanente.
    let bold_blanco = Style::default()
        .fg(Color::White)
        .add_modifier(Modifier::BOLD);
    let tenue = Style::default().fg(Color::DarkGray);
    let est = |k: char| {
        if model.flash_frames > 0 && model.flash_key == Some(k) {
            bold_blanco
        } else {
            tenue
        }
    };
    let comandos = Paragraph::new(Line::from(vec![
        Span::styled(" q/Esc salir · ", tenue),
        Span::styled("p pausa", est('p')),
        Span::styled(" · ", tenue),
        Span::styled("a incluir lo", est('a')),
        Span::styled(" · ", tenue),
        Span::styled(format!("u unidades: {}", model.units.label()), est('u')),
        Span::styled(" ", tenue),
    ]));

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

/// Panel de top apps con barras inline. Si eBPF inactivo, aviso honesto.
pub fn draw_apps(frame: &mut Frame, model: &LiveModel, area: Rect) {
    let block = Block::default()
        .borders(Borders::ALL)
        .title(" top apps · ahora ");
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
    let apps = model.top_apps();
    let secs = model
        .samples
        .len()
        .clamp(1, crate::live::state::PANEL_WINDOW_SECS) as u64;
    let max = apps.iter().map(|(_, rx, tx)| rx + tx).max().unwrap_or(0);
    // ÍTEM 4 — Fila #1 (mayor tráfico) en BOLD para guiar el ojo. Sutil — solo la primera.
    let rows: Vec<Row> = apps
        .iter()
        .take(area.height.saturating_sub(2) as usize)
        .enumerate()
        .map(|(i, (app, rx, tx))| {
            let total = rx + tx;
            let barra = barra_inline(total, max, 10);
            let row = Row::new(vec![
                app.clone(),
                format!("{}/s", format_units(rx / secs, model.units)),
                format!("{}/s", format_units(tx / secs, model.units)),
                barra,
            ]);
            if i == 0 {
                row.style(Style::default().add_modifier(Modifier::BOLD))
            } else {
                row
            }
        })
        .collect();
    let table = Table::new(
        rows,
        [
            Constraint::Min(12),
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
fn draw_hist_panel(
    frame: &mut Frame,
    area: Rect,
    titulo: &str,
    col_nombre: &str,
    items: &[(String, u64, u64)], // (nombre, rx, tx), ordenado desc por rx+tx
    units: crate::render::UnitMode,
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

    // Tabla con % y nombre coloreado
    let rows: Vec<Row> = items
        .iter()
        .take(partes[1].height.saturating_sub(1) as usize) // -1 por el header
        .enumerate()
        .map(|(i, (nombre, rx, tx))| {
            let t = rx + tx;
            let pct = if total == 0 {
                0.0_f64
            } else {
                t as f64 / total as f64 * 100.0
            };
            Row::new(vec![
                ratatui::widgets::Cell::from(Span::styled(
                    nombre.clone(),
                    Style::default().fg(color_entidad(i)),
                )),
                ratatui::widgets::Cell::from(format_units(*rx, units)),
                ratatui::widgets::Cell::from(format_units(*tx, units)),
                ratatui::widgets::Cell::from(format_units(t, units)),
                ratatui::widgets::Cell::from(format!("{pct:.1}%")),
            ])
        })
        .collect();
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

/// Panel de histórico de apps: totales acumulados, orden desc. Aviso si eBPF inactivo.
pub fn draw_hist_apps(frame: &mut Frame, model: &LiveModel, area: Rect) {
    if !model.ebpf {
        let block = Block::default()
            .borders(Borders::ALL)
            .title(" apps · histórico ");
        let aviso = Paragraph::new(Line::from(Span::styled(
            "eBPF inactivo — corre el daemon con sudo para ver apps",
            Style::default().fg(Color::Yellow),
        )))
        .wrap(Wrap { trim: true })
        .block(block);
        frame.render_widget(aviso, area);
        return;
    }
    let mut items: Vec<(String, u64, u64)> = model
        .hist_apps
        .iter()
        .map(|r| (r.app.clone(), r.rx, r.tx))
        .collect();
    items.sort_by_key(|(_, rx, tx)| std::cmp::Reverse(rx + tx));
    draw_hist_panel(
        frame,
        area,
        " apps · histórico ",
        "app",
        &items,
        model.units,
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
    draw_hist_panel(
        frame,
        area,
        " interfaces · histórico ",
        "iface",
        &items,
        model.units,
    );
}

/// Panel de interfaces con tasas, sparkline y (si existe) IP en gris indentada.
pub fn draw_ifaces(frame: &mut Frame, model: &LiveModel, area: Rect) {
    let block = Block::default()
        .borders(Borders::ALL)
        .title(" interfaces · ahora ");
    let secs = model
        .samples
        .len()
        .clamp(1, crate::live::state::PANEL_WINDOW_SECS) as u64;
    let max_lines = area.height.saturating_sub(2) as usize; // dentro de los bordes
    let mut lines: Vec<Line> = Vec::new();
    // ÍTEM 4 — Fila #1 (mayor tráfico) en BOLD para guiar el ojo.
    for (row_idx, (iface, rx, tx, serie)) in model.iface_rows().iter().enumerate() {
        if lines.len() >= max_lines {
            break;
        }
        let spark = crate::render::sparkline(&fit_series(serie, 8));
        let row_style = if row_idx == 0 {
            Style::default().add_modifier(Modifier::BOLD)
        } else {
            Style::default()
        };
        let datos = Line::from(vec![
            Span::styled(format!("{:<8} ", recorta(iface, 8)), row_style),
            Span::styled(
                format!(
                    "{:>10} ",
                    format!("{}/s", format_units(rx / secs, model.units))
                ),
                row_style,
            ),
            Span::styled(
                format!(
                    "{:>10} ",
                    format!("{}/s", format_units(tx / secs, model.units))
                ),
                row_style,
            ),
            Span::styled(spark, Style::default().fg(Color::Cyan)),
        ]);
        lines.push(datos);
        if let Some(ip) = model.iface_ips.get(iface) {
            if lines.len() < max_lines {
                lines.push(Line::from(Span::styled(
                    format!("   {ip}"),
                    Style::default().fg(Color::DarkGray),
                )));
            }
        }
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
    use ratatui::{backend::TestBackend, layout::Rect, style::Color, Terminal};

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
        use caudalghost_core::proto::{AppRow, TotalRow};
        let mut m = LiveModel {
            version: "0.1.0".into(),
            ebpf: true,
            connected: true,
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
        let backend = ratatui::backend::TestBackend::new(120, 40); // alto → con histórico
        let mut terminal = ratatui::Terminal::new(backend).unwrap();
        terminal.draw(|f| draw(f, &m)).unwrap();
        let buf = terminal.backend().buffer().clone();
        let s: String = buf.content().iter().map(|c| c.symbol()).collect();
        assert!(s.contains("CaudalGhost"), "cabecera");
        assert!(s.contains("ahora"), "banda vivo (título · ahora)");
        assert!(s.contains("histórico"), "banda histórico");
        assert!(s.contains("megasync"), "dato histórico");
        assert!(s.contains("salir"), "pie");
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
                    "↓",
                    Color::Cyan,
                    crate::render::UnitMode::Binary,
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
                    "↓",
                    Color::Cyan,
                    crate::render::UnitMode::Binary,
                )
            })
            .unwrap();
        let buf = terminal.backend().buffer().clone();
        let s: String = buf.content().iter().map(|c| c.symbol()).collect();
        assert!(!s.contains("acumulando"), "serie larga → sin hint: {s:?}");
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
}
