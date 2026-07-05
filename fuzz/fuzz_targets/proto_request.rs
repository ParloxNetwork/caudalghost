#![no_main]
//! Fuzz del parser del protocolo IPC — el límite exacto de input NO confiable
//! (una línea JSON del socket). Invariante: nunca debe paniquear; a lo sumo
//! devuelve error de serde. Correr: `cargo +nightly fuzz run proto_request`.
use caudalghost_core::proto::{Request, Response};
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    if let Ok(s) = std::str::from_utf8(data) {
        let _ = serde_json::from_str::<Request>(s);
        let _ = serde_json::from_str::<Response>(s);
    }
});
