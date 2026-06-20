//! Cliente del socket: re-exportado desde caudalghost-core para que CLI/TUI y
//! la GUI compartan exactamente la misma implementación. Las llamadas
//! `crate::client::call(...)` de `main.rs` y `live/` siguen funcionando igual.
pub use caudalghost_core::client::call;
