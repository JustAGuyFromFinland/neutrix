#![no_std]

use crate::*;
use x86_64::VirtAddr;
use core::ptr::{read_volatile, write_volatile};
use alloc::vec::Vec;

/// Minimal IOAPIC driver: provides MMIO access to IOAPIC registers via
/// index/data registers and helpers to read ID/version and set redirection table entries.

const IOAPIC_REG_SELECT: usize = 0x00;
const IOAPIC_REG_WINDOW: usize = 0x10;

/// A discovered IOAPIC instance
#[derive(Debug, Clone, Copy)]
pub struct IoApic {
    pub id: u8,
    pub phys_addr: u32,
    pub gsi_base: u32,
    /// Number of redirection entries this IOAPIC implements
    pub redir_entries: u32,
}

impl IoApic {
    /// Read a 32-bit IOREGSEL/IOWIN register
    unsafe fn read_reg(base: *mut u8, reg: u8) -> u32 {
        let sel = base.add(IOAPIC_REG_SELECT) as *mut u32;
        let win = base.add(IOAPIC_REG_WINDOW) as *mut u32;
        write_volatile(sel, reg as u32);
        read_volatile(win)
    }

    /// Write a 32-bit IOREGSEL/IOWIN register
    unsafe fn write_reg(base: *mut u8, reg: u8, val: u32) {
        let sel = base.add(IOAPIC_REG_SELECT) as *mut u32;
        let win = base.add(IOAPIC_REG_WINDOW) as *mut u32;
        write_volatile(sel, reg as u32);
        write_volatile(win, val);
    }

    /// Read IOAPIC ID (register 0)
    pub fn read_id(&self, phys_offset: VirtAddr) -> Option<u8> {
        let virt = (self.phys_addr as u64 + phys_offset.as_u64()) as *mut u8;
        if virt.is_null() { return None; }
        unsafe {
            let id_reg = Self::read_reg(virt, 0) >> 24;
            Some(id_reg as u8)
        }
    }

    /// Read version (register 1 low byte)
    pub fn read_version(&self, phys_offset: VirtAddr) -> Option<u8> {
        let virt = (self.phys_addr as u64 + phys_offset.as_u64()) as *mut u8;
        if virt.is_null() { return None; }
        unsafe {
            let ver = Self::read_reg(virt, 1) & 0xFF;
            Some(ver as u8)
        }
    }

    /// Read maximum redirection entry index from version register and return count
    pub fn read_redir_count(&self, phys_offset: VirtAddr) -> Option<u32> {
        let virt = (self.phys_addr as u64 + phys_offset.as_u64()) as *mut u8;
        if virt.is_null() { return None; }
        unsafe {
            let reg = Self::read_reg(virt, 1);
            let max_redir = (reg >> 16) & 0xFF;
            Some(max_redir + 1)
        }
    }

    // Note: setting redirection entries would require mapping interrupts and careful flags.
}

// Store discovered IOAPICs for later use; allow HAL to initialize them from ACPI data.
use spin::Mutex;
static IOAPIC_TABLE: Mutex<Vec<IoApic>> = Mutex::new(Vec::new());

/// Initialize IOAPIC subsystem using MADT entries discovered by ACPI.
pub fn init_from_acpi(phys_offset: VirtAddr) {
    let ioapics = crate::devices::acpi::get_ioapics();
    if ioapics.is_empty() {
        println!("[HAL][IOAPIC] No IOAPIC entries found in MADT");
        return;
    }

    for info in ioapics.iter() {
        // Read redirection count if possible
        let mut redir_entries = 24u32; // fallback
        let virt = (info.addr as u64 + phys_offset.as_u64()) as *mut u8;
        if !virt.is_null() {
            unsafe {
                let reg = IoApic::read_reg(virt, 1);
                let max_redir = (reg >> 16) & 0xFF;
                redir_entries = max_redir + 1;
            }
        }

        // Push discovered entries into the table
        IOAPIC_TABLE.lock().push(IoApic {
            id: info.id,
            phys_addr: info.addr,
            gsi_base: info.gsi_base,
            redir_entries,
        });
    }

    // Log discovered IOAPICs
    for io in IOAPIC_TABLE.lock().iter() {
        println!("[HAL][IOAPIC] Found IOAPIC id={} phys=0x{:x} gsi_base={} redirs={}", io.id, io.phys_addr, io.gsi_base, io.redir_entries);
        if let Some(ver) = io.read_version(phys_offset) {
            println!("[HAL][IOAPIC] IOAPIC id={} version={}", io.id, ver);
        }
    }
}

