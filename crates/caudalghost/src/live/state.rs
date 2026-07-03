//! Modelo puro del dashboard en vivo y sus derivaciones (sin I/O, testeable).

/// Ventana de agregación de los paneles apps/interfaces — tasas responsivas
/// independientes del ancho del gráfico.
pub const PANEL_WINDOW_SECS: usize = 60;

/// Modo de interacción del dashboard: determina qué teclas hacen qué.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Mode {
    #[default]
    Normal,
    Zoom,
    Filtro,
    PausaScrub,
    Ayuda,
    Drilldown,
}

/// Panel con el foco de navegación (Tab/Shift+Tab ciclan entre estos).
/// Cada panel (Apps/Interfaces/Servidores) tiene su propia `Vista` (ahora/histórico,
/// ver más abajo); `Focus` solo identifica el panel, no su vista activa (0.6.4).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Focus {
    #[default]
    Grafico,
    Apps,
    Interfaces,
    Servidores,
}

/// Vista activa de un panel (apps/interfaces/servidores): en vivo o histórico.
/// Alternada con la tecla `v` sobre el panel enfocado (0.6.4).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Vista {
    #[default]
    Ahora,
    Historico,
}

/// Estado del drill-down activo: app elegida y la vista del panel de apps al
/// entrar (determina si el histórico pide `TopRemotes` o el en-vivo agrega `flows`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Drill {
    pub app: String,
    pub vista: Vista,
}

/// Tecla lógica, independiente del backend de terminal (crossterm).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Key {
    Char(char),
    Enter,
    Esc,
    Tab,
    ShiftTab,
    Up,
    Down,
    Left,
    Right,
    Backspace,
}

/// Estado del dashboard: lo último recibido del daemon + flags de UI.
#[derive(Debug, Clone, Default)]
pub struct LiveModel {
    pub version: String,
    pub ebpf: bool,
    pub connected: bool,
    pub paused: bool,
    pub show_all: bool,
    pub units: crate::render::UnitMode,
    pub samples: Vec<caudalghost_core::types::Sample>,
    /// IPv4 por interfaz (nombre → IP), leída del sistema local. Vacío al inicio.
    pub iface_ips: std::collections::HashMap<String, String>,
    /// Histórico acumulado de apps (todo el registro). Vacío al inicio.
    pub hist_apps: Vec<caudalghost_core::proto::AppRow>,
    /// Histórico acumulado de interfaces (todo el registro). Vacío al inicio.
    pub hist_ifaces: Vec<caudalghost_core::proto::TotalRow>,
    /// Contador de redibujos, para animar el spinner de reconexión. Lo incrementa run().
    pub frame: u64,
    /// Tecla recién pulsada (u/p/a) para el micro-parpadeo de feedback en el pie.
    pub flash_key: Option<char>,
    /// Redibujos restantes del parpadeo de flash_key (0 = sin flash).
    pub flash_frames: u8,
    /// Modo de interacción activo (Normal/Zoom/Filtro/PausaScrub/Ayuda).
    pub mode: Mode,
    /// Panel con el foco de navegación.
    pub focus: Focus,
    /// Fila seleccionada dentro del panel enfocado.
    pub selected_row: usize,
    /// Texto del filtro incremental del panel enfocado.
    pub filtro: String,
    /// Índice del cursor de scrub sobre la rebanada visible, en PausaScrub.
    pub scrub_idx: Option<usize>,
    /// Intentos de reconexión consecutivos (para el backoff).
    pub reconnect_attempts: u32,
    /// Drill-down activo (app + panel de origen), o `None` fuera de `Mode::Drilldown`.
    pub drill: Option<Drill>,
    /// Índice del rango histórico activo en `RANGOS` (cicla con 'r').
    pub hist_range_idx: usize,
    /// Hosts remotos de la app en drill histórico (poblado por `TopRemotes`).
    pub drill_hosts: Vec<caudalghost_core::proto::RemoteRow>,
    /// Vista activa (Ahora/Histórico) del panel de apps.
    pub vista_apps: Vista,
    /// Vista activa (Ahora/Histórico) del panel de interfaces.
    pub vista_ifaces: Vista,
    /// Vista activa (Ahora/Histórico) del panel de servidores.
    pub vista_servidores: Vista,
    /// Top de hosts remotos (todas las apps) en histórico, poblado por `TopRemotes{app:None}`.
    pub servidores_hist: Vec<caudalghost_core::proto::RemoteRow>,
}

/// Tokens de rango histórico ciclables con 'r': "todo" es el registro completo.
pub const RANGOS: [&str; 5] = ["todo", "hoy", "ayer", "7d", "30d"];

impl LiveModel {
    fn incluir(&self, iface: &str) -> bool {
        self.show_all || iface != "lo"
    }

    /// Throughput total por segundo sobre la ventana: (serie rx, serie tx).
    pub fn throughput_series(&self) -> (Vec<u64>, Vec<u64>) {
        let mut rx = Vec::with_capacity(self.samples.len());
        let mut tx = Vec::with_capacity(self.samples.len());
        for s in &self.samples {
            let mut r = 0u64;
            let mut t = 0u64;
            for d in &s.deltas {
                if self.incluir(&d.iface) {
                    r += d.rx;
                    t += d.tx;
                }
            }
            rx.push(r);
            tx.push(t);
        }
        (rx, tx)
    }

    /// Tasa del último segundo: (rx, tx).
    pub fn current_rate(&self) -> (u64, u64) {
        match self.samples.last() {
            None => (0, 0),
            Some(s) => {
                let mut r = 0;
                let mut t = 0;
                for d in &s.deltas {
                    if self.incluir(&d.iface) {
                        r += d.rx;
                        t += d.tx;
                    }
                }
                (r, t)
            }
        }
    }

    /// Apps agregadas sobre los últimos PANEL_WINDOW_SECS samples, ordenadas por total desc: (app, rx, tx).
    pub fn top_apps(&self) -> Vec<(String, u64, u64)> {
        use std::collections::HashMap;
        let start = self.samples.len().saturating_sub(PANEL_WINDOW_SECS);
        let recientes = &self.samples[start..];
        let mut acc: HashMap<String, (u64, u64)> = HashMap::new();
        for s in recientes {
            for a in &s.apps {
                let e = acc.entry(a.app.clone()).or_default();
                e.0 += a.rx;
                e.1 += a.tx;
            }
        }
        let mut v: Vec<(String, u64, u64)> =
            acc.into_iter().map(|(k, (rx, tx))| (k, rx, tx)).collect();
        v.sort_by_key(|(_, rx, tx)| std::cmp::Reverse(rx + tx));
        v
    }

    /// Nº de filas del panel actualmente enfocado (0 si el foco no es una tabla).
    fn filas_panel_enfocado(&self) -> usize {
        match self.focus {
            Focus::Grafico => 0,
            Focus::Apps => match self.vista_apps {
                Vista::Ahora => self.top_apps().len(),
                Vista::Historico => self.hist_apps.len(),
            },
            Focus::Interfaces => match self.vista_ifaces {
                Vista::Ahora => self.iface_rows().len(),
                Vista::Historico => self.hist_ifaces.len(),
            },
            Focus::Servidores => match self.vista_servidores {
                Vista::Ahora => self.top_hosts_vivo().len(),
                Vista::Historico => self.servidores_hist.len(),
            },
        }
    }

