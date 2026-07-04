#![no_main]
//! Fuzz de la atribución cgroup→app (`app_from_cgroup` y sus helpers puros).
//! Semi-no-confiable: un proceso local puede disponer su cgroup con bytes
//! arbitrarios. Invariante: funciones puras, nunca paniquean ni se cuelgan.
//! Correr: `cargo +nightly fuzz run appmap`.
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    if let Ok(s) = std::str::from_utf8(data) {
        let _ = caudalghostd::appmap::app_from_cgroup(s);
    }
});
