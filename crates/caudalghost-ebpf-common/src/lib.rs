#![no_std]

/// Contadores de tráfico por proceso — valor del mapa eBPF.
/// repr(C): el layout debe coincidir exactamente entre kernel y userspace.
#[repr(C)]
#[derive(Debug, Clone, Copy, Default)]
pub struct TrafficVal {
    pub rx: u64,
    pub tx: u64,
}

#[cfg(feature = "user")]
unsafe impl aya::Pod for TrafficVal {}

/// Clave del mapa eBPF: proceso + endpoint remoto.
/// repr(C): el layout debe coincidir EXACTAMENTE entre kernel y userspace.
/// Orden elegido para que el padding sea explícito (size 24, align 4): así la
/// clave no tiene bytes indefinidos que rompan el hash/compare del mapa.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct FlowKey {
    pub tgid: u32,      // 0..4
    pub addr: [u8; 16], // 4..20  IPv4 en addr[0..4] (orden de red), resto 0; IPv6 completo
    pub family: u8,     // 20     4 = IPv4, 6 = IPv6, 0 = desconocido / sin destino
    pub _pad: [u8; 3],  // 21..24 padding explícito
}

#[cfg(feature = "user")]
unsafe impl aya::Pod for FlowKey {}
