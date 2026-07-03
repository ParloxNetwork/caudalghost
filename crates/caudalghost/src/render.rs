use caudalghost_core::proto::{AppRow, RemoteRow, TotalRow};
use comfy_table::{presets::UTF8_FULL_CONDENSED, Cell, CellAlignment, Table};
use owo_colors::{OwoColorize, Stream::Stdout};
use serde::Serialize;

const BARS: [char; 8] = ['▁', '▂', '▃', '▄', '▅', '▆', '▇', '█'];

/// Mini-gráfica unicode escalada al máximo de la serie.
pub fn sparkline(values: &[u64]) -> String {
    let max = values.iter().copied().max().unwrap_or(0);
    values
        .iter()
        .map(|&v| {
            let idx = v.saturating_mul(7).checked_div(max).unwrap_or(0) as usize;
            BARS[idx]
        })
        .collect()
}

/// Modo de presentación de tasas: binario (KiB, 1024), decimal (KB, 1000) o bits (Mbps).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, clap::ValueEnum)]
pub enum UnitMode {
    #[default]
    Binary,
    Decimal,
    Bits,
}

impl UnitMode {
    /// Cicla al siguiente modo: Binary → Decimal → Bits → Binary.
    pub fn next(self) -> Self {
        match self {
            UnitMode::Binary => UnitMode::Decimal,
            UnitMode::Decimal => UnitMode::Bits,
            UnitMode::Bits => UnitMode::Binary,
        }
    }
    /// Etiqueta corta para el pie.
    pub fn label(self) -> &'static str {
        match self {
            UnitMode::Binary => "KiB",
            UnitMode::Decimal => "KB",
            UnitMode::Bits => "Mbps",
        }
    }
}

/// Formatea bytes (o bytes/s) según el modo. En Bits multiplica ×8 y usa sufijos de bit.
pub fn format_units(b: u64, mode: UnitMode) -> String {
    let (value, base, units): (u64, f64, [&str; 6]) = match mode {
        UnitMode::Binary => (b, 1024.0, ["B", "KiB", "MiB", "GiB", "TiB", "PiB"]),
        UnitMode::Decimal => (b, 1000.0, ["B", "KB", "MB", "GB", "TB", "PB"]),
        UnitMode::Bits => (
            b.saturating_mul(8),
            1000.0,
            ["b", "Kb", "Mb", "Gb", "Tb", "Pb"],
        ),
    };
    if (value as f64) < base {
        return format!("{value} {}", units[0]);
    }
    let mut v = value as f64;
    let mut u = 0;
    while v >= base && u < units.len() - 1 {
        v /= base;
        u += 1;
    }
    format!("{:.1} {}", v, units[u])
}

/// Formatea bytes en unidades binarias (KiB). Wrapper de compatibilidad,
/// conservado para uso en tests tras enchufar `UnitMode` en todas las tablas.
#[allow(dead_code)]
pub fn format_bytes(b: u64) -> String {
    format_units(b, UnitMode::Binary)
}

/// Formatea una tasa (bytes/s) según el modo: `format_units` + sufijo "/s".
pub fn format_rate(bytes: u64, mode: UnitMode) -> String {
    format!("{}/s", format_units(bytes, mode))
}

/// Formato de salida elegido con `-o/--output` (o el alias `--json`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, clap::ValueEnum)]
pub enum Formato {
    #[default]
    Table,
    Json,
    Csv,
    Ndjson,
}

/// Filas que saben presentarse como cabecera + campos formateados (para
/// tablas/CSV/NDJSON), respetando el `UnitMode` elegido.
pub trait Tabulable {
    fn cabecera() -> Vec<&'static str>;
    fn campos(&self, u: UnitMode) -> Vec<String>;
}

impl Tabulable for TotalRow {
    fn cabecera() -> Vec<&'static str> {
        vec!["interfaz", "rx", "tx", "total"]
    }
    fn campos(&self, u: UnitMode) -> Vec<String> {
        vec![
            self.iface.clone(),
            format_units(self.rx, u),
            format_units(self.tx, u),
            format_units(self.rx + self.tx, u),
        ]
    }
}

impl Tabulable for AppRow {
    fn cabecera() -> Vec<&'static str> {
        vec!["app", "rx", "tx", "total"]
    }
    fn campos(&self, u: UnitMode) -> Vec<String> {
        vec![
            self.app.clone(),
            format_units(self.rx, u),
            format_units(self.tx, u),
            format_units(self.rx + self.tx, u),
        ]
    }
}

impl Tabulable for RemoteRow {
    fn cabecera() -> Vec<&'static str> {
        vec!["host", "rx", "tx", "total"]
    }
    fn campos(&self, u: UnitMode) -> Vec<String> {
        vec![
            self.host.clone(),
            format_units(self.rx, u),
            format_units(self.tx, u),
            format_units(self.rx + self.tx, u),
        ]
    }
}

