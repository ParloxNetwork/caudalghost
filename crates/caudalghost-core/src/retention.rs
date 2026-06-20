//! Retenciones de las capas escalonadas. Compartidas por el storage del daemon
//! (enrutado de consultas) y el cliente (selección de cubo para graficar).
pub const MINUTE_RETENTION_SECS: i64 = 90 * 86_400; // ~90 días
pub const HOUR_RETENTION_SECS: i64 = 730 * 86_400; // ~2 años

/// Retención de la dimensión host (más cara → más corta que iface/app).
/// La capa de día (`day_app_remote`) nunca se poda (histórico indefinido).
pub const REMOTE_MINUTE_RETENTION_SECS: i64 = 7 * 86_400; // ~7 días
pub const REMOTE_HOUR_RETENTION_SECS: i64 = 90 * 86_400; // ~90 días
