#![no_std]
#![no_main]

use aya_ebpf::{
    helpers::{bpf_get_current_pid_tgid, bpf_probe_read_kernel},
    macros::{kprobe, map},
    maps::PerCpuHashMap,
    programs::ProbeContext,
};
use caudalghost_ebpf_common::{FlowKey, TrafficVal};

/// (tgid, remoto) → contadores. PerCpu: sin contención en kernel; userspace suma
/// las CPUs y borra la clave tras drenar. `max_entries` dimensionado al working
/// set de un ciclo de drenado (~2 s), no sobredimensionado (footprint, §3.7).
#[map]
static TRAFFIC: PerCpuHashMap<FlowKey, TrafficVal> = PerCpuHashMap::with_max_entries(16_384, 0);

// Offsets dentro de `struct sock` (sock_common está en offset 0), verificados
// contra el BTF del kernel en ejecución (2026-06-12). Reverificar con bpftool si
// cambia el kernel. `skc_v6_daddr` depende de CONFIG_NET_NS (=y en escritorio).
const SKC_DADDR_OFF: usize = 0;
const SKC_FAMILY_OFF: usize = 16;
const SKC_V6_DADDR_OFF: usize = 56;
const AF_INET: u16 = 2;
const AF_INET6: u16 = 10;

/// Lee el endpoint remoto del socket `sk`. Ante cualquier fallo: family=0
/// (el userspace lo muestra como "(sin nombre)"). Sin bucles → verificador feliz.
#[inline(always)]
fn remote_of(sk: u64) -> ([u8; 16], u8) {
    let mut addr = [0u8; 16];
    if sk == 0 {
        return (addr, 0);
    }
    let family =
        unsafe { bpf_probe_read_kernel::<u16>((sk as usize + SKC_FAMILY_OFF) as *const u16) }
            .unwrap_or(0);
    match family {
        AF_INET => match unsafe {
            bpf_probe_read_kernel::<[u8; 4]>((sk as usize + SKC_DADDR_OFF) as *const [u8; 4])
        } {
            Ok(d) => {
                addr[0..4].copy_from_slice(&d); // __be32: ya en orden de red
                (addr, 4)
            }
            Err(_) => (addr, 0),
        },
        AF_INET6 => match unsafe {
            bpf_probe_read_kernel::<[u8; 16]>((sk as usize + SKC_V6_DADDR_OFF) as *const [u8; 16])
        } {
            Ok(d) => (d, 6),
            Err(_) => (addr, 0),
        },
        _ => (addr, 0),
    }
}

#[inline(always)]
fn add(sk: u64, rx: u64, tx: u64) {
    let tgid = (bpf_get_current_pid_tgid() >> 32) as u32;
    let (addr, family) = remote_of(sk);
    let key = FlowKey {
        tgid,
        addr,
        family,
        _pad: [0; 3],
    };
    unsafe {
        match TRAFFIC.get_ptr_mut(&key) {
            Some(v) => {
                (*v).rx += rx;
                (*v).tx += tx;
            }
            None => {
                let _ = TRAFFIC.insert(&key, &TrafficVal { rx, tx }, 0);
            }
        }
    }
}

/// TX TCP: tcp_sendmsg(sk, msg, size) — arg0 = sk, arg2 = bytes a enviar.
#[kprobe]
pub fn tcp_sendmsg(ctx: ProbeContext) -> u32 {
    let sk = ctx.arg::<u64>(0).unwrap_or(0);
    if let Some(size) = ctx.arg::<usize>(2) {
        add(sk, 0, size as u64);
    }
    0
}

/// RX TCP: tcp_cleanup_rbuf(sk, copied) — arg0 = sk, arg1 = bytes entregados.
#[kprobe]
pub fn tcp_cleanup_rbuf(ctx: ProbeContext) -> u32 {
    let sk = ctx.arg::<u64>(0).unwrap_or(0);
    if let Some(copied) = ctx.arg::<i32>(1) {
        if copied > 0 {
            add(sk, copied as u64, 0);
        }
    }
    0
}

/// TX UDP: udp_sendmsg(sk, msg, len) — arg0 = sk, arg2 = bytes. El remoto solo
/// es válido si el socket está connected (D4); si no, daddr=0 → "(sin nombre)".
#[kprobe]
pub fn udp_sendmsg(ctx: ProbeContext) -> u32 {
    let sk = ctx.arg::<u64>(0).unwrap_or(0);
    if let Some(len) = ctx.arg::<usize>(2) {
        add(sk, 0, len as u64);
    }
    0
}

/// RX UDP: skb_consume_udp(sk, skb, len) — arg0 = sk, arg2 = bytes consumidos.
#[kprobe]
pub fn skb_consume_udp(ctx: ProbeContext) -> u32 {
    let sk = ctx.arg::<u64>(0).unwrap_or(0);
    if let Some(len) = ctx.arg::<i32>(2) {
        if len > 0 {
            add(sk, len as u64, 0);
        }
    }
    0
}

#[cfg(not(test))]
#[panic_handler]
fn panic(_info: &core::panic::PanicInfo) -> ! {
    loop {}
}

#[link_section = "license"]
#[no_mangle]
static LICENSE: [u8; 7] = *b"GPL v2\0";
