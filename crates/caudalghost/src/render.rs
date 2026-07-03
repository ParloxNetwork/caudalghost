use caudalghost_core::proto::{AppRow, Bucket, RemoteRow, SeriePoint, TotalRow};
use chrono::{Local, TimeZone};
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

/// Fila de presentación para `serie`: un punto de la serie temporal con la
/// etiqueta de tiempo ya formateada según el cubo. Serializa CRUDO (ts/rx/tx)
/// para json/ndjson (apto para jq); la etiqueta es solo para table/csv.
#[derive(Serialize)]
pub struct SerieRow {
    pub ts: i64,
    pub rx: u64,
    pub tx: u64,
    #[serde(skip)]
    etiqueta: String,
}

impl SerieRow {
    /// Formatea `ts` (epoch UTC) a hora LOCAL según el cubo.
    pub fn desde(p: &SeriePoint, bucket: Bucket) -> SerieRow {
        let fmt = match bucket {
            Bucket::Minute | Bucket::Hour => "%m-%d %H:%M",
            Bucket::Day => "%Y-%m-%d",
        };
        let etiqueta = Local
            .timestamp_opt(p.ts, 0)
            .single()
            .map(|dt| dt.format(fmt).to_string())
            .unwrap_or_else(|| p.ts.to_string());
        SerieRow {
            ts: p.ts,
            rx: p.rx,
            tx: p.tx,
            etiqueta,
        }
    }
}

impl Tabulable for SerieRow {
    fn cabecera() -> Vec<&'static str> {
        vec!["tiempo", "rx", "tx", "total"]
    }
    fn campos(&self, u: UnitMode) -> Vec<String> {
        vec![
            self.etiqueta.clone(),
            format_units(self.rx, u),
            format_units(self.tx, u),
            format_units(self.rx + self.tx, u),
        ]
    }
}

