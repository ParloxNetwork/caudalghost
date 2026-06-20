use crate::resolve::HostResolver;
use aya::maps::{MapData, PerCpuHashMap};
use aya::programs::KProbe;
use caudalghost_core::types::{AppDelta, AppRemoteDelta};
use caudalghost_ebpf_common::{FlowKey, TrafficVal};
use std::collections::HashMap;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

const PROBES: [&str; 4] = [
    "tcp_sendmsg",
    "tcp_cleanup_rbuf",
    "udp_sendmsg",
    "skb_consume_udp",
];

/// Motor eBPF: mantiene vivos los programas y drena el mapa de tráfico.
pub struct EbpfEngine {
    // se conserva para que los programas sigan attacheados
    _ebpf: aya::Ebpf,
    traffic: PerCpuHashMap<MapData, FlowKey, TrafficVal>,
}

impl EbpfEngine {
    /// Carga el objeto BPF embebido y attachea las 4 kprobes.
    /// Falla sin privilegios (CAP_BPF+CAP_PERFMON) — el llamador degrada honesto.
    pub fn load() -> anyhow::Result<Self> {
        let mut ebpf = aya::Ebpf::load(aya::include_bytes_aligned!(concat!(
            env!("OUT_DIR"),
            "/bpf"
        )))?;
        for name in PROBES {
            let prog: &mut KProbe = ebpf
                .program_mut(name)
                .ok_or_else(|| anyhow::anyhow!("programa {name} no encontrado en el objeto"))?
                .try_into()?;
            prog.load()?;
            prog.attach(name, 0)?;
        }
        let traffic = PerCpuHashMap::try_from(
            ebpf.take_map("TRAFFIC")
                .ok_or_else(|| anyhow::anyhow!("mapa TRAFFIC no encontrado"))?,
        )?;
        Ok(Self {
            _ebpf: ebpf,
            traffic,
        })
    }

    /// Lee y vacía el mapa: suma los valores per-CPU de cada clave y la borra.
    /// (Los bytes que lleguen entre lectura y borrado se pierden: ínfimo, aceptado.)
    pub fn drain(&mut self) -> anyhow::Result<Vec<(FlowKey, TrafficVal)>> {
        let keys: Vec<FlowKey> = self.traffic.keys().collect::<Result<_, _>>()?;
        let mut out = Vec::with_capacity(keys.len());
        for k in keys {
            if let Ok(percpu) = self.traffic.get(&k, 0) {
                let mut total = TrafficVal::default();
                for v in percpu.iter() {
                    total.rx += v.rx;
                    total.tx += v.tx;
                }
                out.push((k, total));
            }
            let _ = self.traffic.remove(&k);
        }
        Ok(out)
    }
}

/// Colapsa los contadores por-tgid en deltas por-app (función pura, testeable).
pub fn aggregate_by_app(
    drained: &[(FlowKey, TrafficVal)],
    resolve: &mut impl FnMut(u32) -> String,
) -> Vec<AppDelta> {
    let mut acc: HashMap<String, (u64, u64)> = HashMap::new();
    for (k, v) in drained {
        let e = acc.entry(resolve(k.tgid)).or_default();
        e.0 += v.rx;
        e.1 += v.tx;
    }
    acc.into_iter()
        .map(|(app, (rx, tx))| AppDelta { app, rx, tx })
        .collect()
}

/// IP del remoto de una clave, si se capturó una dirección válida (no comodín).
pub fn flow_ip(k: &FlowKey) -> Option<IpAddr> {
    match k.family {
        4 => {
            let o = [k.addr[0], k.addr[1], k.addr[2], k.addr[3]];
            if o == [0, 0, 0, 0] {
                None
            } else {
                Some(IpAddr::V4(Ipv4Addr::from(o)))
            }
        }
        6 => {
            let ip = Ipv6Addr::from(k.addr);
            if ip.is_unspecified() {
                None
            } else {
                Some(IpAddr::V6(ip))
            }
        }
        _ => None,
    }
}

/// Etiqueta legible del remoto de una clave: la IP, o "(sin nombre)" cuando no
/// se capturó destino (family 0) o es la dirección comodín (UDP no-connected).
pub fn flow_host_label(k: &FlowKey) -> String {
    match flow_ip(k) {
        Some(ip) => ip.to_string(),
        None => "(sin nombre)".to_string(),
    }
}

/// Etiqueta del remoto usando el resolver; si no hay nombre, cae a la IP cruda.
pub fn host_label_with(k: &FlowKey, resolver: &dyn HostResolver) -> String {
    flow_ip(k)
        .and_then(|ip| resolver.lookup(ip))
        .unwrap_or_else(|| flow_host_label(k))
}

