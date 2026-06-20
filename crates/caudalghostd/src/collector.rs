use caudalghost_core::types::{IfaceCounters, IfaceDelta};
use std::collections::HashMap;
use std::fs;
use std::path::Path;

/// Lee rx_bytes/tx_bytes de cada interfaz bajo `sysfs_net` (/sys/class/net).
/// Las entradas sin statistics legibles se ignoran en silencio.
pub fn read_counters(sysfs_net: &Path) -> std::io::Result<Vec<IfaceCounters>> {
    let mut out = Vec::new();
    for entry in fs::read_dir(sysfs_net)? {
        let entry = entry?;
        let stats = entry.path().join("statistics");
        let rx = read_u64(&stats.join("rx_bytes"));
        let tx = read_u64(&stats.join("tx_bytes"));
        if let (Some(rx_bytes), Some(tx_bytes)) = (rx, tx) {
            out.push(IfaceCounters {
                iface: entry.file_name().to_string_lossy().into_owned(),
                rx_bytes,
                tx_bytes,
            });
        }
    }
    out.sort_by(|a, b| a.iface.cmp(&b.iface));
    Ok(out)
}

fn read_u64(p: &Path) -> Option<u64> {
    fs::read_to_string(p).ok()?.trim().parse().ok()
}

/// Convierte lecturas absolutas en deltas, tolerando resets e interfaces nuevas.
#[derive(Default)]
pub struct CounterTracker {
    last: HashMap<String, (u64, u64)>,
}

impl CounterTracker {
    pub fn deltas(&mut self, counters: &[IfaceCounters]) -> Vec<IfaceDelta> {
        let mut out = Vec::with_capacity(counters.len());
        for c in counters {
            let (rx, tx) = match self.last.get(&c.iface) {
                Some(&(rx0, tx0)) => (
                    if c.rx_bytes >= rx0 {
                        c.rx_bytes - rx0
                    } else {
                        c.rx_bytes
                    },
                    if c.tx_bytes >= tx0 {
                        c.tx_bytes - tx0
                    } else {
                        c.tx_bytes
                    },
                ),
                None => (0, 0), // sin línea base no se inventa tráfico
            };
            self.last.insert(c.iface.clone(), (c.rx_bytes, c.tx_bytes));
            out.push(IfaceDelta {
                iface: c.iface.clone(),
                rx,
                tx,
            });
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use caudalghost_core::types::IfaceCounters;

    /// Imita /sys/class/net en un tempdir.
    fn fake_sysfs(ifaces: &[(&str, u64, u64)]) -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        for (name, rx, tx) in ifaces {
            let stats = dir.path().join(name).join("statistics");
            std::fs::create_dir_all(&stats).unwrap();
            std::fs::write(stats.join("rx_bytes"), format!("{rx}\n")).unwrap();
            std::fs::write(stats.join("tx_bytes"), format!("{tx}\n")).unwrap();
        }
        dir
    }

    #[test]
    fn lee_contadores_ordenados() {
        let sys = fake_sysfs(&[("wlan0", 500, 60), ("eth0", 1000, 200)]);
        let c = read_counters(sys.path()).unwrap();
        assert_eq!(
            c,
            vec![
                IfaceCounters {
                    iface: "eth0".into(),
                    rx_bytes: 1000,
                    tx_bytes: 200
                },
                IfaceCounters {
                    iface: "wlan0".into(),
                    rx_bytes: 500,
                    tx_bytes: 60
                },
            ]
        );
    }

    #[test]
    fn ignora_entradas_sin_statistics() {
        let sys = fake_sysfs(&[("eth0", 1, 1)]);
        std::fs::create_dir_all(sys.path().join("bonding_masters")).unwrap();
        assert_eq!(read_counters(sys.path()).unwrap().len(), 1);
    }

    fn c(iface: &str, rx: u64, tx: u64) -> IfaceCounters {
        IfaceCounters {
            iface: iface.into(),
            rx_bytes: rx,
            tx_bytes: tx,
        }
    }

    #[test]
    fn primera_lectura_da_delta_cero() {
        let mut t = CounterTracker::default();
        let d = t.deltas(&[c("eth0", 1000, 200)]);
        assert_eq!((d[0].rx, d[0].tx), (0, 0));
    }

    #[test]
    fn delta_normal_y_reset() {
        let mut t = CounterTracker::default();
        t.deltas(&[c("eth0", 1000, 200)]);
        let d = t.deltas(&[c("eth0", 1500, 250)]);
        assert_eq!((d[0].rx, d[0].tx), (500, 50));
        // El contador retrocede → reset del kernel: delta = valor actual.
        let d = t.deltas(&[c("eth0", 80, 10)]);
        assert_eq!((d[0].rx, d[0].tx), (80, 10));
    }
}