/// Fila de suma (Σ) para las tablas de totales/top: etiqueta concisa y fija
/// (no deriva del título, que puede ser largo o repetir el símbolo Σ).
/// Mismo layout que `Tabulable::campos` para poder añadirse como fila extra
/// en el render de tabla (comportamiento heredado de `tabla_total/top/hosts`).
/// Suma SIEMPRE el total real de todas las filas recibidas, aunque la tabla
/// solo muestre las primeras `limit` (ver `emitir`).
fn fila_suma(rx: u64, tx: u64, u: UnitMode) -> Vec<String> {
    vec![
        "Σ total".to_string(),
        format_units(rx, u),
        format_units(tx, u),
        format_units(rx + tx, u),
    ]
}

/// Acceso a los pares rx/tx crudos de una fila, para poder acumular la fila
/// de suma en la tabla sin reparsear los campos ya formateados.
pub(crate) trait ConRxTx {
    fn rx_tx(&self) -> (u64, u64);
}

impl ConRxTx for TotalRow {
    fn rx_tx(&self) -> (u64, u64) {
        (self.rx, self.tx)
    }
}
impl ConRxTx for AppRow {
    fn rx_tx(&self) -> (u64, u64) {
        (self.rx, self.tx)
    }
}
impl ConRxTx for RemoteRow {
    fn rx_tx(&self) -> (u64, u64) {
        (self.rx, self.tx)
    }
}

/// Entrecomilla un campo CSV si contiene coma, comilla o salto de línea
/// (RFC 4180 mínimo, sin dependencia externa).
fn csv_campo(s: &str) -> String {
    if s.contains(',') || s.contains('"') || s.contains('\n') {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s.to_string()
    }
}

/// Punto único de salida: despacha por `fmt` (tabla/JSON/CSV/NDJSON),
/// respetando `quiet` (omite título/decoración) y `color`.
///
/// `limit`, si viene, acota cuántas filas se **muestran** en la tabla (las
/// primeras `limit`, tras el orden ya aplicado por el llamante) — pero la
/// fila `Σ total` siempre suma TODAS las filas recibidas en `filas`, no solo
/// las mostradas, para que no sea un subtotal disfrazado de total. `limit`
/// solo afecta al render de tabla; JSON/CSV/NDJSON emiten todas las filas.
pub fn emitir<T: Tabulable + ConRxTx + Serialize>(
    filas: &[T],
    fmt: Formato,
    units: UnitMode,
    quiet: bool,
    color: bool,
    titulo_txt: &str,
    limit: Option<usize>,
) -> String {
    match fmt {
        Formato::Table => {
            let mut t = Table::new();
            t.load_preset(UTF8_FULL_CONDENSED);
            t.set_header(T::cabecera());
            let (mut srx, mut stx) = (0u64, 0u64);
            let mostrar = limit.unwrap_or(filas.len());
            for (i, fila) in filas.iter().enumerate() {
                let (rx, tx) = fila.rx_tx();
                srx += rx;
                stx += tx;
                if i >= mostrar {
                    continue;
                }
                let campos = fila.campos(units);
                let n = campos.len();
                t.add_row(campos.into_iter().enumerate().map(|(i, c)| {
                    let mut cell = Cell::new(c);
                    if i > 0 && i < n {
                        cell = cell.set_alignment(CellAlignment::Right);
                    }
                    cell
                }));
            }
            if !filas.is_empty() {
                let suma = fila_suma(srx, stx, units);
                let n = suma.len();
                t.add_row(suma.into_iter().enumerate().map(|(i, c)| {
                    let mut cell = Cell::new(c);
                    if i > 0 && i < n {
                        cell = cell.set_alignment(CellAlignment::Right);
                    }
                    cell
                }));
            }
            let cuerpo = t.to_string();
            if quiet {
                cuerpo
            } else {
                let cab = if color {
                    titulo(titulo_txt)
                } else {
                    titulo_txt.to_string()
                };
                format!("{cab}\n{cuerpo}")
            }
        }
        Formato::Json => serde_json::to_string(&serde_json::json!({
            "schema": 1,
            "titulo": titulo_txt,
            "filas": filas,
        }))
        .unwrap_or_default(),
        Formato::Csv => {
            let mut out = String::new();
            out.push_str(
                &T::cabecera()
                    .iter()
                    .map(|c| csv_campo(c))
                    .collect::<Vec<_>>()
                    .join(","),
            );
            for fila in filas {
                out.push('\n');
                out.push_str(
                    &fila
                        .campos(units)
                        .iter()
                        .map(|c| csv_campo(c))
                        .collect::<Vec<_>>()
                        .join(","),
                );
            }
            out
        }
        Formato::Ndjson => filas
            .iter()
            .map(|f| serde_json::to_string(f).unwrap_or_default())
            .collect::<Vec<_>>()
            .join("\n"),
    }
}

