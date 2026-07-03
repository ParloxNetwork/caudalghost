//! Dashboard interactivo `caudalghost live`: event loop, lifecycle y self-test.

pub mod netinfo;
pub mod state;
pub mod ui;

use caudalghost_core::proto::{Request, Response};
use state::{apply_key, Action, Key, LiveModel};
use std::path::Path;
use std::time::{Duration, Instant};

/// Aplica una respuesta Pong al modelo (versión + estado eBPF).
pub fn apply_pong(model: &mut LiveModel, resp: &Response) {
    if let Response::Pong { version, ebpf } = resp {
        model.version = version.clone();
        model.ebpf = *ebpf;
        model.connected = true;
    }
}

/// Aplica una respuesta Snapshot al modelo (samples de la ventana).
pub fn apply_snapshot(model: &mut LiveModel, resp: &Response) {
    if let Response::Snapshot { samples } = resp {
        model.samples = samples.clone();
        model.connected = true;
    }
}

/// Marca el modelo como desconectado (el daemon no respondió).
pub fn mark_disconnected(model: &mut LiveModel) {
    model.connected = false;
}

/// Aplica una respuesta Total (interfaces) al histórico acumulado.
pub fn apply_total_ifaces(model: &mut LiveModel, resp: &Response) {
    if let Response::Total { rows } = resp {
        model.hist_ifaces = rows.clone();
    }
}

/// Aplica una respuesta TotalApps (apps) al histórico acumulado.
pub fn apply_total_apps(model: &mut LiveModel, resp: &Response) {
    if let Response::TotalApps { rows } = resp {
        model.hist_apps = rows.clone();
    }
}

/// Restaura la terminal al salir aunque haya panic.
struct TerminalGuard;
impl Drop for TerminalGuard {
    fn drop(&mut self) {
        use ratatui::crossterm::{
            execute,
            terminal::{disable_raw_mode, LeaveAlternateScreen},
        };
        let _ = disable_raw_mode();
        let _ = execute!(std::io::stdout(), LeaveAlternateScreen);
    }
}

/// Corre el dashboard interactivo contra el daemon en `socket`.
pub fn run(socket: &Path, window: u32, all: bool) -> anyhow::Result<()> {
    use ratatui::crossterm::{
        event::{self, Event, KeyCode, KeyEventKind},
        execute,
        terminal::{enable_raw_mode, EnterAlternateScreen},
    };
    use ratatui::{backend::CrosstermBackend, Terminal};

    enable_raw_mode()?;
    let _guard = TerminalGuard;
    let mut stdout = std::io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let mut terminal = Terminal::new(CrosstermBackend::new(stdout))?;

    let mut model = LiveModel {
        show_all: all,
        ..Default::default()
    };
    model.iface_ips = netinfo::read_iface_ipv4s();
    if let Ok(p) = crate::client::call(socket, &Request::Ping) {
        apply_pong(&mut model, &p);
    } else {
        mark_disconnected(&mut model);
    }

    let tick = Duration::from_secs(1);
    let mut last_fetch = Instant::now()
        .checked_sub(tick)
        .unwrap_or_else(Instant::now);

    loop {
        model.frame = model.frame.wrapping_add(1);

        let espera = if model.connected {
            tick
        } else {
            state::siguiente_backoff(model.reconnect_attempts)
        };
        if !model.paused && last_fetch.elapsed() >= espera {
            // Re-pedir Ping cada tick: refresca versión y estado eBPF. Sin esto, si el
            // daemon se relanza con sudo (eBPF pasa a activo), el flag quedaría congelado
            // en el valor de arranque y los paneles de apps seguirían con el aviso.
            let ok_ping = match crate::client::call(socket, &Request::Ping) {
                Ok(resp) => {
                    apply_pong(&mut model, &resp);
                    true
                }
                Err(_) => {
                    mark_disconnected(&mut model);
                    false
                }
            };
            let ok_snapshot =
                match crate::client::call(socket, &Request::Snapshot { seconds: window }) {
                    Ok(resp) => {
                        apply_snapshot(&mut model, &resp);
                        true
                    }
                    Err(_) => {
                        mark_disconnected(&mut model);
                        false
                    }
                };
            model.iface_ips = netinfo::read_iface_ipv4s();
            if let Ok(resp) = crate::client::call(
                socket,
                &Request::Total {
                    start: 0,
                    end: i64::MAX,
                },
            ) {
                apply_total_ifaces(&mut model, &resp);
            }
            if let Ok(resp) = crate::client::call(
                socket,
                &Request::TotalApps {
                    start: 0,
                    end: i64::MAX,
                },
            ) {
                apply_total_apps(&mut model, &resp);
            }
            if ok_ping && ok_snapshot {
                model.reconnect_attempts = 0;
            } else {
                model.reconnect_attempts = model.reconnect_attempts.saturating_add(1);
            }
            last_fetch = Instant::now();
        }

        terminal.draw(|f| ui::draw(f, &model))?;

        if model.flash_frames > 0 {
            model.flash_frames -= 1;
            if model.flash_frames == 0 {
                model.flash_key = None;
            }
        }

        if event::poll(Duration::from_millis(250))? {
            if let Event::Key(k) = event::read()? {
                if k.kind == KeyEventKind::Press {
                    let key = match k.code {
                        KeyCode::Char(c) => Key::Char(c),
                        KeyCode::Esc => Key::Esc,
                        KeyCode::Enter => Key::Enter,
                        KeyCode::Backspace => Key::Backspace,
                        KeyCode::Tab => Key::Tab,
                        KeyCode::BackTab => Key::ShiftTab,
                        KeyCode::Up => Key::Up,
                        KeyCode::Down => Key::Down,
                        KeyCode::Left => Key::Left,
                        KeyCode::Right => Key::Right,
                        _ => continue,
                    };
                    if apply_key(&mut model, key) == Action::Quit {
                        break;
                    }
                }
            }
        }
    }
    Ok(())
}