    /// Interfaces agregadas sobre los últimos PANEL_WINDOW_SECS samples:
    /// (iface, rx, tx, serie rx+tx por segundo alineada). Ordenadas por total desc.
    pub fn iface_rows(&self) -> Vec<(String, u64, u64, Vec<u64>)> {
        use std::collections::BTreeMap;
        let start = self.samples.len().saturating_sub(PANEL_WINDOW_SECS);
        let recientes = &self.samples[start..];
        let mut tot: BTreeMap<String, (u64, u64, Vec<u64>)> = BTreeMap::new();
        for (i, s) in recientes.iter().enumerate() {
            for d in &s.deltas {
                if self.incluir(&d.iface) {
                    let e = tot.entry(d.iface.clone()).or_default();
                    e.0 += d.rx;
                    e.1 += d.tx;
                }
            }
            for (iface, e) in tot.iter_mut() {
                e.2.resize(i, 0);
                let v = s
                    .deltas
                    .iter()
                    .find(|d| &d.iface == iface)
                    .map(|d| d.rx + d.tx)
                    .unwrap_or(0);
                e.2.push(v);
            }
        }
        let mut v: Vec<(String, u64, u64, Vec<u64>)> = tot
            .into_iter()
            .map(|(i, (rx, tx, serie))| (i, rx, tx, serie))
            .collect();
        v.sort_by_key(|(_, rx, tx, _)| std::cmp::Reverse(rx + tx));
        v
    }

    /// Hosts remotos de `app` agregados por host sobre los últimos PANEL_WINDOW_SECS
    /// samples (en vivo), ordenados por total desc: (host, rx, tx).
    pub fn hosts_de_app(&self, app: &str) -> Vec<(String, u64, u64)> {
        use std::collections::HashMap;
        let start = self.samples.len().saturating_sub(PANEL_WINDOW_SECS);
        let mut acc: HashMap<String, (u64, u64)> = HashMap::new();
        for s in &self.samples[start..] {
            for f in &s.flows {
                if f.app == app {
                    let e = acc.entry(f.host.clone()).or_default();
                    e.0 += f.rx;
                    e.1 += f.tx;
                }
            }
        }
        let mut v: Vec<(String, u64, u64)> =
            acc.into_iter().map(|(k, (rx, tx))| (k, rx, tx)).collect();
        v.sort_by_key(|(_, rx, tx)| std::cmp::Reverse(rx + tx));
        v
    }

    /// Hosts remotos agregados por host sobre los últimos PANEL_WINDOW_SECS samples (en
    /// vivo), sumando TODAS las apps (a diferencia de `hosts_de_app`, que filtra por una
    /// app). Alimenta el panel de servidores (Task 3, 0.6.4). Ordenado por total desc.
    pub fn top_hosts_vivo(&self) -> Vec<(String, u64, u64)> {
        use std::collections::HashMap;
        let start = self.samples.len().saturating_sub(PANEL_WINDOW_SECS);
        let mut acc: HashMap<String, (u64, u64)> = HashMap::new();
        for s in &self.samples[start..] {
            for f in &s.flows {
                let e = acc.entry(f.host.clone()).or_default();
                e.0 += f.rx;
                e.1 += f.tx;
            }
        }
        let mut v: Vec<(String, u64, u64)> =
            acc.into_iter().map(|(k, (rx, tx))| (k, rx, tx)).collect();
        v.sort_by_key(|(_, rx, tx)| std::cmp::Reverse(rx + tx));
        v
    }

    /// App en `selected_row` del panel de apps enfocado (mismo orden/filtro que dibuja
    /// ui.rs: top_apps()/hist_apps ordenado desc, filtrado por `nombre_coincide`).
    /// `None` si el foco no es el panel de apps o no hay selección válida.
    pub fn app_seleccionada(&self) -> Option<String> {
        if self.focus != Focus::Apps {
            return None;
        }
        let nombres: Vec<String> = match self.vista_apps {
            Vista::Ahora => self
                .top_apps()
                .into_iter()
                .map(|(a, _, _)| a)
                .filter(|a| nombre_coincide(a, &self.filtro))
                .collect(),
            Vista::Historico => {
                let mut v: Vec<(String, u64, u64)> = self
                    .hist_apps
                    .iter()
                    .map(|r| (r.app.clone(), r.rx, r.tx))
                    .collect();
                v.sort_by_key(|(_, rx, tx)| std::cmp::Reverse(rx + tx));
                v.into_iter()
                    .map(|(a, _, _)| a)
                    .filter(|a| nombre_coincide(a, &self.filtro))
                    .collect()
            }
        };
        nombres.get(self.selected_row).cloned()
    }
}

/// Rango histórico → ventana (start, end) en epoch UTC. "todo" es todo el registro
/// (sin filtro de tiempo); el resto delega en `caudalghost_core::range::resolve`.
pub fn rango_a_ventana(token: &str, ahora: chrono::DateTime<chrono::Local>) -> (i64, i64) {
    if token == "todo" {
        (0, i64::MAX)
    } else {
        match caudalghost_core::range::resolve(token, ahora) {
            Ok(w) => (w.start, w.end),
            Err(_) => (0, i64::MAX),
        }
    }
}

/// Resultado de procesar una tecla.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    None,
    Redraw,
    Quit,
}

/// Cicla el foco de panel en el orden fijo Grafico→Apps→Interfaces→Servidores→
/// (vuelve a Grafico). `forward=false` recorre el orden inverso.
fn cycle_focus(f: Focus, forward: bool) -> Focus {
    const ORDEN: [Focus; 4] = [
        Focus::Grafico,
        Focus::Apps,
        Focus::Interfaces,
        Focus::Servidores,
    ];
    let i = ORDEN.iter().position(|x| *x == f).unwrap_or(0);
    let n = ORDEN.len();
    let j = if forward {
        (i + 1) % n
    } else {
        (i + n - 1) % n
    };
    ORDEN[j]
}

/// Substring case-insensitive; patrón vacío siempre coincide.
pub fn nombre_coincide(nombre: &str, patron: &str) -> bool {
    if patron.is_empty() {
        return true;
    }
    nombre.to_lowercase().contains(&patron.to_lowercase())
}

/// Mueve un índice de selección `delta` posiciones, con clamp a `[0, len)`.
/// `len == 0` siempre devuelve 0 (sin filas que seleccionar).
fn mover_indice(idx: usize, len: usize, delta: i32) -> usize {
    if len == 0 {
        return 0;
    }
    let max = len - 1;
    let nuevo = idx as i32 + delta;
    nuevo.clamp(0, max as i32) as usize
}

