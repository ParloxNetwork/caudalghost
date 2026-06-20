//! Enriquecimiento DNS: resuelve IP remoto → hostname suscribiéndose al monitor
//! de systemd-resolved. En tiempo de captura, nunca reverse-DNS. Sin resolved o
//! sin privilegio, queda inerte y el llamador cae a la IP cruda.

use std::collections::HashMap;
use std::io::{Read, Write};
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};
use std::os::unix::net::UnixStream;
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

const MONITOR_SOCKET: &str = "/run/systemd/resolve/io.systemd.Resolve.Monitor";
/// TTL de las entradas de caché. El monitor no entrega TTL por respuesta; usamos
/// uno fijo prudente — las apps re-resuelven y refrescan. Footprint acotado.
const CACHE_TTL_SECS: i64 = 600;
const CACHE_CAP: usize = 8_192;
/// Longitud máxima de un FQDN (RFC 1035). Un hostname del monitor más largo (o
/// con caracteres de control) es ilegítimo: se descarta antes de cachearlo y
/// pintarlo, para no inflar la caché ni inyectar control en la UI.
const MAX_HOSTNAME_LEN: usize = 253;
/// Tope defensivo del buffer de recepción del monitor varlink: si llega ese
/// volumen sin un delimitador NUL, el monitor se considera averiado y el resolver
/// queda inerte (evita crecimiento de memoria sin límite en el daemon).
const MAX_MONITOR_BUF: usize = 1024 * 1024;

/// ¿Es un hostname aceptable para cachear? No vacío, ≤253 chars y sin controles.
fn is_acceptable_hostname(name: &str) -> bool {
    !name.is_empty() && name.len() <= MAX_HOSTNAME_LEN && !name.chars().any(|c| c.is_control())
}

/// Resuelve un IP remoto a hostname.
pub trait HostResolver: Send + Sync {
    fn lookup(&self, ip: IpAddr) -> Option<String>;
}

/// Resolver inerte: siempre None (degradación honesta).
pub struct NullResolver;
impl HostResolver for NullResolver {
    fn lookup(&self, _ip: IpAddr) -> Option<String> {
        None
    }
}

/// Caché IP→(nombre, expiry epoch). Tope simple por inserción: al exceder `cap`
/// se limpia la mitad más antigua por expiry (evita crecimiento ilimitado).
pub struct HostCache {
    map: HashMap<IpAddr, (String, i64)>,
    cap: usize,
}

impl HostCache {
    pub fn new(cap: usize) -> Self {
        Self {
            map: HashMap::new(),
            cap: cap.max(1),
        }
    }

    pub fn len(&self) -> usize {
        self.map.len()
    }

    pub fn is_empty(&self) -> bool {
        self.map.is_empty()
    }

    /// Nombre vigente para `ip` si su expiry es estrictamente posterior a `now`.
    pub fn lookup(&self, ip: IpAddr, now: i64) -> Option<String> {
        self.map
            .get(&ip)
            .filter(|(_, exp)| *exp > now)
            .map(|(name, _)| name.clone())
    }

    /// Inserta/actualiza; si excede el tope, poda la mitad de menor expiry.
    pub fn insert(&mut self, ip: IpAddr, name: String, expiry: i64) {
        if self.map.len() >= self.cap && !self.map.contains_key(&ip) {
            let mut entries: Vec<(IpAddr, i64)> =
                self.map.iter().map(|(k, (_, e))| (*k, *e)).collect();
            entries.sort_by_key(|(_, e)| *e);
            for (k, _) in entries.into_iter().take(self.map.len() / 2 + 1) {
                self.map.remove(&k);
            }
        }
        self.map.insert(ip, (name, expiry));
    }
}