/// Renderiza un frame contra un backend en memoria con datos sintéticos y
/// devuelve el buffer como texto. Base del `--self-test` headless (sin TTY).
pub fn render_to_string(model: &LiveModel, width: u16, height: u16) -> String {
    use ratatui::{backend::TestBackend, Terminal};
    let backend = TestBackend::new(width, height);
    let mut terminal = Terminal::new(backend).expect("TestBackend no debería fallar");
    terminal
        .draw(|f| ui::draw(f, model))
        .expect("draw no debería fallar");
    let buffer = terminal.backend().buffer().clone();
    buffer
        .content()
        .iter()
        .map(|c| c.symbol())
        .collect::<String>()
}

/// `--self-test`: renderiza frames sintéticos sin abrir TTY y devuelve Ok.
/// Sale 0 si no hay panic. Smoke headless del dashboard: cubre el modo
/// Normal (por defecto) y el overlay de Ayuda.
pub fn self_test() -> anyhow::Result<()> {
    let model = LiveModel {
        version: "self-test".into(),
        ebpf: false,
        connected: true,
        ..Default::default()
    };
    let _ = render_to_string(&model, 100, 30);

    let model_ayuda = LiveModel {
        version: "self-test".into(),
        ebpf: false,
        connected: true,
        mode: state::Mode::Ayuda,
        ..Default::default()
    };
    let _ = render_to_string(&model_ayuda, 100, 30);

    println!("caudalghost live --self-test: OK");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn render_to_string_no_panica_y_produce_contenido() {
        let model = LiveModel {
            version: "0.1.0".into(),
            connected: true,
            ..Default::default()
        };
        let s = render_to_string(&model, 80, 24);
        assert!(!s.trim().is_empty());
        assert!(s.chars().count() == 80 * 24);
    }

    #[test]
    fn render_to_string_modo_ayuda_no_panica_y_produce_contenido() {
        let model = LiveModel {
            version: "0.1.0".into(),
            connected: true,
            mode: state::Mode::Ayuda,
            ..Default::default()
        };
        let s = render_to_string(&model, 100, 30);
        assert!(!s.trim().is_empty());
        assert!(s.chars().count() == 100 * 30);
        assert!(s.contains("ayuda") || s.contains("AYUDA"));
    }

    #[test]
    fn fetch_actualiza_modelo_desde_respuestas() {
        use caudalghost_core::proto::Response;
        use caudalghost_core::types::{IfaceDelta, Sample};
        let mut m = LiveModel::default();
        let pong = Response::Pong {
            version: "1.2.3".into(),
            ebpf: true,
        };
        let snap = Response::Snapshot {
            samples: vec![Sample {
                ts: 0,
                deltas: vec![IfaceDelta {
                    iface: "eth0".into(),
                    rx: 9,
                    tx: 1,
                }],
                apps: vec![],
                flows: vec![],
            }],
        };
        apply_pong(&mut m, &pong);
        apply_snapshot(&mut m, &snap);
        assert_eq!(m.version, "1.2.3");
        assert!(m.ebpf);
        assert_eq!(m.samples.len(), 1);
        assert!(m.connected);
    }

    #[test]
    fn apply_total_apps_e_ifaces_pueblan_historico() {
        use caudalghost_core::proto::{AppRow, Response, TotalRow};
        let mut m = LiveModel::default();
        let ra = Response::TotalApps {
            rows: vec![AppRow {
                app: "megasync".into(),
                rx: 1000,
                tx: 200,
            }],
        };
        let ri = Response::Total {
            rows: vec![TotalRow {
                iface: "wlp2s0".into(),
                rx: 5000,
                tx: 600,
            }],
        };
        apply_total_apps(&mut m, &ra);
        apply_total_ifaces(&mut m, &ri);
        assert_eq!(m.hist_apps.len(), 1);
        assert_eq!(m.hist_apps[0].app, "megasync");
        assert_eq!(m.hist_ifaces.len(), 1);
        assert_eq!(m.hist_ifaces[0].iface, "wlp2s0");
    }

    #[test]
    fn marca_desconectado() {
        let mut m = LiveModel {
            connected: true,
            ..Default::default()
        };
        mark_disconnected(&mut m);
        assert!(!m.connected);
    }
}
