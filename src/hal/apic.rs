//! HAL APIC support (Local APIC)
//!
//! This module provides a small wrapper around the Local APIC hardware. It
//! reads the Local APIC base address from the MADT (via ACPI) when available
//! and provides basic operations: init, enable, send EOI, and read ID.

use crate::*;
use x86_64::VirtAddr;
use core::ptr::{read_volatile, write_volatile};
use core::sync::atomic::{AtomicUsize, Ordering};

// Offsets for some Local APIC registers (relative to the LAPIC base)
const LAPIC_ID: usize = 0x20;
const LAPIC_EOI: usize = 0xB0;
const LAPIC_SVR: usize = 0xF0;
const LAPIC_SVR_APIC_ENABLE: u32 = 0x100;

// Store LAPIC base as an atomic usize (0 == not initialized)
static LAPIC_BASE: AtomicUsize = AtomicUsize::new(0);

/// Initialize Local APIC using ACPI-provided MADT address (phys_offset is required to map)
pub fn init_from_acpi(phys_offset: VirtAddr) -> bool {
    // ACPI code will parse MADT during `init_acpi`; query for the discovered local APIC address
    if let Some(lapic_phys) = crate::devices::acpi::get_local_apic_address() {
        return init_lapic_phys(lapic_phys as u64, phys_offset);
    }
    false
}

/// Initialize LAPIC by mapping the physical LAPIC address using the provided phys_offset
fn init_lapic_phys(phys_addr: u64, phys_offset: VirtAddr) -> bool {
    // Convert physical to virtual using the provided phys_offset (this kernel maps identity + offset)
    let virt = (phys_addr + phys_offset.as_u64()) as *mut u8;
    if virt.is_null() {
        return false;
    }
    unsafe {
        // store pointer as usize atomically
        LAPIC_BASE.store(virt as usize, Ordering::SeqCst);

        // Enable APIC by setting Spurious Interrupt Vector Register's APIC enable bit
        let svr_addr = virt.add(LAPIC_SVR) as *mut u32;
        let mut svr = read_volatile(svr_addr);
        svr |= LAPIC_SVR_APIC_ENABLE;
        write_volatile(svr_addr, svr);
    }
    println!("[HAL][APIC] Local APIC initialized at phys 0x{:x}", phys_addr);
    true
}

/// Send End Of Interrupt to the local APIC
pub fn send_eoi() {
    // Load the base pointer atomically
    let base_usize = LAPIC_BASE.load(Ordering::SeqCst);
    if base_usize == 0 {
        return;
    }
    unsafe {
        let base = base_usize as *mut u8;
        let eoi = (base as usize + LAPIC_EOI) as *mut u32;
        write_volatile(eoi, 0u32);
    }
}

/// Read Local APIC ID
pub fn local_apic_id() -> Option<u8> {
    let base_usize = LAPIC_BASE.load(Ordering::SeqCst);
    if base_usize == 0 {
        return None;
    }
    unsafe {
        let base = base_usize as *const u8;
        let id_ptr = (base as usize + LAPIC_ID) as *const u32;
        let id = read_volatile(id_ptr);
        Some(((id >> 24) & 0xFF) as u8)
    }
}

/// Check whether LAPIC has been initialized
pub fn is_initialized() -> bool {
    LAPIC_BASE.load(Ordering::SeqCst) != 0
}