fn now_epoch() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// Extrae pares (IP, hostname) de los `parameters` de un mensaje del monitor.
/// Solo registros A (1) y AAAA (28); PTR y demás se ignoran (jamás reverse-DNS).
/// Atribuye cada IP al nombre de la pregunta (lo que la app pidió), o al nombre
/// del propio registro si no hay pregunta. Tolerante: omite lo que no encaje.
pub fn parse_monitor_answer(params: &serde_json::Value) -> Vec<(IpAddr, String)> {
    fn strip_dot(s: &str) -> String {
        s.strip_suffix('.').unwrap_or(s).to_string()
    }
    let q_name = params
        .get("question")
        .and_then(|q| q.get(0))
        .and_then(|k| k.get("name"))
        .and_then(|n| n.as_str())
        .map(strip_dot);

    let mut out = Vec::new();
    let Some(answers) = params.get("answer").and_then(|a| a.as_array()) else {
        return out;
    };
    for ans in answers {
        let Some(rr) = ans.get("rr") else { continue };
        let key = rr.get("key");
        let Some(rtype) = key.and_then(|k| k.get("type")).and_then(|t| t.as_i64()) else {
            continue;
        };
        let Some(addr) = rr.get("address").and_then(|a| a.as_array()) else {
            continue;
        };
        let bytes: Vec<u8> = addr
            .iter()
            .filter_map(|n| n.as_u64())
            .map(|n| n as u8)
            .collect();
        let ip = match rtype {
            1 if bytes.len() == 4 => {
                IpAddr::V4(Ipv4Addr::new(bytes[0], bytes[1], bytes[2], bytes[3]))
            }
            28 if bytes.len() == 16 => {
                let mut o = [0u8; 16];
                o.copy_from_slice(&bytes);
                IpAddr::V6(Ipv6Addr::from(o))
            }
            _ => continue, // PTR, CNAME, longitud inválida, etc.
        };
        let name = q_name.clone().unwrap_or_else(|| {
            key.and_then(|k| k.get("name"))
                .and_then(|n| n.as_str())
                .map(strip_dot)
                .unwrap_or_default()
        });
        if is_acceptable_hostname(&name) {
            out.push((ip, name));
        }
    }
    out
}

/// Resolver respaldado por la caché que llena el hilo del monitor.
pub struct ResolvedResolver {
    cache: Arc<Mutex<HostCache>>,
}

impl ResolvedResolver {
    /// Crea el resolver y lanza el hilo del monitor. Si resolved no está o falla
    /// el privilegio, el hilo termina y la caché queda vacía (→ IP cruda).
    pub fn start() -> Self {
        let cache = Arc::new(Mutex::new(HostCache::new(CACHE_CAP)));
        let c = cache.clone();
        std::thread::spawn(move || monitor_loop(c));
        Self { cache }
    }
}

impl HostResolver for ResolvedResolver {
    fn lookup(&self, ip: IpAddr) -> Option<String> {
        self.cache.lock().ok()?.lookup(ip, now_epoch())
    }
}

