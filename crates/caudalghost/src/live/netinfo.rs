//! Lectura de IPs locales por interfaz (la CLI las obtiene del sistema, no del daemon).

use std::collections::HashMap;

/// Mapa nombre-de-interfaz → IPv4 principal (la primera encontrada).
/// Las interfaces sin IPv4 simplemente no aparecen en el mapa.
pub fn read_iface_ipv4s() -> HashMap<String, String> {
    let mut map = HashMap::new();
    if let Ok(ifaces) = if_addrs::get_if_addrs() {
        for iface in ifaces {
            if let if_addrs::IfAddr::V4(v4) = iface.addr {
                map.entry(iface.name).or_insert_with(|| v4.ip.to_string());
            }
        }
    }
    map
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lee_loopback_127() {
        // En Linux, lo siempre tiene 127.0.0.1
        let map = read_iface_ipv4s();
        assert_eq!(map.get("lo").map(String::as_str), Some("127.0.0.1"));
    }
}