/// Fuente única de verdad del keymap activo en 0.6.0: (tecla, acción).
/// La reutilizan tanto el overlay de ayuda como el pie contextual. Solo
/// lista teclas ya implementadas (nada de "próximamente").
pub fn keymap_activo() -> Vec<(&'static str, &'static str)> {
    vec![
        ("q", "Salir (solo en modo Normal)"),
        ("Esc", "Retrocede un nivel según el modo"),
        ("p", "Pausa + activa scrub"),
        ("a", "Incluir/excluir loopback"),
        ("u", "Ciclar unidades (KiB/KB/Mbps)"),
        ("Tab / Shift+Tab", "Ciclar foco de panel"),
        ("j/k · ↑/↓", "Mover selección de fila"),
        ("←/→", "(en pausa) mover cursor de scrub"),
        ("Enter", "Zoom del panel enfocado"),
        ("/", "Filtro incremental (panel enfocado)"),
        ("→/l", "Hosts de la app (drill-down)"),
        ("←", "Volver (sale del drill-down)"),
        ("r", "Ciclar rango histórico (todo/hoy/ayer/7d/30d)"),
        ("v", "Alternar vista ahora/histórico del panel enfocado"),
        ("?", "Overlay de ayuda"),
    ]
}

/// Reducer puro de teclado: muta el modelo y dice qué hacer.
/// 'q' sale solo en Normal · Esc tiene tabla por modo (Normal sale; Ayuda/Zoom/Filtro/
/// PausaScrub vuelven a Normal, cada uno con su limpieza) · 'p' pausa · 'a' incluir lo ·
/// 'u' unidades · '?' abre/cierra el overlay de ayuda · Tab/Shift+Tab ciclan el foco de
/// panel (y resetean selected_row y filtro) · j/k y ↑/↓ mueven la fila seleccionada del
/// panel enfocado (sin efecto si el foco es el gráfico) · '/' entra en modo Filtro desde
/// Normal; en Filtro, los caracteres y Backspace editan `model.filtro`, Enter confirma
/// (vuelve a Normal conservando el texto) y Esc cancela (vuelve a Normal y lo limpia).
pub fn apply_key(model: &mut LiveModel, key: Key) -> Action {
    if model.mode == Mode::Filtro {
        return match key {
            Key::Char(c) => {
                model.filtro.push(c);
                Action::Redraw
            }
            Key::Backspace => {
                model.filtro.pop();
                Action::Redraw
            }
            Key::Enter => {
                model.mode = Mode::Normal;
                Action::Redraw
            }
            Key::Esc => {
                model.mode = Mode::Normal;
                model.filtro.clear();
                Action::Redraw
            }
            _ => Action::None,
        };
    }
    if model.mode == Mode::Drilldown {
        return match key {
            Key::Left | Key::Esc => {
                model.mode = Mode::Normal;
                model.drill = None;
                Action::Redraw
            }
            _ => Action::None,
        };
    }
    match key {
        Key::Char('/') if model.mode == Mode::Normal => {
            model.mode = Mode::Filtro;
            model.filtro.clear();
            Action::Redraw
        }
        Key::Tab => {
            model.focus = cycle_focus(model.focus, true);
            model.selected_row = 0;
            model.filtro.clear();
            Action::Redraw
        }
        Key::ShiftTab => {
            model.focus = cycle_focus(model.focus, false);
            model.selected_row = 0;
            model.filtro.clear();
            Action::Redraw
        }
        Key::Char('j') | Key::Down => {
            if model.focus == Focus::Grafico {
                Action::None
            } else {
                let len = model.filas_panel_enfocado();
                model.selected_row = mover_indice(model.selected_row, len, 1);
                Action::Redraw
            }
        }
        Key::Char('k') | Key::Up => {
            if model.focus == Focus::Grafico {
                Action::None
            } else {
                let len = model.filas_panel_enfocado();
                model.selected_row = mover_indice(model.selected_row, len, -1);
                Action::Redraw
            }
        }
        Key::Enter => match model.mode {
            Mode::Normal | Mode::Zoom => {
                model.mode = Mode::Zoom;
                Action::Redraw
            }
            _ => Action::None,
        },
        Key::Esc => match model.mode {
            Mode::Normal => Action::Quit,
            Mode::Ayuda | Mode::Zoom => {
                model.mode = Mode::Normal;
                Action::Redraw
            }
            Mode::Filtro => {
                model.mode = Mode::Normal;
                model.filtro.clear();
                Action::Redraw
            }
            Mode::PausaScrub => {
                model.mode = Mode::Normal;
                model.paused = false;
                model.scrub_idx = None;
                Action::Redraw
            }
            // Mode::Drilldown ya se intercepta en el early-return de arriba
            // (Esc/Left vuelven a Normal y limpian `drill`); no llega hasta aquí.
            Mode::Drilldown => Action::None,
        },
        Key::Char('q') if model.mode == Mode::Normal => Action::Quit,
        Key::Left if model.mode == Mode::PausaScrub => {
            let len = model.samples.len();
            let idx = model.scrub_idx.unwrap_or(0);
            model.scrub_idx = Some(mover_indice(idx, len, -1));
            Action::Redraw
        }
        Key::Right if model.mode == Mode::PausaScrub => {
            let len = model.samples.len();
            let idx = model.scrub_idx.unwrap_or(0);
            model.scrub_idx = Some(mover_indice(idx, len, 1));
            Action::Redraw
        }
        Key::Right | Key::Char('l') if model.mode == Mode::Normal && model.focus == Focus::Apps => {
            if let Some(app) = model.app_seleccionada() {
                model.drill = Some(Drill {
                    app,
                    vista: model.vista_apps,
                });
                model.mode = Mode::Drilldown;
                Action::Redraw
            } else {
                Action::None
            }
        }
        Key::Char('v') if model.mode == Mode::Normal => {
            match model.focus {
                Focus::Grafico => {}
                Focus::Apps => {
                    model.vista_apps = match model.vista_apps {
                        Vista::Ahora => Vista::Historico,
                        Vista::Historico => Vista::Ahora,
                    };
                }
                Focus::Interfaces => {
                    model.vista_ifaces = match model.vista_ifaces {
                        Vista::Ahora => Vista::Historico,
                        Vista::Historico => Vista::Ahora,
                    };
                }
                Focus::Servidores => {
                    model.vista_servidores = match model.vista_servidores {
                        Vista::Ahora => Vista::Historico,
                        Vista::Historico => Vista::Ahora,
                    };
                }
            }
            // Al alternar la vista, la lista cambia de largo: reinicia la selección
            // para no dejarla apuntando a una fila fuera de rango (igual que Tab).
            if model.focus != Focus::Grafico {
                model.selected_row = 0;
            }
            Action::Redraw
        }
        Key::Char('r') => {
            model.hist_range_idx = (model.hist_range_idx + 1) % RANGOS.len();
            Action::Redraw
        }
        Key::Char('p') => {
            model.paused = !model.paused;
            model.mode = if model.paused {
                Mode::PausaScrub
            } else {
                Mode::Normal
            };
            model.scrub_idx = if model.paused {
                Some(model.samples.len().saturating_sub(1))
            } else {
                None
            };
            model.flash_key = Some('p');
            model.flash_frames = 2;
            Action::Redraw
        }
        Key::Char('a') => {
            model.show_all = !model.show_all;
            model.flash_key = Some('a');
            model.flash_frames = 2;
            Action::Redraw
        }
        Key::Char('u') => {
            model.units = model.units.next();
            model.flash_key = Some('u');
            model.flash_frames = 2;
            Action::Redraw
        }
        Key::Char('?') => {
            model.mode = if model.mode == Mode::Ayuda {
                Mode::Normal
            } else {
                Mode::Ayuda
            };
            Action::Redraw
        }
        _ => Action::None,
    }
}