/// Conecta al socket varlink del monitor, se suscribe y vuelca cada respuesta a
/// la caché. Bloqueante; vive en su propio hilo. Cualquier error lo termina.
fn monitor_loop(cache: Arc<Mutex<HostCache>>) {
    let mut stream = match UnixStream::connect(MONITOR_SOCKET) {
        Ok(s) => s,
        Err(e) => {
            eprintln!(
                "caudalghostd: resolver inerte (sin monitor de systemd-resolved: {e}); IPs crudas"
            );
            return;
        }
    };
    let req = br#"{"method":"io.systemd.Resolve.Monitor.SubscribeQueryResults","parameters":{},"more":true}"#;
    if stream
        .write_all(req)
        .and_then(|_| stream.write_all(&[0]))
        .is_err()
    {
        eprintln!("caudalghostd: resolver inerte (no pude suscribirme al monitor); IPs crudas");
        return;
    }
    let mut buf: Vec<u8> = Vec::with_capacity(8192);
    let mut chunk = [0u8; 4096];
    loop {
        let n = match stream.read(&mut chunk) {
            Ok(0) => break, // EOF
            Ok(n) => n,
            Err(_) => break,
        };
        buf.extend_from_slice(&chunk[..n]);
        if buf.len() > MAX_MONITOR_BUF {
            eprintln!(
                "caudalghostd: marco del monitor de resolved excede {MAX_MONITOR_BUF} bytes sin NUL; resolver inerte"
            );
            return;
        }
        // Mensajes varlink: JSON terminados en NUL.
        while let Some(pos) = buf.iter().position(|&b| b == 0) {
            let msg: Vec<u8> = buf.drain(..=pos).collect();
            let json = &msg[..msg.len() - 1]; // sin el NUL
            let Ok(val) = serde_json::from_slice::<serde_json::Value>(json) else {
                continue;
            };
            if let Some(err) = val.get("error").and_then(|e| e.as_str()) {
                eprintln!(
                    "caudalghostd: monitor de resolved devolvió error ({err}); resolver inerte"
                );
                return;
            }
            let Some(params) = val.get("parameters") else {
                continue;
            };
            let pairs = parse_monitor_answer(params);
            if pairs.is_empty() {
                continue;
            }
            let exp = now_epoch() + CACHE_TTL_SECS;
            if let Ok(mut c) = cache.lock() {
                for (ip, name) in pairs {
                    c.insert(ip, name, exp);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ip(a: u8) -> IpAddr {
        IpAddr::V4(Ipv4Addr::new(a, a, a, a))
    }

    #[test]
    fn cache_respeta_expiry() {
        let mut c = HostCache::new(8);
        c.insert(ip(1), "a.com".into(), 100);
        assert_eq!(c.lookup(ip(1), 50), Some("a.com".to_string())); // vivo
        assert_eq!(c.lookup(ip(1), 100), None); // expirado (expiry no es > now)
        assert_eq!(c.lookup(ip(2), 50), None); // ausente
    }

    #[test]
    fn cache_evita_crecer_sin_limite() {
        let mut c = HostCache::new(2);
        c.insert(ip(1), "a".into(), 1000);
        c.insert(ip(2), "b".into(), 1000);
        c.insert(ip(3), "c".into(), 1000); // excede cap 2 → evicción
        assert!(c.len() <= 2);
        assert_eq!(c.lookup(ip(3), 0), Some("c".to_string())); // el más nuevo queda
    }

    #[test]
    fn null_resolver_siempre_none() {
        assert_eq!(NullResolver.lookup(ip(9)), None);
    }

    #[test]
    fn parsea_a_y_aaaa_usando_el_nombre_de_la_pregunta() {
        let v: serde_json::Value = serde_json::from_str(
            r#"{
          "state":"success",
          "question":[{"class":1,"type":1,"name":"example.com"}],
          "answer":[
            {"rr":{"key":{"class":1,"type":1,"name":"example.com."},"address":[93,184,215,14]},"raw":"x"},
            {"rr":{"key":{"class":1,"type":28,"name":"example.com."},"address":[38,6,40,0,0,0,0,0,0,0,0,0,0,0,0,1]},"raw":"y"},
            {"rr":{"key":{"class":1,"type":12,"name":"14.215.184.93.in-addr.arpa"},"address":null},"raw":"z"}
          ]
        }"#,
        )
        .unwrap();
        let mut out = parse_monitor_answer(&v);
        out.sort_by_key(|p| p.0);
        // El PTR (type 12) se ignora; A y AAAA quedan, atribuidos al nombre de la pregunta.
        assert_eq!(out.len(), 2);
        assert!(out.iter().all(|(_, name)| name == "example.com"));
        assert!(out.iter().any(|(ip, _)| ip.to_string() == "93.184.215.14"));
        assert!(out.iter().any(|(ip, _)| ip.to_string() == "2606:2800::1"));
    }

    #[test]
    fn hostname_aceptable_rechaza_vacio_largo_y_control() {
        assert!(is_acceptable_hostname("example.com"));
        assert!(!is_acceptable_hostname(""));
        assert!(is_acceptable_hostname(&"a".repeat(MAX_HOSTNAME_LEN)));
        assert!(!is_acceptable_hostname(&"a".repeat(MAX_HOSTNAME_LEN + 1)));
        assert!(!is_acceptable_hostname("evil\nhost"));
        assert!(!is_acceptable_hostname("x\x1by"));
    }

    #[test]
    fn parse_descarta_hostname_larguisimo() {
        let largo = "a".repeat(10_000);
        let json = format!(
            r#"{{"question":[{{"class":1,"type":1,"name":"{largo}"}}],
                 "answer":[{{"rr":{{"key":{{"class":1,"type":1,"name":"{largo}."}},"address":[1,2,3,4]}},"raw":"x"}}]}}"#
        );
        let v: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert!(
            parse_monitor_answer(&v).is_empty(),
            "un hostname descomunal debe descartarse"
        );
    }

    #[test]
    fn parser_tolera_answer_vacio_o_ausente() {
        let v: serde_json::Value = serde_json::from_str(r#"{"ready":true}"#).unwrap();
        assert!(parse_monitor_answer(&v).is_empty());
        let v2: serde_json::Value =
            serde_json::from_str(r#"{"state":"success","answer":[]}"#).unwrap();
        assert!(parse_monitor_answer(&v2).is_empty());
    }
}