impl ConRxTx for SerieRow {
    fn rx_tx(&self) -> (u64, u64) {
        (self.rx, self.tx)
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

/// Valor CRUDO (numérico/string) de una columna por su nombre de cabecera,
/// para la proyección `--fields` en json/ndjson (los números quedan números).
pub trait Proyectable {
    fn valor_crudo(&self, col: &str) -> Option<serde_json::Value>;
}

impl Proyectable for TotalRow {
    fn valor_crudo(&self, col: &str) -> Option<serde_json::Value> {
        match col {
            // El nombre de columna debe coincidir con `Tabulable::cabecera()` ("interfaz"),
            // porque `--fields` se valida contra la cabecera: si aquí dijera "iface", el
            // campo pasaría la validación pero se dropearía en silencio en json/ndjson.
            "interfaz" => Some(serde_json::json!(self.iface)),
            "rx" => Some(serde_json::json!(self.rx)),
            "tx" => Some(serde_json::json!(self.tx)),
            "total" => Some(serde_json::json!(self.rx + self.tx)),
            _ => None,
        }
    }
}

impl Proyectable for AppRow {
    fn valor_crudo(&self, col: &str) -> Option<serde_json::Value> {
        match col {
            "app" => Some(serde_json::json!(self.app)),
            "rx" => Some(serde_json::json!(self.rx)),
            "tx" => Some(serde_json::json!(self.tx)),
            "total" => Some(serde_json::json!(self.rx + self.tx)),
            _ => None,
        }
    }
}

impl Proyectable for RemoteRow {
    fn valor_crudo(&self, col: &str) -> Option<serde_json::Value> {
        match col {
            "host" => Some(serde_json::json!(self.host)),
            "rx" => Some(serde_json::json!(self.rx)),
            "tx" => Some(serde_json::json!(self.tx)),
            "total" => Some(serde_json::json!(self.rx + self.tx)),
            _ => None,
        }
    }
}

impl Proyectable for SerieRow {
    fn valor_crudo(&self, col: &str) -> Option<serde_json::Value> {
        match col {
            "tiempo" => Some(serde_json::json!(self.ts)),
            "rx" => Some(serde_json::json!(self.rx)),
            "tx" => Some(serde_json::json!(self.tx)),
            "total" => Some(serde_json::json!(self.rx + self.tx)),
            _ => None,
        }
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
/// `limit`, si viene, acota cuántas filas se emiten en **todos** los formatos
/// (las primeras `limit`, tras el orden ya aplicado por el llamante): la tabla
/// muestra solo esas, y JSON/CSV/NDJSON serializan solo esas. La única
/// excepción es la fila `Σ total` de la tabla, que siempre suma TODAS las filas
/// recibidas en `filas`, no solo las mostradas, para que no sea un subtotal
/// disfrazado de total.
///
/// `fields`, si viene, proyecta las columnas de `csv`/`json`/`ndjson` a solo
/// las pedidas (en el orden pedido); `table` lo ignora (nunca se llama con
/// `fields=Some` para tabla). Con `fields=None` la salida es idéntica a la de
/// siempre (cero cambio de contrato cuando no se pide proyección).
// Punto único de salida: agrupa formato/unidades/quiet/color/título/limit/fields
// deliberadamente en una sola función; empaquetarlos en un struct no aporta claridad.
#[allow(clippy::too_many_arguments)]
pub fn emitir<T: Tabulable + ConRxTx + Serialize + Proyectable>(
    filas: &[T],
    fmt: Formato,
    units: UnitMode,
    quiet: bool,
    color: bool,
    titulo_txt: &str,
    limit: Option<usize>,
    fields: Option<&[String]>,
) -> String {
    // `limit` acota las filas mostradas/emitidas en TODOS los formatos. En Table la fila
    // `Σ total` sigue sumando TODAS (total real); los formatos máquina emiten solo las visibles.
    let n_mostrar = limit.map(|n| n.min(filas.len())).unwrap_or(filas.len());
    let visibles = &filas[..n_mostrar];
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
        Formato::Json => {
            let filas_json: serde_json::Value = match fields {
                Some(cols) => visibles
                    .iter()
                    .map(|f| proyecta_obj(f, cols))
                    .collect::<Vec<_>>()
                    .into(),
                None => serde_json::json!(visibles),
            };
            serde_json::to_string(&serde_json::json!({
                "schema": 1,
                "titulo": titulo_txt,
                "filas": filas_json,
            }))
            .unwrap_or_default()
        }
        Formato::Csv => {
            let cab = T::cabecera();
            let idxs: Vec<usize> = match fields {
                Some(cols) => cols
                    .iter()
                    .filter_map(|c| cab.iter().position(|h| h == c))
                    .collect(),
                None => (0..cab.len()).collect(),
            };
            let mut out = String::new();
            out.push_str(
                &idxs
                    .iter()
                    .map(|&i| csv_campo(cab[i]))
                    .collect::<Vec<_>>()
                    .join(","),
            );
            for fila in visibles {
                let campos = fila.campos(units);
                out.push('\n');
                out.push_str(
                    &idxs
                        .iter()
                        .map(|&i| csv_campo(&campos[i]))
                        .collect::<Vec<_>>()
                        .join(","),
                );
            }
            out
        }
        Formato::Ndjson => match fields {
            Some(cols) => visibles
                .iter()
                .map(|f| serde_json::to_string(&proyecta_obj(f, cols)).unwrap_or_default())
                .collect::<Vec<_>>()
                .join("\n"),
            None => visibles
                .iter()
                .map(|f| serde_json::to_string(f).unwrap_or_default())
                .collect::<Vec<_>>()
                .join("\n"),
        },
    }
}

/// Proyecta una fila a un objeto JSON `{col: valor_crudo(col)}` solo con las
/// columnas pedidas (en el orden pedido); columnas sin valor para el tipo se
/// omiten. Compartido por las ramas `json`/`ndjson` de `emitir` con `fields`.
fn proyecta_obj<T: Proyectable>(fila: &T, cols: &[String]) -> serde_json::Value {
    let mut m = serde_json::Map::new();
    for c in cols {
        if let Some(v) = fila.valor_crudo(c) {
            m.insert(c.clone(), v);
        }
    }
    serde_json::Value::Object(m)
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
            None,
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

    #[test]
    fn limit_acota_tambien_json_csv_ndjson() {
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
        let nd = super::emitir(
            &filas,
            Formato::Ndjson,
            UnitMode::Binary,
            true,
            false,
            "top",
            Some(2),
            None,
        );
        assert_eq!(nd.lines().count(), 2, "ndjson respeta el limit: {nd}");
        assert!(nd.contains("firefox") && nd.contains("curl") && !nd.contains("ssh"));
        let csv = super::emitir(
            &filas,
            Formato::Csv,
            UnitMode::Binary,
            true,
            false,
            "top",
            Some(1),
            None,
        );
        assert_eq!(csv.lines().count(), 2, "csv = cabecera + 1 fila: {csv}"); // header + 1
        assert!(csv.contains("firefox") && !csv.contains("ssh"));
        let json = super::emitir(
            &filas,
            Formato::Json,
            UnitMode::Binary,
            true,
            false,
            "top",
            Some(1),
            None,
        );
        assert!(json.contains("firefox") && !json.contains("curl") && !json.contains("ssh"));
    }

    #[test]
    fn serie_row_cabecera_y_serializa_crudo() {
        use caudalghost_core::proto::{Bucket, SeriePoint};
        let p = SeriePoint {
            ts: 1_720_000_000,
            rx: 100,
            tx: 20,
        };
        let row = super::SerieRow::desde(&p, Bucket::Hour);
        assert_eq!(
            super::SerieRow::cabecera(),
            vec!["tiempo", "rx", "tx", "total"]
        );
        let c = row.campos(UnitMode::Binary);
        assert_eq!(c[0], row_etiqueta(&row)); // etiqueta no vacía
        assert!(!c[0].is_empty());
        assert_eq!(c[3], "120 B"); // total = rx+tx
                                   // json/ndjson: crudo ts/rx/tx, SIN 'etiqueta' ni 'total'
        let j = serde_json::to_string(&row).unwrap();
        assert!(
            j.contains("\"ts\":1720000000") && j.contains("\"rx\":100") && j.contains("\"tx\":20")
        );
        assert!(!j.contains("etiqueta") && !j.contains("total"));
    }
    fn row_etiqueta(r: &super::SerieRow) -> String {
        r.campos(UnitMode::Binary)[0].clone()
    }

    #[test]
    fn serie_row_etiqueta_por_bucket() {
        use caudalghost_core::proto::{Bucket, SeriePoint};
        let p = SeriePoint {
            ts: 1_720_000_000,
            rx: 1,
            tx: 1,
        };
        let dia = super::SerieRow::desde(&p, Bucket::Day).campos(UnitMode::Binary)[0].clone();
        // Day => "YYYY-MM-DD" (10 chars, dos guiones)
        assert_eq!(dia.len(), 10, "etiqueta día: {dia}");
        assert_eq!(dia.matches('-').count(), 2);
        let hora = super::SerieRow::desde(&p, Bucket::Hour).campos(UnitMode::Binary)[0].clone();
        assert!(hora.contains(':'), "etiqueta hora lleva HH:MM: {hora}");
    }

    #[test]
    fn proyectable_valor_crudo_por_tipo() {
        use caudalghost_core::proto::AppRow;
        let r = AppRow {
            app: "firefox".into(),
            rx: 100,
            tx: 20,
        };
        assert_eq!(
            super::Proyectable::valor_crudo(&r, "app"),
            Some(serde_json::json!("firefox"))
        );
        assert_eq!(
            super::Proyectable::valor_crudo(&r, "total"),
            Some(serde_json::json!(120))
        );
        assert_eq!(super::Proyectable::valor_crudo(&r, "nope"), None);
    }

    #[test]
    fn emitir_fields_none_identico_a_hoy() {
        use caudalghost_core::proto::AppRow;
        let filas = vec![AppRow {
            app: "a".into(),
            rx: 1,
            tx: 2,
        }];
        let antes = super::emitir(
            &filas,
            Formato::Ndjson,
            UnitMode::Binary,
            true,
            false,
            "t",
            None,
            None,
        );
        assert_eq!(antes, "{\"app\":\"a\",\"rx\":1,\"tx\":2}");
    }

    #[test]
    fn emitir_fields_proyecta_csv_en_orden() {
        use caudalghost_core::proto::AppRow;
        let filas = vec![AppRow {
            app: "a".into(),
            rx: 10,
            tx: 5,
        }];
        let cols = vec!["total".to_string(), "app".to_string()];
        let csv = super::emitir(
            &filas,
            Formato::Csv,
            UnitMode::Binary,
            true,
            false,
            "t",
            None,
            Some(&cols),
        );
        let mut lineas = csv.lines();
        assert_eq!(lineas.next().unwrap(), "total,app"); // orden pedido
        assert_eq!(lineas.next().unwrap(), "15 B,a");
    }

    #[test]
    fn emitir_fields_proyecta_ndjson_crudo() {
        use caudalghost_core::proto::AppRow;
        let filas = vec![AppRow {
            app: "a".into(),
            rx: 10,
            tx: 5,
        }];
        let cols = vec!["app".to_string(), "total".to_string()];
        let nd = super::emitir(
            &filas,
            Formato::Ndjson,
            UnitMode::Binary,
            true,
            false,
            "t",
            None,
            Some(&cols),
        );
        let v: serde_json::Value = serde_json::from_str(&nd).unwrap();
        assert_eq!(v["app"], "a");
        assert_eq!(v["total"], 15); // número crudo, no "15 B"
        assert!(v.get("rx").is_none()); // rx no pedido
    }

    /// Regresión: `TotalRow` se proyecta por su nombre de cabecera real
    /// ("interfaz"), no "iface". Cubre la ruta json/ndjson de `now`/`total`,
    /// que antes dropeaba la interfaz en silencio con `--fields interfaz`.
    #[test]
    fn emitir_fields_totalrow_usa_nombre_de_cabecera() {
        use caudalghost_core::proto::TotalRow;
        assert!(TotalRow::cabecera().contains(&"interfaz"));
        let filas = vec![TotalRow {
            iface: "eth0".into(),
            rx: 100,
            tx: 20,
        }];
        let cols = vec!["interfaz".to_string(), "total".to_string()];
        let nd = super::emitir(
            &filas,
            Formato::Ndjson,
            UnitMode::Binary,
            true,
            false,
            "t",
            None,
            Some(&cols),
        );
        let v: serde_json::Value = serde_json::from_str(&nd).unwrap();
        assert_eq!(v["interfaz"], "eth0", "la interfaz no debe dropearse: {nd}");
        assert_eq!(v["total"], 120);
    }
}