/// Nivel de estado visual: frescura del dato y resaltado por umbral.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Nivel {
    Verde,
    Ambar,
    Rojo,
}

/// Frescura del último dato recibido, dado `ahora` inyectado (testeable sin reloj real).
/// <2s Verde · 2..=5s Ambar · >5s Rojo. Texto siempre "hace N s".
pub fn frescura(ultimo_ts: i64, ahora: i64) -> (Nivel, String) {
    let delta = (ahora - ultimo_ts).max(0);
    let nivel = if delta < 2 {
        Nivel::Verde
    } else if delta <= 5 {
        Nivel::Ambar
    } else {
        Nivel::Rojo
    };
    (nivel, format!("hace {delta} s"))
}

/// Backoff de reconexión: 1,2,4,5,5,5… s (tope 5 s). `intentos` = reintentos fallidos ya hechos.
pub fn siguiente_backoff(intentos: u32) -> std::time::Duration {
    let secs = 2u64.saturating_pow(intentos).min(5);
    std::time::Duration::from_secs(secs)
}

/// Nivel de una tasa `actual` respecto al percentil 90 de su propia `serie` reciente.
/// actual > p90 => Rojo; actual > mediana => Ambar; resto => Verde. Serie vacía => Verde.
pub fn nivel_umbral(serie: &[u64], actual: u64) -> Nivel {
    if serie.is_empty() {
        return Nivel::Verde;
    }
    let mut ordenada = serie.to_vec();
    ordenada.sort_unstable();
    let percentil = |p: f64| -> u64 {
        let idx = ((ordenada.len() as f64 - 1.0) * p).floor() as usize;
        ordenada[idx.min(ordenada.len() - 1)]
    };
    let p90 = percentil(0.9);
    let mediana = percentil(0.5);
    if actual > p90 {
        Nivel::Rojo
    } else if actual > mediana {
        Nivel::Ambar
    } else {
        Nivel::Verde
    }
}

/// Ventana de suavizado (segundos) del resaltado por umbral. La media de estos segundos se
/// compara contra los percentiles de la serie IGUALMENTE suavizada (sostenido vs sostenido),
/// así el color no parpadea de tick a tick y los tres tonos siguen apareciendo con sentido.
pub const SUAVIZADO_UMBRAL_SECS: usize = 10;

/// Media de los últimos `n` valores de la serie (0 si está vacía o `n == 0`).
pub fn media_reciente(serie: &[u64], n: usize) -> u64 {
    if serie.is_empty() || n == 0 {
        return 0;
    }
    let start = serie.len().saturating_sub(n);
    let slice = &serie[start..];
    let suma: u64 = slice.iter().copied().sum();
    suma / slice.len() as u64
}

/// Serie suavizada: cada punto es la media móvil trasera de los `n` valores que terminan en él.
/// Comparar el "actual" (su último punto) contra los percentiles de ESTA serie da un umbral
/// estable (sostenido vs sostenido), en vez de un instante contra picos sueltos.
pub fn serie_suavizada(serie: &[u64], n: usize) -> Vec<u64> {
    (0..serie.len())
        .map(|i| media_reciente(&serie[..=i], n))
        .collect()
}

#[cfg(test)]
pub mod tests_support {
    use caudalghost_core::types::{IfaceDelta, Sample};
    pub fn sample(ts: i64, ifaces: &[(&str, u64, u64)]) -> Sample {
        Sample {
            ts,
            deltas: ifaces
                .iter()
                .map(|(i, rx, tx)| IfaceDelta {
                    iface: (*i).into(),
                    rx: *rx,
                    tx: *tx,
                })
                .collect(),
            apps: vec![],
            flows: vec![],
        }
    }