/// Return a cloned list of discovered IOAPICs
pub fn list_ioapics() -> Vec<IoApic> {
    IOAPIC_TABLE.lock().clone()
}

/// Find the IOAPIC that handles a given GSI. Returns (index into IOAPIC_TABLE, local_index)
fn find_ioapic_for_gsi(gsi: u32) -> Option<(usize, u32)> {
    let table = IOAPIC_TABLE.lock();
    for (i, io) in table.iter().enumerate() {
        let start = io.gsi_base;
        let end = start + io.redir_entries;
        if gsi >= start && gsi < end {
            return Some((i, gsi - start));
        }
    }
    None
}

/// Stub: write a redirection entry (64-bit) for the given global system interrupt.
/// In a real kernel this must be used carefully and synchronized with interrupt controllers.
pub fn write_redirection_entry_for_gsi(gsi: u32, low: u32, high: u32, phys_offset: VirtAddr) -> bool {
    if let Some((idx, local)) = find_ioapic_for_gsi(gsi) {
        let table = IOAPIC_TABLE.lock();
        if let Some(io) = table.get(idx) {
            let virt = (io.phys_addr as u64 + phys_offset.as_u64()) as *mut u8;
            if virt.is_null() { return false; }
            unsafe {
                // Redirection entries start at register 0x10; each is two 32-bit registers
                let reg_low = 0x10 + (local as usize * 2) as u8;
                let reg_high = reg_low + 1;
                IoApic::write_reg(virt, reg_high, high);
                IoApic::write_reg(virt, reg_low, low);
            }
            return true;
        }
    }
    false
}

/// Apply Interrupt Source Overrides discovered from ACPI MADT: map legacy ISA IRQs to GSIs
pub fn apply_isos_from_acpi(phys_offset: VirtAddr) {
    let isos = crate::devices::acpi::get_isos();
    if isos.is_empty() {
        println!("[HAL][IOAPIC] No ISOs found in MADT");
        return;
    }

    for iso in isos.iter() {
        println!("[HAL][IOAPIC] ISO: bus={} source={} gsi={} flags=0x{:x}", iso.bus, iso.source, iso.gsi, iso.flags);
        if let Some((ioidx, local)) = find_ioapic_for_gsi(iso.gsi) {
            println!("[HAL][IOAPIC] GSI {} belongs to IOAPIC index {} at local entry {}", iso.gsi, ioidx, local);

            // Program a sane default redirection entry:
            // - use vector = 0x20 + source (keeps legacy mapping)
            // - delivery mode = fixed (0)
            // - destination mode = physical (0)
            // - polarity = 0 (active high)
            // - trigger mode = 0 (edge)
            // - masked = 1 initially (do not enable interrupts until kernel configures)
            let vector = 0x20u32.wrapping_add(iso.source as u32) & 0xFF;
            let low: u32 = (vector & 0xFF) | (1 << 16); // mask bit set
            let high: u32 = 0; // destination field left zero (physical CPU 0); can be updated later

            if write_redirection_entry_for_gsi(iso.gsi, low, high, phys_offset) {
                println!("[HAL][IOAPIC] Programmed redir for GSI {} -> vector 0x{:x} (masked)", iso.gsi, vector);
            } else {
                println!("[HAL][IOAPIC] Failed to program redir for GSI {}", iso.gsi);
            }
        } else {
            println!("[HAL][IOAPIC] No IOAPIC found that handles GSI {}", iso.gsi);
        }
    }
}

