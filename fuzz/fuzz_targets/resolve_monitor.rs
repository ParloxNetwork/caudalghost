#![no_main]
//! Fuzz de `parse_monitor_answer` — el parser de respuestas del monitor varlink
//! de systemd-resolved. Su entrada deriva de registros DNS, así que es una
//! frontera **influida por la red** (un servidor DNS hostil o un atacante on-path
//! sin DNSSEC/DoT puede traer bytes arbitrarios). Invariante: nunca paniquear
//! (índice de `address`, overflow, tipos cruzados). Correr:
//! `cargo +nightly fuzz run resolve_monitor`.
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    if let Ok(s) = std::str::from_utf8(data) {
        if let Ok(v) = serde_json::from_str::<serde_json::Value>(s) {
            let _ = caudalghostd::resolve::parse_monitor_answer(&v);
        }
    }
});