    pub fn sample_apps(ts: i64, apps: &[(&str, u64, u64)]) -> Sample {
        use caudalghost_core::types::AppDelta;
        Sample {
            ts,
            deltas: vec![],
            apps: apps
                .iter()
                .map(|(a, rx, tx)| AppDelta {
                    app: (*a).into(),
                    rx: *rx,
                    tx: *tx,
                })
                .collect(),
            flows: vec![],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use caudalghost_core::types::{AppDelta, IfaceDelta, Sample};

    fn sample(ts: i64, ifaces: &[(&str, u64, u64)], apps: &[(&str, u64, u64)]) -> Sample {
        Sample {
            ts,
            deltas: ifaces
                .iter()
                .map(|(i, rx, tx)| IfaceDelta {
                    iface: (*i).into(),
                    rx: *rx,
                    tx: *tx,
                })
                .collect(),
            apps: apps
                .iter()
                .map(|(a, rx, tx)| AppDelta {
                    app: (*a).into(),
                    rx: *rx,
                    tx: *tx,
                })
                .collect(),
            flows: vec![],
        }
    }

    #[test]
    fn throughput_series_suma_interfaces_por_segundo() {
        let m = LiveModel {
            show_all: false,
            samples: vec![
                sample(0, &[("eth0", 10, 1), ("lo", 99, 99)], &[]),
                sample(1, &[("eth0", 20, 2), ("wlan0", 5, 0)], &[]),
            ],
            ..Default::default()
        };
        let (rx, tx) = m.throughput_series();
        assert_eq!(rx, vec![10, 25]);
        assert_eq!(tx, vec![1, 2]);
    }

    #[test]
    fn throughput_incluye_lo_con_show_all() {
        let m = LiveModel {
            show_all: true,
            samples: vec![sample(0, &[("eth0", 10, 1), ("lo", 5, 5)], &[])],
            ..Default::default()
        };
        let (rx, tx) = m.throughput_series();
        assert_eq!(rx, vec![15]);
        assert_eq!(tx, vec![6]);
    }

    #[test]
    fn top_apps_ordena_por_total_desc() {
        let m = LiveModel {
            samples: vec![
                sample(0, &[], &[("firefox", 10, 1)]),
                sample(1, &[], &[("firefox", 20, 2), ("curl", 5, 5)]),
            ],
            ..Default::default()
        };
        let apps = m.top_apps();
        assert_eq!(apps.len(), 2);
        assert_eq!(
            (apps[0].0.as_str(), apps[0].1, apps[0].2),
            ("firefox", 30, 3)
        );
        assert_eq!((apps[1].0.as_str(), apps[1].1, apps[1].2), ("curl", 5, 5));
    }

    #[test]
    fn iface_rows_agrega_y_excluye_lo() {
        let m = LiveModel {
            show_all: false,
            samples: vec![
                sample(0, &[("eth0", 10, 1), ("lo", 5, 5)], &[]),
                sample(1, &[("eth0", 20, 2)], &[]),
            ],
            ..Default::default()
        };
        let rows = m.iface_rows();
        assert_eq!(rows.len(), 1);
        assert_eq!((rows[0].0.as_str(), rows[0].1, rows[0].2), ("eth0", 30, 3));
        assert_eq!(rows[0].3, vec![11, 22]);
    }

    #[test]
    fn current_rate_es_el_ultimo_segundo() {
        let m = LiveModel {
            show_all: false,
            samples: vec![
                sample(0, &[("eth0", 10, 1)], &[]),
                sample(1, &[("eth0", 99, 88)], &[]),
            ],
            ..Default::default()
        };
        assert_eq!(m.current_rate(), (99, 88));
    }

    #[test]
    fn series_vacias_sin_samples() {
        let m = LiveModel::default();
        assert_eq!(m.throughput_series(), (vec![], vec![]));
        assert!(m.top_apps().is_empty());
        assert!(m.iface_rows().is_empty());
        assert_eq!(m.current_rate(), (0, 0));
    }

    #[test]
    fn apply_key_salir() {
        let mut m = LiveModel::default();
        assert_eq!(apply_key(&mut m, Key::Char('q')), Action::Quit);
        assert_eq!(apply_key(&mut m, Key::Esc), Action::Quit);
    }

    #[test]
    fn apply_key_pausa_y_show_all_togglean() {
        let mut m = LiveModel::default();
        assert!(!m.paused);
        assert_eq!(apply_key(&mut m, Key::Char('p')), Action::Redraw);
        assert!(m.paused);
        apply_key(&mut m, Key::Char('p'));
        assert!(!m.paused);
        assert!(!m.show_all);
        apply_key(&mut m, Key::Char('a'));
        assert!(m.show_all);
    }

    #[test]
    fn apply_key_desconocida_es_noop() {
        let mut m = LiveModel::default();
        assert_eq!(apply_key(&mut m, Key::Char('z')), Action::None);
    }

    #[test]
    fn esc_en_ayuda_zoom_filtro_vuelve_a_normal() {
        for modo in [Mode::Ayuda, Mode::Zoom, Mode::Filtro] {
            let mut m = LiveModel {
                mode: modo,
                ..Default::default()
            };
            assert_eq!(apply_key(&mut m, Key::Esc), Action::Redraw);
            assert_eq!(m.mode, Mode::Normal, "modo de origen: {modo:?}");
        }
    }

    #[test]
    fn esc_en_filtro_limpia_el_texto_del_filtro() {
        let mut m = LiveModel {
            mode: Mode::Filtro,
            filtro: "fire".into(),
            ..Default::default()
        };
        apply_key(&mut m, Key::Esc);
        assert_eq!(m.mode, Mode::Normal);
        assert_eq!(m.filtro, "");
    }

    #[test]
    fn esc_en_pausa_scrub_reanuda_y_limpia_scrub_idx() {
        let mut m = LiveModel {
            mode: Mode::PausaScrub,
            paused: true,
            scrub_idx: Some(3),
            ..Default::default()
        };
        assert_eq!(apply_key(&mut m, Key::Esc), Action::Redraw);
        assert_eq!(m.mode, Mode::Normal);
        assert!(!m.paused);
        assert_eq!(m.scrub_idx, None);
    }

    #[test]
    fn apply_key_p_entra_en_pausa_scrub_con_borde_vivo() {
        let mut m = LiveModel {
            samples: vec![
                tests_support::sample(0, &[("eth0", 1, 1)]),
                tests_support::sample(1, &[("eth0", 2, 2)]),
                tests_support::sample(2, &[("eth0", 3, 3)]),
            ],
            ..Default::default()
        };
        assert_eq!(apply_key(&mut m, Key::Char('p')), Action::Redraw);
        assert!(m.paused);
        assert_eq!(m.mode, Mode::PausaScrub);
        assert_eq!(m.scrub_idx, Some(2), "borde vivo = último índice visible");
    }

    #[test]
    fn apply_key_left_right_mueven_scrub_idx_con_clamp() {
        let mut m = LiveModel {
            mode: Mode::PausaScrub,
            paused: true,
            scrub_idx: Some(2),
            samples: vec![
                tests_support::sample(0, &[("eth0", 1, 1)]),
                tests_support::sample(1, &[("eth0", 2, 2)]),
                tests_support::sample(2, &[("eth0", 3, 3)]),
            ],
            ..Default::default()
        };
        assert_eq!(apply_key(&mut m, Key::Left), Action::Redraw);
        assert_eq!(m.scrub_idx, Some(1));
        apply_key(&mut m, Key::Left);
        assert_eq!(m.scrub_idx, Some(0));
        apply_key(&mut m, Key::Left); // clamp por debajo de 0
        assert_eq!(m.scrub_idx, Some(0));
        apply_key(&mut m, Key::Right);
        apply_key(&mut m, Key::Right);
        apply_key(&mut m, Key::Right); // clamp por encima del máximo (2)
        assert_eq!(m.scrub_idx, Some(2));
    }

    #[test]
    fn apply_key_p_en_pausa_scrub_reanuda_y_limpia_scrub_idx() {
        let mut m = LiveModel {
            mode: Mode::PausaScrub,
            paused: true,
            scrub_idx: Some(0),
            samples: vec![tests_support::sample(0, &[("eth0", 1, 1)])],
            ..Default::default()
        };
        assert_eq!(apply_key(&mut m, Key::Char('p')), Action::Redraw);
        assert_eq!(m.mode, Mode::Normal);
        assert!(!m.paused);
        assert_eq!(m.scrub_idx, None);
    }

    #[test]
    fn esc_en_normal_sale_de_la_app() {
        let mut m = LiveModel {
            mode: Mode::Normal,
            ..Default::default()
        };
        assert_eq!(apply_key(&mut m, Key::Esc), Action::Quit);
    }

    #[test]
    fn q_solo_sale_en_normal() {
        for modo in [Mode::Ayuda, Mode::Zoom, Mode::Filtro, Mode::PausaScrub] {
            let mut m = LiveModel {
                mode: modo,
                ..Default::default()
            };
            assert_ne!(
                apply_key(&mut m, Key::Char('q')),
                Action::Quit,
                "modo: {modo:?}"
            );
        }
        let mut normal = LiveModel {
            mode: Mode::Normal,
            ..Default::default()
        };
        assert_eq!(apply_key(&mut normal, Key::Char('q')), Action::Quit);
    }

    #[test]
    fn nuevos_campos_de_live_model_tienen_default_esperado() {
        let m = LiveModel::default();
        assert_eq!(m.mode, Mode::Normal);
        assert_eq!(m.focus, Focus::Grafico);
        assert_eq!(m.selected_row, 0);
        assert_eq!(m.filtro, "");
        assert_eq!(m.scrub_idx, None);
        assert_eq!(m.reconnect_attempts, 0);
    }

    #[test]
    fn top_apps_solo_agrega_ventana_de_panel() {
        // 65 samples: app "vieja" solo en los 5 más antiguos, "nueva" en los últimos 60.
        // top_apps debe agregar solo los últimos PANEL_WINDOW_SECS (60) → "vieja" excluida.
        let mut samples = Vec::new();
        for i in 0..5 {
            samples.push(sample(i, &[], &[("vieja", 100, 10)]));
        }
        for i in 5..65 {
            samples.push(sample(i, &[], &[("nueva", 1, 1)]));
        }
        let m = LiveModel {
            samples,
            ..Default::default()
        };
        let apps = m.top_apps();
        assert!(
            apps.iter().all(|(a, _, _)| a != "vieja"),
            "vieja fuera de la ventana de panel: {apps:?}"
        );
        assert!(apps.iter().any(|(a, _, _)| a == "nueva"));
    }

    #[test]
    fn throughput_usa_todos_los_samples() {
        // el gráfico SÍ usa toda la ventana (no solo 60)
        let mut samples = Vec::new();
        for i in 0..65 {
            samples.push(sample(i, &[("eth0", 1, 1)], &[]));
        }
        let m = LiveModel {
            samples,
            ..Default::default()
        };
        let (rx, _tx) = m.throughput_series();
        assert_eq!(rx.len(), 65, "el gráfico usa todos los samples, no solo 60");
    }

    #[test]
    fn apply_key_u_cicla_tres_modos() {
        use crate::render::UnitMode;
        let mut m = LiveModel::default();
        assert_eq!(m.units, UnitMode::Binary); // default
        assert_eq!(apply_key(&mut m, Key::Char('u')), Action::Redraw);
        assert_eq!(m.units, UnitMode::Decimal);
        apply_key(&mut m, Key::Char('u'));
        assert_eq!(m.units, UnitMode::Bits);
        apply_key(&mut m, Key::Char('u'));
        assert_eq!(m.units, UnitMode::Binary); // vuelve al inicio
    }

    #[test]
    fn apply_key_uap_marcan_flash() {
        let mut m = LiveModel::default();
        apply_key(&mut m, Key::Char('u'));
        assert_eq!(m.flash_key, Some('u'));
        assert_eq!(m.flash_frames, 2);
        apply_key(&mut m, Key::Char('p'));
        assert_eq!(m.flash_key, Some('p'));
        apply_key(&mut m, Key::Char('a'));
        assert_eq!(m.flash_key, Some('a'));
    }

    #[test]
    fn apply_key_quit_y_desconocida_no_marcan_flash() {
        let mut m = LiveModel::default();
        apply_key(&mut m, Key::Char('q')); // Quit
        assert_eq!(m.flash_key, None);
        apply_key(&mut m, Key::Char('z')); // desconocida
        assert_eq!(m.flash_key, None);
    }

    #[test]
    fn keymap_activo_longitud_y_teclas_unicas() {
        let mapa = keymap_activo();
        assert!(
            mapa.len() >= 10,
            "el keymap de 0.6.0 debe listar al menos las 10 teclas nuevas/existentes: {mapa:?}"
        );
        let mut teclas: Vec<&str> = mapa.iter().map(|(tecla, _)| *tecla).collect();
        let antes = teclas.len();
        teclas.sort_unstable();
        teclas.dedup();
        assert_eq!(
            teclas.len(),
            antes,
            "no debe haber teclas duplicadas: {mapa:?}"
        );
    }

    #[test]
    fn apply_key_interrogacion_togglea_ayuda() {
        let mut m = LiveModel::default();
        assert_eq!(m.mode, Mode::Normal);
        assert_eq!(apply_key(&mut m, Key::Char('?')), Action::Redraw);
        assert_eq!(m.mode, Mode::Ayuda);
        assert_eq!(apply_key(&mut m, Key::Char('?')), Action::Redraw);
        assert_eq!(m.mode, Mode::Normal);
    }

    #[test]
    fn cycle_focus_cuatro_focos() {
        assert_eq!(cycle_focus(Focus::Grafico, true), Focus::Apps);
        assert_eq!(cycle_focus(Focus::Apps, true), Focus::Interfaces);
        assert_eq!(cycle_focus(Focus::Interfaces, true), Focus::Servidores);
        assert_eq!(cycle_focus(Focus::Servidores, true), Focus::Grafico);
        // hacia atrás
        assert_eq!(cycle_focus(Focus::Grafico, false), Focus::Servidores);
        assert_eq!(cycle_focus(Focus::Apps, false), Focus::Grafico);
    }

    #[test]
    fn apply_key_v_alterna_vista_del_panel_enfocado() {
        let mut m = LiveModel {
            focus: Focus::Apps,
            ..Default::default()
        };
        assert_eq!(m.vista_apps, Vista::Ahora);
        apply_key(&mut m, Key::Char('v'));
        assert_eq!(m.vista_apps, Vista::Historico);
        m.focus = Focus::Servidores;
        apply_key(&mut m, Key::Char('v'));
        assert_eq!(m.vista_servidores, Vista::Historico);
        m.focus = Focus::Grafico;
        let before = (m.vista_apps, m.vista_ifaces, m.vista_servidores);
        apply_key(&mut m, Key::Char('v')); // Grafico: no-op
        assert_eq!((m.vista_apps, m.vista_ifaces, m.vista_servidores), before);
    }

    #[test]
    fn apply_key_v_reinicia_seleccion() {
        let mut m = LiveModel {
            focus: Focus::Apps,
            selected_row: 5,
            ..Default::default()
        };
        apply_key(&mut m, Key::Char('v'));
        assert_eq!(m.selected_row, 0, "alternar vista reinicia la selección");
    }

    #[test]
    fn drill_arma_vista_del_panel() {
        use caudalghost_core::proto::AppRow;
        let mut m = LiveModel {
            focus: Focus::Apps,
            vista_apps: Vista::Historico,
            hist_apps: vec![AppRow {
                app: "firefox".into(),
                rx: 9,
                tx: 1,
            }],
            ..Default::default()
        };
        apply_key(&mut m, Key::Right);
        assert_eq!(m.mode, Mode::Drilldown);
        assert_eq!(m.drill.as_ref().map(|d| d.vista), Some(Vista::Historico));
    }

    #[test]
    fn mover_indice_clampa_en_ambos_extremos_y_len_cero() {
        assert_eq!(mover_indice(0, 5, -1), 0); // no baja de 0
        assert_eq!(mover_indice(4, 5, 1), 4); // no sube de len-1
        assert_eq!(mover_indice(2, 5, 1), 3);
        assert_eq!(mover_indice(2, 5, -1), 1);
        assert_eq!(mover_indice(0, 0, 1), 0); // len=0 => 0
    }

    #[test]
    fn apply_key_tab_cicla_focus_y_resetea_selected_row() {
        let mut m = LiveModel {
            focus: Focus::Grafico,
            selected_row: 3,
            ..Default::default()
        };
        assert_eq!(apply_key(&mut m, Key::Tab), Action::Redraw);
        assert_eq!(m.focus, Focus::Apps);
        assert_eq!(m.selected_row, 0);
    }

    #[test]
    fn apply_key_shift_tab_cicla_focus_hacia_atras() {
        let mut m = LiveModel {
            focus: Focus::Grafico,
            selected_row: 2,
            ..Default::default()
        };
        assert_eq!(apply_key(&mut m, Key::ShiftTab), Action::Redraw);
        assert_eq!(m.focus, Focus::Servidores);
        assert_eq!(m.selected_row, 0);
    }

    #[test]
    fn apply_key_j_k_mueven_selected_row_con_clamp() {
        let mut m = LiveModel {
            focus: Focus::Apps,
            selected_row: 0,
            samples: vec![tests_support::sample_apps(
                0,
                &[("firefox", 1, 1), ("curl", 1, 1), ("wget", 1, 1)],
            )],
            ..Default::default()
        };
        assert_eq!(apply_key(&mut m, Key::Char('j')), Action::Redraw);
        assert_eq!(m.selected_row, 1);
        assert_eq!(apply_key(&mut m, Key::Down), Action::Redraw);
        assert_eq!(m.selected_row, 2);
        // clamp: no hay cuarta fila
        apply_key(&mut m, Key::Char('j'));
        assert_eq!(m.selected_row, 2);
        assert_eq!(apply_key(&mut m, Key::Char('k')), Action::Redraw);
        assert_eq!(m.selected_row, 1);
        apply_key(&mut m, Key::Up);
        assert_eq!(m.selected_row, 0);
    }

    #[test]
    fn apply_key_j_k_sin_efecto_si_foco_es_grafico() {
        let mut m = LiveModel {
            focus: Focus::Grafico,
            selected_row: 0,
            ..Default::default()
        };
        assert_eq!(apply_key(&mut m, Key::Char('j')), Action::None);
        assert_eq!(m.selected_row, 0);
    }

    #[test]
    fn apply_key_enter_en_normal_entra_en_zoom_del_panel_enfocado() {
        let mut m = LiveModel {
            mode: Mode::Normal,
            focus: Focus::Apps,
            ..Default::default()
        };
        assert_eq!(apply_key(&mut m, Key::Enter), Action::Redraw);
        assert_eq!(m.mode, Mode::Zoom);
        assert_eq!(m.focus, Focus::Apps, "el zoom no cambia el panel enfocado");
    }

    #[test]
    fn apply_key_enter_en_zoom_permanece_en_zoom() {
        let mut m = LiveModel {
            mode: Mode::Zoom,
            focus: Focus::Grafico,
            ..Default::default()
        };
        assert_eq!(apply_key(&mut m, Key::Enter), Action::Redraw);
        assert_eq!(m.mode, Mode::Zoom);
    }

    #[test]
    fn apply_key_esc_en_zoom_vuelve_a_normal() {
        let mut m = LiveModel {
            mode: Mode::Zoom,
            focus: Focus::Interfaces,
            ..Default::default()
        };
        assert_eq!(apply_key(&mut m, Key::Esc), Action::Redraw);
        assert_eq!(m.mode, Mode::Normal);
        assert_eq!(
            m.focus,
            Focus::Interfaces,
            "Esc no cambia el panel enfocado"
        );
    }

    #[test]
    fn nombre_coincide_substring_case_insensitive() {
        assert!(nombre_coincide("firefox", "fire"));
        assert!(nombre_coincide("FireFox", "fire"));
        assert!(nombre_coincide("firefox", "FIRE"));
        assert!(!nombre_coincide("curl", "fire"));
    }

    #[test]
    fn nombre_coincide_patron_vacio_siempre_true() {
        assert!(nombre_coincide("firefox", ""));
        assert!(nombre_coincide("", ""));
    }

    #[test]
    fn apply_key_barra_entra_en_modo_filtro() {
        let mut m = LiveModel::default();
        assert_eq!(m.mode, Mode::Normal);
        assert_eq!(apply_key(&mut m, Key::Char('/')), Action::Redraw);
        assert_eq!(m.mode, Mode::Filtro);
        assert_eq!(m.filtro, "");
    }

    #[test]
    fn apply_key_en_filtro_char_y_backspace_editan_filtro() {
        let mut m = LiveModel {
            mode: Mode::Filtro,
            ..Default::default()
        };
        apply_key(&mut m, Key::Char('f'));
        apply_key(&mut m, Key::Char('i'));
        apply_key(&mut m, Key::Char('r'));
        assert_eq!(m.filtro, "fir");
        apply_key(&mut m, Key::Backspace);
        assert_eq!(m.filtro, "fi");
        // sigue en modo Filtro mientras se edita
        assert_eq!(m.mode, Mode::Filtro);
    }

    #[test]
    fn apply_key_enter_en_filtro_confirma_y_vuelve_a_normal() {
        let mut m = LiveModel {
            mode: Mode::Filtro,
            filtro: "fire".into(),
            ..Default::default()
        };
        assert_eq!(apply_key(&mut m, Key::Enter), Action::Redraw);
        assert_eq!(m.mode, Mode::Normal);
        // el filtro queda visible tras confirmar
        assert_eq!(m.filtro, "fire");
    }

    #[test]
    fn apply_key_tab_limpia_filtro_al_cambiar_de_foco() {
        let mut m = LiveModel {
            mode: Mode::Normal,
            focus: Focus::Apps,
            filtro: "fire".into(),
            ..Default::default()
        };
        apply_key(&mut m, Key::Tab);
        assert_eq!(
            m.filtro, "",
            "el filtro se limpia al cambiar de foco con Tab"
        );
        assert_ne!(m.focus, Focus::Apps);
    }

    #[test]
    fn frescura_escala_verde_ambar_rojo() {
        assert_eq!(frescura(10, 11), (Nivel::Verde, "hace 1 s".to_string()));
        assert_eq!(frescura(10, 13), (Nivel::Ambar, "hace 3 s".to_string()));
        assert_eq!(frescura(10, 17), (Nivel::Rojo, "hace 7 s".to_string()));
    }

    #[test]
    fn frescura_limites_2_y_5_segundos() {
        assert_eq!(frescura(0, 2).0, Nivel::Ambar, "2s ya no es Verde");
        assert_eq!(frescura(0, 5).0, Nivel::Ambar, "5s todavía es Ambar");
        assert_eq!(frescura(0, 6).0, Nivel::Rojo, "6s ya es Rojo");
    }

    #[test]
    fn siguiente_backoff_crece_hasta_tope_5s() {
        use std::time::Duration;
        assert_eq!(siguiente_backoff(0), Duration::from_secs(1));
        assert_eq!(siguiente_backoff(1), Duration::from_secs(2));
        assert_eq!(siguiente_backoff(2), Duration::from_secs(4));
        assert_eq!(siguiente_backoff(3), Duration::from_secs(5));
        assert_eq!(siguiente_backoff(4), Duration::from_secs(5));
        assert_eq!(siguiente_backoff(10), Duration::from_secs(5));
    }

    #[test]
    fn nivel_umbral_rojo_sobre_p90_ambar_sobre_mediana_verde_resto() {
        // serie ordenada 1..=10: mediana=5, p90=9 (índice 90% de 10 elementos, redondeo hacia abajo)
        let serie: Vec<u64> = (1..=10).collect();
        assert_eq!(nivel_umbral(&serie, 10), Nivel::Rojo, "10 > p90(9)");
        assert_eq!(
            nivel_umbral(&serie, 6),
            Nivel::Ambar,
            "6 > mediana(5) y <= p90(9)"
        );
        assert_eq!(nivel_umbral(&serie, 3), Nivel::Verde, "3 <= mediana(5)");
    }

    #[test]
    fn nivel_umbral_serie_vacia_es_verde() {
        assert_eq!(nivel_umbral(&[], 100), Nivel::Verde);
    }

    #[test]
    fn media_reciente_suaviza_los_ultimos_n() {
        use super::media_reciente;
        // media de los últimos 3: (30+40+50)/3 = 40 (ignora los primeros, spiky)
        assert_eq!(media_reciente(&[1000, 0, 30, 40, 50], 3), 40);
        // n mayor que la serie → media de toda la serie
        assert_eq!(media_reciente(&[10, 20], 5), 15);
        // suaviza un pico suelto: [0,0,0,0,120] con ventana 5 → 24 (no 120)
        assert_eq!(media_reciente(&[0, 0, 0, 0, 120], 5), 24);
        assert_eq!(media_reciente(&[], 5), 0);
        assert_eq!(media_reciente(&[10, 20, 30], 0), 0);
    }

    #[test]
    fn serie_suavizada_media_movil_trasera() {
        use super::{media_reciente, serie_suavizada};
        // ventana 2: cada punto = media de los ≤2 valores que terminan en él
        assert_eq!(serie_suavizada(&[10, 20, 30], 2), vec![10, 15, 25]);
        // el último punto coincide con la media reciente de toda la serie
        let s = [4u64, 8, 12, 16];
        assert_eq!(
            *serie_suavizada(&s, 3).last().unwrap(),
            media_reciente(&s, 3)
        );
        assert!(serie_suavizada(&[], 5).is_empty());
    }

    #[test]
    fn mark_disconnected_no_borra_samples_ni_historico() {
        let mut m = LiveModel {
            connected: true,
            samples: vec![tests_support::sample(0, &[("eth0", 10, 1)])],
            hist_apps: vec![caudalghost_core::proto::AppRow {
                app: "firefox".into(),
                rx: 100,
                tx: 10,
            }],
            hist_ifaces: vec![caudalghost_core::proto::TotalRow {
                iface: "eth0".into(),
                rx: 100,
                tx: 10,
            }],
            ..Default::default()
        };
        let samples_antes = m.samples.clone();
        let hist_apps_antes = m.hist_apps.clone();
        let hist_ifaces_antes = m.hist_ifaces.clone();
        crate::live::mark_disconnected(&mut m);
        assert!(!m.connected);
        assert_eq!(m.samples, samples_antes, "samples no debe cambiar");
        assert_eq!(m.hist_apps, hist_apps_antes, "hist_apps no debe cambiar");
        assert_eq!(
            m.hist_ifaces, hist_ifaces_antes,
            "hist_ifaces no debe cambiar"
        );
    }

    #[test]
    fn hosts_de_app_agrega_por_host_y_ordena() {
        use caudalghost_core::types::{AppRemoteDelta, Sample};
        let s = |ts: i64, flows: Vec<(&str, &str, u64, u64)>| Sample {
            ts,
            deltas: vec![],
            apps: vec![],
            flows: flows
                .into_iter()
                .map(|(a, h, rx, tx)| AppRemoteDelta {
                    app: a.into(),
                    host: h.into(),
                    rx,
                    tx,
                })
                .collect(),
        };
        let m = LiveModel {
            samples: vec![
                s(
                    0,
                    vec![
                        ("firefox", "github.com", 100, 10),
                        ("firefox", "scdn.co", 5, 5),
                    ],
                ),
                s(
                    1,
                    vec![
                        ("firefox", "github.com", 50, 0),
                        ("curl", "x.com", 999, 999),
                    ],
                ),
            ],
            ..Default::default()
        };
        let h = m.hosts_de_app("firefox");
        assert_eq!(h.len(), 2);
        assert_eq!(h[0], ("github.com".to_string(), 150, 10)); // mayor total primero
        assert!(m.hosts_de_app("desconocida").is_empty());
    }

    #[test]
    fn app_seleccionada_por_foco_y_seleccion() {
        use caudalghost_core::proto::AppRow;
        let mut m = LiveModel {
            focus: Focus::Apps,
            vista_apps: Vista::Historico,
            hist_apps: vec![
                AppRow {
                    app: "mega".into(),
                    rx: 9_000,
                    tx: 1,
                },
                AppRow {
                    app: "firefox".into(),
                    rx: 100,
                    tx: 1,
                },
            ],
            ..Default::default()
        };
        m.selected_row = 1;
        assert_eq!(m.app_seleccionada().as_deref(), Some("firefox")); // ordenado desc: mega(0), firefox(1)
        m.focus = Focus::Grafico;
        assert_eq!(m.app_seleccionada(), None);
    }

    #[test]
    fn rango_a_ventana_todo_y_token() {
        let ahora = chrono::Local::now();
        assert_eq!(super::rango_a_ventana("todo", ahora), (0, i64::MAX));
        let (s, e) = super::rango_a_ventana("7d", ahora);
        assert!(s < e && e <= ahora.timestamp() + 1);
    }

    #[test]
    fn apply_key_drill_entra_y_vuelve() {
        use caudalghost_core::proto::AppRow;
        let mut m = LiveModel {
            focus: Focus::Apps,
            vista_apps: Vista::Historico,
            hist_apps: vec![AppRow {
                app: "firefox".into(),
                rx: 9,
                tx: 1,
            }],
            ..Default::default()
        };
        assert_eq!(apply_key(&mut m, Key::Right), Action::Redraw);
        assert_eq!(m.mode, Mode::Drilldown);
        assert_eq!(m.drill.as_ref().map(|d| d.app.as_str()), Some("firefox"));
        assert_eq!(apply_key(&mut m, Key::Left), Action::Redraw);
        assert_eq!(m.mode, Mode::Normal);
        assert!(m.drill.is_none());
        // Esc también sale del drill:
        apply_key(&mut m, Key::Char('l'));
        assert_eq!(m.mode, Mode::Drilldown);
        apply_key(&mut m, Key::Esc);
        assert_eq!(m.mode, Mode::Normal);
    }

    #[test]
    fn apply_key_drill_noop_sin_app_o_foco_no_apps() {
        let mut m = LiveModel {
            focus: Focus::Grafico,
            ..Default::default()
        };
        assert_eq!(apply_key(&mut m, Key::Right), Action::None);
        assert_eq!(m.mode, Mode::Normal);
    }

    #[test]
    fn apply_key_r_cicla_rango() {
        let mut m = LiveModel::default();
        assert_eq!(m.hist_range_idx, 0);
        apply_key(&mut m, Key::Char('r'));
        assert_eq!(m.hist_range_idx, 1);
        for _ in 0..4 {
            apply_key(&mut m, Key::Char('r'));
        }
        assert_eq!(m.hist_range_idx, 0, "cicla con wrap");
    }

    #[test]
    fn top_hosts_vivo_agrega_todas_las_apps() {
        use caudalghost_core::types::{AppRemoteDelta, Sample};
        let s = |flows: Vec<(&str, &str, u64, u64)>| Sample {
            ts: 0,
            deltas: vec![],
            apps: vec![],
            flows: flows
                .into_iter()
                .map(|(a, h, rx, tx)| AppRemoteDelta {
                    app: a.into(),
                    host: h.into(),
                    rx,
                    tx,
                })
                .collect(),
        };
        let m = LiveModel {
            samples: vec![s(vec![
                ("firefox", "github.com", 100, 10),
                ("curl", "github.com", 50, 0),
                ("firefox", "scdn.co", 5, 5),
            ])],
            ..Default::default()
        };
        let h = m.top_hosts_vivo();
        assert_eq!(h[0], ("github.com".to_string(), 150, 10)); // suma firefox+curl al mismo host
        assert_eq!(h.len(), 2);
    }
}