// NOTE: On some systems MADT may not contain explicit ISOs for all legacy
// ISA IRQs. As a pragmatic fallback we also program redirection entries for
// legacy IRQs 0..15 (GSI 0..15) to the conventional vectors 0x20+IRQ and
// keep them masked; the per-CPU unmask routine will set the destination
// and unmask when appropriate. This helps ensure devices like the PS/2
// keyboard (IRQ1 -> vector 0x21) actually deliver interrupts when using
// an IOAPIC-only setup.
pub fn apply_legacy_isa_fallback(phys_offset: VirtAddr) {
    for irq in 0u32..16u32 {
        let gsi = irq; // legacy ISA interrupts map directly to GSI 0..15 on most platforms
        // vector: 0x20 + irq
        let vector = 0x20u32.wrapping_add(irq) & 0xFF;
        let low: u32 = (vector & 0xFF) | (1 << 16); // masked by default
        let high: u32 = 0; // leave destination zero until per-CPU enable

        if write_redirection_entry_for_gsi(gsi, low, high, phys_offset) {
            println!("[HAL][IOAPIC] Fallback-programmed GSI {} -> vector 0x{:x} (masked)", gsi, vector);
        }
    }
}

/// Enable ISOs for the current CPU by setting the destination field to this CPU's APIC ID
/// and clearing the mask bit. This should be called on each CPU (AP) during bring-up when
/// the IDT is prepared to handle the vectors.
pub fn enable_isos_for_local(phys_offset: VirtAddr, local_apic_id: u8) {
    let isos = crate::devices::acpi::get_isos();
    if isos.is_empty() { return; }

    for iso in isos.iter() {
        if let Some((ioidx, local)) = find_ioapic_for_gsi(iso.gsi) {
            // read current low/high from the correct IOAPIC
            let table = IOAPIC_TABLE.lock();
            if let Some(io) = table.get(ioidx) {
                let virt = (io.phys_addr as u64 + phys_offset.as_u64()) as *mut u8;
                if virt.is_null() { continue; }
                unsafe {
                    let reg_low = 0x10 + (local as usize * 2) as u8;
                    let reg_high = reg_low + 1;
                    let mut high = IoApic::read_reg(virt, reg_high);
                    let mut low = IoApic::read_reg(virt, reg_low);
                    // set destination (high dword bits [63:56] for physical mode)
                    high = ((local_apic_id as u32) << 24) as u32; // corresponds to high dword bits [56:63] in x86 doc
                    // clear mask bit (bit 16)
                    low &= !(1 << 16);
                    IoApic::write_reg(virt, reg_high, high);
                    IoApic::write_reg(virt, reg_low, low);
                    println!("[HAL][IOAPIC] Enabled ISO GSI {} -> APIC {} (unmasked)", iso.gsi, local_apic_id);
                }
            }
        }
    }

    // Also enable legacy ISA IRQs (GSI 0..15) as a pragmatic per-CPU fallback.
    // This ensures devices wired to legacy IRQs (keyboard IRQ1) are targeted
    // to this CPU and unmasked when the CPU calls this function.
    for irq in 0u32..16u32 {
        if let Some((ioidx, local)) = find_ioapic_for_gsi(irq) {
            let table = IOAPIC_TABLE.lock();
            if let Some(io) = table.get(ioidx) {
                let virt = (io.phys_addr as u64 + phys_offset.as_u64()) as *mut u8;
                if virt.is_null() { continue; }
                unsafe {
                    let reg_low = 0x10 + (local as usize * 2) as u8;
                    let reg_high = reg_low + 1;
                    let mut high = IoApic::read_reg(virt, reg_high);
                    let mut low = IoApic::read_reg(virt, reg_low);
                    // program destination for physical mode
                    high = ((local_apic_id as u32) << 24) as u32;
                    // clear mask bit
                    low &= !(1 << 16);
                    IoApic::write_reg(virt, reg_high, high);
                    IoApic::write_reg(virt, reg_low, low);
                    let vector = 0x20u32.wrapping_add(irq) & 0xFF;
                    println!("[HAL][IOAPIC] Per-CPU enabled legacy IRQ {} (GSI {}) -> APIC {} vector 0x{:x}", irq, irq, local_apic_id, vector);
                }
            }
        }
    }
}