/// Colapsa los flujos por-(tgid,ip) en deltas por (app, host). Función pura.
/// `resolve_host` se inyecta para permitir el resolver real (Plan 2) o IP cruda.
pub fn aggregate_by_app_remote(
    drained: &[(FlowKey, TrafficVal)],
    resolve_app: &mut impl FnMut(u32) -> String,
    resolve_host: &mut impl FnMut(&FlowKey) -> String,
) -> Vec<AppRemoteDelta> {
    let mut acc: HashMap<(String, String), (u64, u64)> = HashMap::new();
    for (k, v) in drained {
        let e = acc
            .entry((resolve_app(k.tgid), resolve_host(k)))
            .or_default();
        e.0 += v.rx;
        e.1 += v.tx;
    }
    acc.into_iter()
        .map(|((app, host), (rx, tx))| AppRemoteDelta { app, host, rx, tx })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn k4(tgid: u32, ip: [u8; 4]) -> FlowKey {
        let mut k = FlowKey {
            tgid,
            addr: [0; 16],
            family: 4,
            _pad: [0; 3],
        };
        k.addr[0..4].copy_from_slice(&ip);
        k
    }

    #[test]
    fn host_label_v4_v6_y_sin_nombre() {
        assert_eq!(flow_host_label(&k4(1, [1, 2, 3, 4])), "1.2.3.4");
        let k0 = FlowKey {
            tgid: 1,
            addr: [0; 16],
            family: 0,
            _pad: [0; 3],
        };
        assert_eq!(flow_host_label(&k0), "(sin nombre)");
        assert_eq!(flow_host_label(&k4(1, [0, 0, 0, 0])), "(sin nombre)"); // UDP no-conn
        let mut k6 = FlowKey {
            tgid: 1,
            addr: [0; 16],
            family: 6,
            _pad: [0; 3],
        };
        k6.addr = [0x20, 0x01, 0x0d, 0xb8, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1];
        assert_eq!(flow_host_label(&k6), "2001:db8::1");
    }

    #[test]
    fn agrega_por_app_y_host() {
        let drained = vec![
            (k4(100, [1, 1, 1, 1]), TrafficVal { rx: 10, tx: 1 }),
            (k4(101, [1, 1, 1, 1]), TrafficVal { rx: 5, tx: 0 }),
            (k4(100, [8, 8, 8, 8]), TrafficVal { rx: 2, tx: 2 }),
        ];
        let mut rapp = |tgid: u32| {
            if (100..=101).contains(&tgid) {
                "firefox".to_string()
            } else {
                "x".into()
            }
        };
        let mut rhost = |k: &FlowKey| flow_host_label(k);
        let mut out = aggregate_by_app_remote(&drained, &mut rapp, &mut rhost);
        out.sort_by(|a, b| {
            (a.app.as_str(), a.host.as_str()).cmp(&(b.app.as_str(), b.host.as_str()))
        });
        assert_eq!(out.len(), 2);
        assert_eq!(
            (out[0].host.as_str(), out[0].rx, out[0].tx),
            ("1.1.1.1", 15, 1)
        );
        assert_eq!(
            (out[1].host.as_str(), out[1].rx, out[1].tx),
            ("8.8.8.8", 2, 2)
        );
    }

    #[test]
    fn host_label_with_usa_resolver_y_cae_a_ip() {
        use crate::resolve::HostResolver;
        struct FakeResolver;
        impl HostResolver for FakeResolver {
            fn lookup(&self, ip: std::net::IpAddr) -> Option<String> {
                if ip.to_string() == "1.2.3.4" {
                    Some("ejemplo.com".into())
                } else {
                    None
                }
            }
        }
        let conocido = k4(1, [1, 2, 3, 4]);
        let desconocido = k4(1, [9, 9, 9, 9]);
        assert_eq!(host_label_with(&conocido, &FakeResolver), "ejemplo.com");
        assert_eq!(host_label_with(&desconocido, &FakeResolver), "9.9.9.9"); // cae a IP cruda
        let sin = FlowKey {
            tgid: 1,
            addr: [0; 16],
            family: 0,
            _pad: [0; 3],
        };
        assert_eq!(host_label_with(&sin, &FakeResolver), "(sin nombre)");
    }

    #[test]
    fn agrega_tgids_de_la_misma_app() {
        let f = |tgid: u32| FlowKey {
            tgid,
            addr: [0; 16],
            family: 0,
            _pad: [0; 3],
        };
        let drained = vec![
            (f(100), TrafficVal { rx: 10, tx: 1 }),
            (f(101), TrafficVal { rx: 20, tx: 2 }), // otro proceso de firefox
            (f(200), TrafficVal { rx: 5, tx: 5 }),
        ];
        let mut resolver = |tgid: u32| -> String {
            match tgid {
                100 | 101 => "firefox".into(),
                _ => "curl".into(),
            }
        };
        let mut apps = aggregate_by_app(&drained, &mut resolver);
        apps.sort_by(|a, b| a.app.cmp(&b.app));
        assert_eq!(apps.len(), 2);
        assert_eq!(
            (apps[0].app.as_str(), apps[0].rx, apps[0].tx),
            ("curl", 5, 5)
        );
        assert_eq!(
            (apps[1].app.as_str(), apps[1].rx, apps[1].tx),
            ("firefox", 30, 3)
        );
    }
}
