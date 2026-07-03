use caudalghost_core::proto::{AppRow, RemoteRow, TotalRow};
use comfy_table::{presets::UTF8_FULL_CONDENSED, Cell, Table};
use owo_colors::{OwoColorize, Stream::Stdout};

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
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
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

/// Formatea bytes en unidades binarias (KiB). Wrapper de compatibilidad para la CLI estática.
pub fn format_bytes(b: u64) -> String {
    format_units(b, UnitMode::Binary)
}

/// Formatea una tasa (bytes/s) según el modo: `format_units` + sufijo "/s".
pub fn format_rate(bytes: u64, mode: UnitMode) -> String {
    format!("{}/s", format_units(bytes, mode))
}

/// Tabla de actividad en vivo: una fila por interfaz con tasas y sparkline.
/// `filas` = (iface, rx_total, tx_total, serie por segundo rx+tx).
pub fn tabla_now(filas: &[(String, u64, u64, Vec<u64>)], secs: u64) -> Table {
    let mut t = Table::new();
    t.load_preset(UTF8_FULL_CONDENSED);
    t.set_header(vec!["interfaz", "↓ media", "↑ media", "actividad"]);
    for (iface, rx, tx, serie) in filas {
        t.add_row(vec![
            Cell::new(iface),
            Cell::new(format!("{}/s", format_bytes(rx / secs.max(1)))),
            Cell::new(format!("{}/s", format_bytes(tx / secs.max(1)))),
            Cell::new(sparkline(serie)),
        ]);
    }
    t
}

/// Tabla de totales por rango, con fila de suma.
pub fn tabla_total(rows: &[TotalRow], rango: &str) -> Table {
    let mut t = Table::new();
    t.load_preset(UTF8_FULL_CONDENSED);
    t.set_header(vec!["interfaz", "↓ recibido", "↑ enviado", "total"]);
    let (mut srx, mut stx) = (0u64, 0u64);
    for r in rows {
        srx += r.rx;
        stx += r.tx;
        t.add_row(vec![
            Cell::new(&r.iface),
            Cell::new(format_bytes(r.rx)),
            Cell::new(format_bytes(r.tx)),
            Cell::new(format_bytes(r.rx + r.tx)),
        ]);
    }
    t.add_row(vec![
        Cell::new(format!("Σ {rango}")),
        Cell::new(format_bytes(srx)),
        Cell::new(format_bytes(stx)),
        Cell::new(format_bytes(srx + stx)),
    ]);
    t
}

/// Tabla de top por aplicación, ordenada por total desc, con fila de suma.
pub fn tabla_top(rows: &[AppRow], rango: &str) -> Table {
    let mut t = Table::new();
    t.load_preset(UTF8_FULL_CONDENSED);
    t.set_header(vec!["aplicación", "↓ recibido", "↑ enviado", "total"]);
    let mut ordenadas: Vec<&AppRow> = rows.iter().collect();
    ordenadas.sort_by_key(|r| std::cmp::Reverse(r.rx + r.tx));
    let (mut srx, mut stx) = (0u64, 0u64);
    for r in ordenadas {
        srx += r.rx;
        stx += r.tx;
        t.add_row(vec![
            Cell::new(&r.app),
            Cell::new(format_bytes(r.rx)),
            Cell::new(format_bytes(r.tx)),
            Cell::new(format_bytes(r.rx + r.tx)),
        ]);
    }
    t.add_row(vec![
        Cell::new(format!("Σ {rango}")),
        Cell::new(format_bytes(srx)),
        Cell::new(format_bytes(stx)),
        Cell::new(format_bytes(srx + stx)),
    ]);
    t
}

/// Tabla del top de hosts remotos (dominios registrables o IPs) con su tráfico.
pub fn tabla_hosts(rows: &[RemoteRow], rango: &str) -> Table {
    let mut t = Table::new();
    t.load_preset(UTF8_FULL_CONDENSED);
    t.set_header(vec!["host", "↓ recibido", "↑ enviado", "total"]);
    let mut ordenados: Vec<&RemoteRow> = rows.iter().collect();
    ordenados.sort_by_key(|r| std::cmp::Reverse(r.rx + r.tx));
    let (mut srx, mut stx) = (0u64, 0u64);
    for r in ordenados {
        srx += r.rx;
        stx += r.tx;
        t.add_row(vec![
            Cell::new(&r.host),
            Cell::new(format_bytes(r.rx)),
            Cell::new(format_bytes(r.tx)),
            Cell::new(format_bytes(r.rx + r.tx)),
        ]);
    }
    t.add_row(vec![
        Cell::new(format!("Σ {rango}")),
        Cell::new(format_bytes(srx)),
        Cell::new(format_bytes(stx)),
        Cell::new(format_bytes(srx + stx)),
    ]);
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
        let t = tabla_top(&rows, "hoy");
        let s = t.to_string();
        assert!(s.contains("firefox") && s.contains("Σ hoy"));
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
        let t = tabla_hosts(&rows, "7d");
        let s = t.to_string();
        assert!(s.contains("github.com") && s.contains("scdn.co") && s.contains("Σ 7d"));
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
}
