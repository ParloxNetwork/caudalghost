//! Modelo puro del dashboard en vivo y sus derivaciones (sin I/O, testeable).

/// Ventana de agregación de los paneles apps/interfaces — tasas responsivas
/// independientes del ancho del gráfico.
pub const PANEL_WINDOW_SECS: usize = 60;

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
}

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
}

/// Resultado de procesar una tecla.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    None,
    Redraw,
    Quit,
}

/// Reducer puro de teclado: muta el modelo y dice qué hacer.
/// 'q'/Esc salir · 'p' pausa · 'a' incluir lo. Otras teclas: None.
pub fn apply_key(model: &mut LiveModel, key: char) -> Action {
    match key {
        'q' | '\u{1b}' => Action::Quit,
        'p' => {
            model.paused = !model.paused;
            model.flash_key = Some(key);
            model.flash_frames = 2;
            Action::Redraw
        }
        'a' => {
            model.show_all = !model.show_all;
            model.flash_key = Some(key);
            model.flash_frames = 2;
            Action::Redraw
        }
        'u' => {
            model.units = model.units.next();
            model.flash_key = Some(key);
            model.flash_frames = 2;
            Action::Redraw
        }
        _ => Action::None,
    }
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
        assert_eq!(apply_key(&mut m, 'q'), Action::Quit);
        assert_eq!(apply_key(&mut m, '\u{1b}'), Action::Quit); // Esc
    }

    #[test]
    fn apply_key_pausa_y_show_all_togglean() {
        let mut m = LiveModel::default();
        assert!(!m.paused);
        assert_eq!(apply_key(&mut m, 'p'), Action::Redraw);
        assert!(m.paused);
        apply_key(&mut m, 'p');
        assert!(!m.paused);
        assert!(!m.show_all);
        apply_key(&mut m, 'a');
        assert!(m.show_all);
    }

    #[test]
    fn apply_key_desconocida_es_noop() {
        let mut m = LiveModel::default();
        assert_eq!(apply_key(&mut m, 'z'), Action::None);
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
        assert_eq!(apply_key(&mut m, 'u'), Action::Redraw);
        assert_eq!(m.units, UnitMode::Decimal);
        apply_key(&mut m, 'u');
        assert_eq!(m.units, UnitMode::Bits);
        apply_key(&mut m, 'u');
        assert_eq!(m.units, UnitMode::Binary); // vuelve al inicio
    }

    #[test]
    fn apply_key_uap_marcan_flash() {
        let mut m = LiveModel::default();
        apply_key(&mut m, 'u');
        assert_eq!(m.flash_key, Some('u'));
        assert_eq!(m.flash_frames, 2);
        apply_key(&mut m, 'p');
        assert_eq!(m.flash_key, Some('p'));
        apply_key(&mut m, 'a');
        assert_eq!(m.flash_key, Some('a'));
    }

    #[test]
    fn apply_key_quit_y_desconocida_no_marcan_flash() {
        let mut m = LiveModel::default();
        apply_key(&mut m, 'q'); // Quit
        assert_eq!(m.flash_key, None);
        apply_key(&mut m, 'z'); // desconocida
        assert_eq!(m.flash_key, None);
    }
}