/// Tabla de actividad en vivo: una fila por interfaz con tasas y sparkline.
/// `filas` = (iface, rx_total, tx_total, serie por segundo rx+tx).
pub fn tabla_now(filas: &[(String, u64, u64, Vec<u64>)], secs: u64, units: UnitMode) -> Table {
    let mut t = Table::new();
    t.load_preset(UTF8_FULL_CONDENSED);
    t.set_header(vec!["interfaz", "↓ media", "↑ media", "actividad"]);
    for (iface, rx, tx, serie) in filas {
        t.add_row(vec![
            Cell::new(iface),
            Cell::new(format_rate(rx / secs.max(1), units)).set_alignment(CellAlignment::Right),
            Cell::new(format_rate(tx / secs.max(1), units)).set_alignment(CellAlignment::Right),
            Cell::new(sparkline(serie)),
        ]);
    }
    t
}

/// Título con color solo si stdout es una terminal (degradación limpia en pipes).
pub fn titulo(s: &str) -> String {
    format!("{}", s.if_supports_color(Stdout, |t| t.bold()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tabla_top_tiene_fila_suma() {
        use caudalghost_core::proto::AppRow;
        let rows = vec![
            AppRow {
                app: "firefox".into(),
                rx: 100,
                tx: 10,
            },
            AppRow {
                app: "curl".into(),
                rx: 50,
                tx: 5,
            },
        ];
        let s = super::emitir(
            &rows,
            Formato::Table,
            UnitMode::Binary,
            false,
            false,
            "hoy",
            None,
        );
        assert!(s.contains("firefox") && s.contains("Σ total"));
    }

    #[test]
    fn tabla_hosts_tiene_fila_suma() {
        use caudalghost_core::proto::RemoteRow;
        let rows = vec![
            RemoteRow {
                host: "github.com".into(),
                rx: 100,
                tx: 10,
            },
            RemoteRow {
                host: "scdn.co".into(),
                rx: 50,
                tx: 5,
            },
        ];
        let s = super::emitir(
            &rows,
            Formato::Table,
            UnitMode::Binary,
            false,
            false,
            "7d",
            None,
        );
        assert!(s.contains("github.com") && s.contains("scdn.co") && s.contains("Σ total"));
    }

    #[test]
    fn format_bytes_unidades() {
        assert_eq!(format_bytes(0), "0 B");
        assert_eq!(format_bytes(999), "999 B");
        assert_eq!(format_bytes(1536), "1.5 KiB");
        assert_eq!(format_bytes(3 * 1024 * 1024), "3.0 MiB");
        assert_eq!(format_bytes(5 * 1024 * 1024 * 1024), "5.0 GiB");
    }

    #[test]
    fn sparkline_escala_al_maximo() {
        assert_eq!(sparkline(&[0, 0, 0]), "▁▁▁");
        assert_eq!(sparkline(&[1, 1, 1]), "███");
        let s = sparkline(&[0, 50, 100]);
        assert_eq!(s.chars().count(), 3);
        assert!(s.starts_with('▁') && s.ends_with('█'));
    }

    #[test]
    fn sparkline_vacia() {
        assert_eq!(sparkline(&[]), "");
    }

    #[test]
    fn format_units_tres_modos() {
        use super::UnitMode;
        // Binario (base 1024)
        assert_eq!(format_units(0, UnitMode::Binary), "0 B");
        assert_eq!(format_units(1000, UnitMode::Binary), "1000 B"); // <1024
        assert_eq!(format_units(1536, UnitMode::Binary), "1.5 KiB");
        // Decimal (base 1000)
        assert_eq!(format_units(1000, UnitMode::Decimal), "1.0 KB");
        assert_eq!(format_units(1500, UnitMode::Decimal), "1.5 KB");
        assert_eq!(format_units(3_000_000, UnitMode::Decimal), "3.0 MB");
        // Bits (×8, base 1000, sufijos b/Kb/Mb)
        assert_eq!(format_units(100, UnitMode::Bits), "800 b"); // 100*8=800 bits
        assert_eq!(format_units(1000, UnitMode::Bits), "8.0 Kb"); // 8000 bits
        assert_eq!(format_units(125_000, UnitMode::Bits), "1.0 Mb"); // 1_000_000 bits
    }

    #[test]
    fn format_bytes_sigue_siendo_binario() {
        assert_eq!(format_bytes(1536), "1.5 KiB");
        assert_eq!(format_bytes(5 * 1024 * 1024 * 1024), "5.0 GiB");
    }

    #[test]
    fn format_rate_añade_sufijo_por_segundo() {
        assert_eq!(format_rate(0, UnitMode::Binary), "0 B/s");
        assert_eq!(format_rate(1536, UnitMode::Binary), "1.5 KiB/s");
        assert_eq!(format_rate(1500, UnitMode::Decimal), "1.5 KB/s");
        assert_eq!(format_rate(125_000, UnitMode::Bits), "1.0 Mb/s");
    }

    #[test]
    fn tabla_respeta_unidades() {
        use caudalghost_core::proto::TotalRow;
        let filas = vec![TotalRow {
            iface: "eth0".into(),
            rx: 1_500_000,
            tx: 0,
        }];
        let bin = super::emitir(
            &filas,
            Formato::Table,
            UnitMode::Binary,
            true,
            false,
            "total",
            None,
        );
        assert!(bin.contains("MiB"));
        let bits = super::emitir(
            &filas,
            Formato::Table,
            UnitMode::Bits,
            true,
            false,
            "total",
            None,
        );
        assert!(bits.contains("Mb") && !bits.contains("MiB"));
    }

    #[test]
    fn tabla_now_respeta_unidades() {
        let filas = vec![("eth0".to_string(), 1_500_000u64, 0u64, vec![1, 2, 3])];
        let bin = super::tabla_now(&filas, 1, UnitMode::Binary).to_string();
        assert!(bin.contains("MiB/s"));
        let bits = super::tabla_now(&filas, 1, UnitMode::Bits).to_string();
        assert!(bits.contains("Mb/s") && !bits.contains("MiB/s"));
    }

    #[test]
    fn emitir_csv_y_ndjson_de_filas() {
        use caudalghost_core::proto::AppRow;
        let filas = vec![
            AppRow {
                app: "firefox".into(),
                rx: 1024,
                tx: 512,
            },
            AppRow {
                app: "curl".into(),
                rx: 5,
                tx: 5,
            },
        ];
        let csv = super::emitir(
            &filas,
            super::Formato::Csv,
            UnitMode::Binary,
            true,
            false,
            "top",
            None,
        );
        assert!(csv.lines().next().unwrap().contains("app")); // cabecera
        assert!(csv.contains("firefox"));
        let nd = super::emitir(
            &filas,
            super::Formato::Ndjson,
            UnitMode::Binary,
            true,
            false,
            "top",
            None,
        );
        assert_eq!(nd.lines().count(), 2); // una fila por línea
        assert!(nd.lines().next().unwrap().starts_with('{'));
    }

    #[test]
    fn tabla_alinea_numeros_a_la_derecha() {
        use caudalghost_core::proto::AppRow;
        let filas = vec![
            AppRow {
                app: "a".into(),
                rx: 1,
                tx: 0,
            },
            AppRow {
                app: "b".into(),
                rx: 1048576,
                tx: 0,
            },
        ];
        let t = super::emitir(
            &filas,
            super::Formato::Table,
            UnitMode::Binary,
            true,
            false,
            "top",
            None,
        );
        // la columna rx queda alineada: el '1 B' de la primera fila lleva padding a la izquierda
        assert!(t.contains("  1 B") || t.contains(" 1 B "));
    }

    /// Con `limit = Some(1)` la tabla muestra solo 1 fila de datos, pero la
    /// fila `Σ total` sigue reflejando la suma de TODAS las filas recibidas
    /// (no un subtotal disfrazado de total).
    #[test]
    fn limit_trunca_solo_lo_mostrado_pero_suma_incluye_todo() {
        use caudalghost_core::proto::AppRow;
        let filas = vec![
            AppRow {
                app: "firefox".into(),
                rx: 100,
                tx: 10,
            },
            AppRow {
                app: "curl".into(),
                rx: 50,
                tx: 5,
            },
            AppRow {
                app: "ssh".into(),
                rx: 20,
                tx: 2,
            },
        ];
        let s = super::emitir(
            &filas,
            Formato::Table,
            UnitMode::Binary,
            true,
            false,
            "top",
            Some(1),
        );
        assert!(s.contains("firefox"), "debe mostrar la primera fila");
        assert!(!s.contains("curl"), "no debe mostrar filas fuera del limit");
        assert!(!s.contains("ssh"), "no debe mostrar filas fuera del limit");
        // suma real: rx 100+50+20=170, tx 10+5+2=17 → total 187 B
        assert!(
            s.contains("Σ total") && s.contains("187 B"),
            "la fila Σ total debe sumar TODAS las filas, no solo las mostradas: {s}"
        );
    }
}
