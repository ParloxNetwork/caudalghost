use crate::collector::CounterTracker;
use crate::live::LiveRing;
use crate::{rollup, storage};
use caudalghost_core::types::{AppDelta, AppRemoteDelta, IfaceCounters, Sample};
use rusqlite::Connection;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

/// Lógica del loop principal, separada de main para poder inyectar
/// contadores y timestamps en tests.
pub struct Daemon {
    conn: Connection,
    ring: Arc<Mutex<LiveRing>>,
    tracker: CounterTracker,
    minute_ts: i64,
    acc: HashMap<String, (u64, u64)>,
    app_acc: HashMap<String, (u64, u64)>,
    /// Acumulador del minuto por (app, dominio registrable) para persistir.
    remote_acc: HashMap<(String, String), (u64, u64)>,
    pending_apps: Vec<AppDelta>,
    pending_flows: Vec<AppRemoteDelta>,
}

impl Daemon {
    pub fn new(conn: Connection, ring: Arc<Mutex<LiveRing>>) -> Self {
        Self {
            conn,
            ring,
            tracker: CounterTracker::default(),
            minute_ts: -1,
            acc: HashMap::new(),
            app_acc: HashMap::new(),
            remote_acc: HashMap::new(),
            pending_apps: vec![],
            pending_flows: vec![],
        }
    }

    #[cfg(test)]
    pub fn conn(&self) -> &Connection {
        &self.conn
    }

    /// Recibe los deltas por-app de un drenado eBPF: acumula para el minuto
    /// y los deja pendientes para el próximo Sample del ring.
    pub fn ingest_apps(&mut self, apps: Vec<AppDelta>) {
        for a in &apps {
            let e = self.app_acc.entry(a.app.clone()).or_insert((0, 0));
            e.0 += a.rx;
            e.1 += a.tx;
        }
        self.pending_apps = apps;
    }

    /// Recibe los deltas por (app, host) de un drenado eBPF. Para el vivo, los deja
    /// pendientes con el FQDN; para el histórico, los acumula reducidos a dominio
    /// registrable (eTLD+1), que se volcará a `minute_app_remote` al cerrar el minuto.
    pub fn ingest_flows(&mut self, flows: Vec<AppRemoteDelta>) {
        for f in &flows {
            let key = (f.app.clone(), storage::etld1(&f.host));
            let e = self.remote_acc.entry(key).or_insert((0, 0));
            e.0 += f.rx;
            e.1 += f.tx;
        }
        self.pending_flows = flows;
    }

    /// Una pasada del loop: deltas → ring en vivo → acumulador del minuto;
    /// al cruzar frontera de minuto vuelca a SQLite y corre mantenimiento
    /// (barato: watermark y PK por ts).
    pub fn tick(&mut self, counters: Vec<IfaceCounters>, ts: i64) -> anyhow::Result<()> {
        let deltas = self.tracker.deltas(&counters);
        let minute = ts - ts.rem_euclid(60);
        if minute != self.minute_ts {
            self.flush_pending()?;
            self.minute_ts = minute;
            // maintain encapsula el orden obligatorio hours→days→prune.
            // #1 "degrada, no abortes": el mantenimiento NUNCA debe matar el
            // daemon. Si falla (p. ej. datos envenenados que violan una
            // constraint), se registra y se sigue sirviendo en vivo; el histórico
            // se recupera en el próximo minuto. Misma filosofía que eBPF/ACL.
            if let Err(e) = rollup::maintain(&self.conn, ts) {
                eprintln!(
                    "caudalghostd: mantenimiento de base falló ({e}); sigo en vivo, \
                     reintento al próximo minuto"
                );
            }
        }
        for d in &deltas {
            let e = self.acc.entry(d.iface.clone()).or_insert((0, 0));
            e.0 += d.rx;
            e.1 += d.tx;
        }
        self.ring.lock().unwrap().push(Sample {
            ts,
            deltas,
            apps: std::mem::take(&mut self.pending_apps),
            flows: std::mem::take(&mut self.pending_flows),
        });
        Ok(())
    }

