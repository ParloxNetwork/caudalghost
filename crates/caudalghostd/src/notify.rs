//! Integración opcional con systemd vía el protocolo `sd_notify` (Rust puro,
//! sin enlazar libsystemd). Todo es no-op si el daemon no corre bajo systemd
//! (sin `NOTIFY_SOCKET` en el entorno) → los arranques manuales no se afectan.

use std::time::Duration;

/// ¿El daemon corre bajo systemd con `sd_notify` disponible?
pub fn enabled() -> bool {
    std::env::var_os("NOTIFY_SOCKET").is_some()
}

/// READY=1: la base está abierta, eBPF intentado y el socket ya escucha.
pub fn ready() {
    if enabled() {
        let _ = sd_notify::notify(false, &[sd_notify::NotifyState::Ready]);
    }
}

/// STOPPING=1: apagado ordenado (antes del flush final).
pub fn stopping() {
    if enabled() {
        let _ = sd_notify::notify(false, &[sd_notify::NotifyState::Stopping]);
    }
}

/// WATCHDOG=1: latido de "sigo vivo".
pub fn watchdog_ping() {
    if enabled() {
        let _ = sd_notify::notify(false, &[sd_notify::NotifyState::Watchdog]);
    }
}

/// Mitad del periodo del watchdog (µs → intervalo de latido); None si es 0.
/// Núcleo puro y testeable de `watchdog_ping_interval`.
pub fn watchdog_half(usec: u64) -> Option<Duration> {
    (usec > 0).then(|| Duration::from_micros(usec / 2))
}

/// Intervalo de latido recomendado leyendo el entorno real de systemd
/// (`WATCHDOG_USEC`/`WATCHDOG_PID`), o None si el watchdog no está activado.
pub fn watchdog_ping_interval() -> Option<Duration> {
    let mut usec: u64 = 0;
    if sd_notify::watchdog_enabled(false, &mut usec) {
        watchdog_half(usec)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn watchdog_half_es_la_mitad_y_none_si_cero() {
        assert_eq!(watchdog_half(30_000_000), Some(Duration::from_secs(15)));
        assert_eq!(watchdog_half(2_000_000), Some(Duration::from_secs(1)));
        assert_eq!(watchdog_half(0), None);
    }
}