    /// Vuelca el acumulado pendiente (cierre de minuto o apagado ordenado).
    pub fn flush_pending(&mut self) -> anyhow::Result<()> {
        if self.minute_ts >= 0 && !self.acc.is_empty() {
            let rows: Vec<(String, u64, u64)> =
                self.acc.drain().map(|(i, (rx, tx))| (i, rx, tx)).collect();
            storage::insert_minute(&self.conn, self.minute_ts, &rows)?;
        }
        self.acc.clear();
        if self.minute_ts >= 0 && !self.app_acc.is_empty() {
            let rows: Vec<(String, u64, u64)> = self
                .app_acc
                .drain()
                .map(|(a, (rx, tx))| (a, rx, tx))
                .collect();
            storage::insert_minute_apps(&self.conn, self.minute_ts, &rows)?;
        }
        self.app_acc.clear();
        if self.minute_ts >= 0 && !self.remote_acc.is_empty() {
            // Top-N por (minuto, app) + "(otros)" antes de persistir (spec §3.4).
            let acc = std::mem::take(&mut self.remote_acc);
            let rows = storage::apply_top_n(acc, 50);
            storage::insert_minute_app_remote(&self.conn, self.minute_ts, &rows)?;
        }
        self.remote_acc.clear();
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::live::LiveRing;
    use crate::storage;
    use caudalghost_core::types::IfaceCounters;
    use std::path::Path;
    use std::sync::{Arc, Mutex};

    fn c(iface: &str, rx: u64, tx: u64) -> IfaceCounters {
        IfaceCounters {
            iface: iface.into(),
            rx_bytes: rx,
            tx_bytes: tx,
        }
    }

    #[test]
    fn vuelca_el_minuto_al_cruzar_la_frontera() {
        let conn = storage::open(Path::new(":memory:")).unwrap();
        let ring = Arc::new(Mutex::new(LiveRing::new(3600)));
        let mut d = Daemon::new(conn, ring.clone());
        d.tick(vec![c("eth0", 1000, 100)], 59).unwrap(); // línea base, delta 0
        d.tick(vec![c("eth0", 2000, 150)], 60).unwrap(); // cruza → vuelca minuto 0
        d.tick(vec![c("eth0", 2500, 170)], 120).unwrap(); // cruza → vuelca minuto 60
        let rows = storage::query_total(d.conn(), 60, 120, 120).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!((rows[0].rx, rows[0].tx), (1000, 50));
        // La vista en vivo recibió las tres muestras.
        assert_eq!(ring.lock().unwrap().last(10).len(), 3);
    }

    #[test]
    fn tick_no_muere_si_el_mantenimiento_falla() {
        // #1 "degrada, no abortes": si rollup::maintain falla —aquí forzado al
        // dropear una tabla que necesita, simulando datos envenenados/DB rara—
        // tick debe REGISTRAR y seguir sirviendo en vivo, nunca propagar el Err
        // (que mataría el proceso → el crash-loop que inutilizaba la instalación).
        let conn = storage::open(Path::new(":memory:")).unwrap();
        conn.execute_batch("DROP TABLE hour_iface;").unwrap();
        let ring = Arc::new(Mutex::new(LiveRing::new(3600)));
        let mut d = Daemon::new(conn, ring.clone());
        // Cruza frontera de minuto en el primer tick → dispara maintain (que peta).
        let r = d.tick(vec![c("eth0", 1000, 100)], 60);
        assert!(r.is_ok(), "tick debe degradar, no propagar el error de maintain");
        // La vista en vivo sigue recibiendo muestras pese al fallo de mantenimiento.
        assert_eq!(ring.lock().unwrap().last(10).len(), 1);
    }

    #[test]
    fn flush_pending_no_pierde_el_minuto_en_curso() {
        let conn = storage::open(Path::new(":memory:")).unwrap();
        let ring = Arc::new(Mutex::new(LiveRing::new(3600)));
        let mut d = Daemon::new(conn, ring);
        d.tick(vec![c("eth0", 0, 0)], 60).unwrap();
        d.tick(vec![c("eth0", 300, 30)], 61).unwrap();
        d.flush_pending().unwrap(); // apagado ordenado (SIGTERM)
        let rows = storage::query_total(d.conn(), 60, 120, 119).unwrap();
        assert_eq!((rows[0].rx, rows[0].tx), (300, 30));
    }

    #[test]
    fn ingest_apps_llega_al_ring_y_a_sqlite() {
        use caudalghost_core::types::AppDelta;
        let conn = storage::open(Path::new(":memory:")).unwrap();
        let ring = Arc::new(Mutex::new(LiveRing::new(3600)));
        let mut d = Daemon::new(conn, ring.clone());
        d.tick(vec![c("eth0", 0, 0)], 60).unwrap();
        d.ingest_apps(vec![AppDelta {
            app: "firefox".into(),
            rx: 100,
            tx: 10,
        }]);
        d.tick(vec![c("eth0", 0, 0)], 61).unwrap(); // el sample de ts=61 lleva las apps
        let samples = ring.lock().unwrap().last(10);
        assert!(samples
            .last()
            .unwrap()
            .apps
            .iter()
            .any(|a| a.app == "firefox"));
        d.tick(vec![c("eth0", 0, 0)], 120).unwrap(); // cruza minuto → vuelca minute_app
        let rows = storage::query_total_apps(d.conn(), 60, 120, 120).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!((rows[0].rx, rows[0].tx), (100, 10));
    }

    #[test]
    fn flows_se_persisten_reducidos_a_etld1() {
        use caudalghost_core::types::AppRemoteDelta;
        let conn = storage::open(Path::new(":memory:")).unwrap();
        let ring = Arc::new(Mutex::new(LiveRing::new(3600)));
        let mut d = Daemon::new(conn, ring);
        d.tick(vec![c("eth0", 0, 0)], 60).unwrap();
        d.ingest_flows(vec![
            AppRemoteDelta {
                app: "firefox".into(),
                host: "audio-fa.scdn.co".into(),
                rx: 100,
                tx: 10,
            },
            AppRemoteDelta {
                app: "firefox".into(),
                host: "api.scdn.co".into(),
                rx: 50,
                tx: 5,
            },
        ]);
        d.tick(vec![c("eth0", 0, 0)], 120).unwrap(); // cruza minuto → vuelca
                                                     // Ambos hosts colapsan a scdn.co → 150/15
        let rows = storage::query_top_remotes(d.conn(), 60, 120, Some("firefox"), 120).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(
            (rows[0].0.as_str(), rows[0].1, rows[0].2),
            ("scdn.co", 150, 15)
        );
    }

    #[test]
    fn ingest_flows_llega_al_ring() {
        use caudalghost_core::types::AppRemoteDelta;
        let conn = storage::open(Path::new(":memory:")).unwrap();
        let ring = Arc::new(Mutex::new(LiveRing::new(3600)));
        let mut d = Daemon::new(conn, ring.clone());
        d.tick(vec![c("eth0", 0, 0)], 60).unwrap();
        d.ingest_flows(vec![AppRemoteDelta {
            app: "firefox".into(),
            host: "1.1.1.1".into(),
            rx: 100,
            tx: 10,
        }]);
        d.tick(vec![c("eth0", 0, 0)], 61).unwrap(); // el sample de ts=61 lleva los flows
        let samples = ring.lock().unwrap().last(10);
        assert!(samples
            .last()
            .unwrap()
            .flows
            .iter()
            .any(|f| f.host == "1.1.1.1"));
    }

    #[test]
    fn flush_pending_cubre_apps() {
        use caudalghost_core::types::AppDelta;
        let conn = storage::open(Path::new(":memory:")).unwrap();
        let ring = Arc::new(Mutex::new(LiveRing::new(3600)));
        let mut d = Daemon::new(conn, ring);
        d.tick(vec![c("eth0", 0, 0)], 60).unwrap();
        d.ingest_apps(vec![AppDelta {
            app: "curl".into(),
            rx: 7,
            tx: 3,
        }]);
        d.flush_pending().unwrap();
        let rows = storage::query_total_apps(d.conn(), 60, 120, 119).unwrap();
        assert_eq!((rows[0].rx, rows[0].tx), (7, 3));
    }
}
