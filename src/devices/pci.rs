use crate::driver_framework::manager::GLOBAL_MANAGER;
use crate::driver_framework::device::{DeviceInfo, Resource, ResourceKind};
use crate::arch::ports::{outdw, indw};
use alloc::string::String;
use crate::*;
use alloc::format;

fn pci_write(bus: u8, slot: u8, func: u8, offset: u8, val: u32) {
    let addr = pci_config_address(bus, slot, func, offset);
    unsafe { outdw(0xCF8, addr); }
    unsafe { outdw(0xCFC, val); }
}

fn pci_config_address(bus: u8, slot: u8, func: u8, offset: u8) -> u32 {
    let l = 0x8000_0000u32
        | ((bus as u32) << 16)
        | ((slot as u32) << 11)
        | ((func as u32) << 8)
        | ((offset as u32) & 0xfc);
    l
}

fn pci_read(bus: u8, slot: u8, func: u8, offset: u8) -> u32 {
    let addr = pci_config_address(bus, slot, func, offset);
    unsafe { outdw(0xCF8, addr); }
    unsafe { indw(0xCFC) }
}

/// Very small PCI scan that registers devices with the global manager.
pub fn scan_and_register() {
    scan_and_register_with_phys_offset(0)
}

/// Scan with a physical memory offset so we can map BARs for MSI-X table reads.
pub fn scan_and_register_with_phys_offset(physical_memory_offset: u64) {
    // Scan all buses (0-255). This is simple and safe for a basic enumerator.
    for bus in 0u8..=255u8 {
        for slot in 0u8..32u8 {
            // First probe function 0 to see if device exists and whether it's multifunction
            let vendor_device = pci_read(bus, slot, 0, 0);
            let vendor0 = (vendor_device & 0xFFFF) as u16;
            if vendor0 == 0xFFFF || vendor0 == 0x0000 {
                continue;
            }

            // Determine if multifunction by reading header type (byte at 0x0E)
            let header_dword = pci_read(bus, slot, 0, 0x0C);
            let header_type = ((header_dword >> 16) & 0xFF) as u8;
            let multifunction = (header_type & 0x80) != 0;

            let max_funcs = if multifunction { 8 } else { 1 };

            for func in 0u8..max_funcs {
                let vendor_device = pci_read(bus, slot, func, 0);
                let vendor = (vendor_device & 0xFFFF) as u16;
                if vendor == 0xFFFF || vendor == 0x0000 {
                    continue;
                }
                let device = ((vendor_device >> 16) & 0xFFFF) as u16;
                let class_reg = pci_read(bus, slot, func, 8);
                let prog_if = ((class_reg >> 8) & 0xFF) as u8;
                let subclass = ((class_reg >> 16) & 0xFF) as u8;
                let class = ((class_reg >> 24) & 0xFF) as u8;

                let mut resources = alloc::vec::Vec::new();

                // Read and size BARs
                let mut bar_index: u8 = 0;
                while bar_index < 6 {
                    let off = 0x10u8 + (bar_index * 4);
                    let orig = pci_read(bus, slot, func, off);
                    if orig == 0 || orig == 0xFFFF_FFFF {
                        bar_index += 1;
                        continue;
                    }

                    // IO BAR
                    if (orig & 0x1) == 0x1 {
                        // Save, write all 1s, read back, restore
                        pci_write(bus, slot, func, off, 0xFFFF_FFFF);
                        let mask = pci_read(bus, slot, func, off);
                        pci_write(bus, slot, func, off, orig);

                        let mask32 = mask & 0xFFFF_FFFC;
                        let size = ((!mask32).wrapping_add(1)) as u64;
                        let addr = (orig & 0xFFFFFFFC) as u64;
                        resources.push(Resource { kind: ResourceKind::IO, addr, len: size });
                        bar_index += 1;
                        continue;
                    }

                    // Memory BAR - could be 64-bit
                    let mem_type = (orig >> 1) & 0x3;
                    if mem_type == 0x2 {
                        // 64-bit BAR consumes this and the next
                        let off_high = 0x10u8 + ((bar_index + 1) * 4);
                        let orig_high = pci_read(bus, slot, func, off_high);

                        // Write mask to low and high
                        pci_write(bus, slot, func, off, 0xFFFF_FFFF);
                        pci_write(bus, slot, func, off_high, 0xFFFF_FFFF);
                        let mask_low = pci_read(bus, slot, func, off) as u32;
                        let mask_high = pci_read(bus, slot, func, off_high) as u32;
                        // Restore originals
                        pci_write(bus, slot, func, off, orig);
                        pci_write(bus, slot, func, off_high, orig_high);

                        let mask64 = ((mask_high as u64) << 32) | (mask_low as u64);
                        let mask64_base = mask64 & !0xF_u64;
                        let size = ((!mask64_base).wrapping_add(1)) as u64;
                        let addr = (((orig_high as u64) << 32) | ((orig as u64) & 0xFFFF_FFF0)) as u64;
                        resources.push(Resource { kind: ResourceKind::MemoryMapped, addr, len: size });

                        // Skip the next BAR since it was part of 64-bit
                        bar_index += 2;
                        continue;
                    } else {
                        // 32-bit memory BAR
                        pci_write(bus, slot, func, off, 0xFFFF_FFFF);
                        let mask = pci_read(bus, slot, func, off);
                        pci_write(bus, slot, func, off, orig);

                        let mask32 = (mask & !0xF) as u32;
                        let size = ((!mask32).wrapping_add(1)) as u64;
                        let addr = (orig & 0xFFFF_FFF0) as u64;
                        resources.push(Resource { kind: ResourceKind::MemoryMapped, addr, len: size });
                        bar_index += 1;
                        continue;
                    }
                }

                // Read interrupt information (offset 0x3C: byte IRQ, byte Pin)
                let intr = pci_read(bus, slot, func, 0x3C);
                let irq_line = (intr & 0xFF) as u8;
                let irq_pin = ((intr >> 8) & 0xFF) as u8;
                if irq_line != 0 && irq_line != 0xFF {
                    resources.push(Resource { kind: ResourceKind::Interrupt(irq_line), addr: 0, len: 0 });
                }

                // Parse capability list if present (Status register bit 4)
                let status = pci_read(bus, slot, func, 0x04);
                let status_word = ((status >> 16) & 0xFFFF) as u16;
                let mut capabilities: alloc::vec::Vec<crate::driver_framework::device::Capability> = alloc::vec::Vec::new();
                if (status_word & (1 << 4)) != 0 {
                    // capabilities pointer at offset 0x34 (byte)
                    let mut cap_ptr = (pci_read(bus, slot, func, 0x34) & 0xFF) as u8;
                    let mut caps_searched = 0;
                    while cap_ptr != 0 && caps_searched < 48 {
                        let cap_dword = pci_read(bus, slot, func, (cap_ptr & 0xFC));
                        let cap_id = (cap_dword & 0xFF) as u8;
                        let next_ptr = ((cap_dword >> 8) & 0xFF) as u8;

                        match cap_id {
                            0x01 => {
                                // Power Management - read PM Capabilities (16-bit) and PMCSR (16-bit at offset +4)
                                let pmcap = ((cap_dword >> 16) & 0xFFFF) as u16;
                                let pmcsr_dword = pci_read(bus, slot, func, ((cap_ptr).wrapping_add(4) & 0xFC));
                                let shift = (((cap_ptr as usize + 4) & 3) * 8) as u32;
                                let pmcsr = ((pmcsr_dword >> shift) & 0xFFFF) as u16;
                                capabilities.push(crate::driver_framework::device::Capability::PowerManagement { pm_cap: pmcap, pmcsr });
                            }
                            0x05 => {
                                // MSI
                                // MSI control is at offset cap_ptr+2 (16 bits)
                                let ctrl_dword = pci_read(bus, slot, func, ((cap_ptr).wrapping_add(2) & 0xFC));
                                let shift = (((cap_ptr as usize + 2) & 3) * 8) as u32;
                                let ctrl = ((ctrl_dword >> shift) & 0xFFFF) as u16;
                                let multiple_message_capable = (ctrl >> 1) & 0x7;
                                let multiple_message_enable = (ctrl >> 4) & 0x1;
                                let vectors = 1u8 << multiple_message_capable;
                                // Address64 flag located at bit 7 of control
                                let addr64 = (ctrl & (1 << 7)) != 0;
                                // Maskable/per-vector mask presence (bit 8 indicates Maskable)
                                let maskable = (ctrl & (1 << 8)) != 0;

                                // Read message address and data fields following the control field.
                                // Message address low is at cap_ptr+4 (dword aligned), may have an upper dword if addr64.
                                let mut msg_addr_low: u32 = 0;
                                let mut msg_addr_high: u32 = 0;
                                let mut msg_data: u16 = 0;
                                let off_addr = (cap_ptr).wrapping_add(4);
                                let daddr = pci_read(bus, slot, func, (off_addr & 0xFC));
                                let shift_addr = (((off_addr as usize) & 3) * 8) as u32;
                                msg_addr_low = ((daddr >> shift_addr) & 0xFFFF_FFFF) as u32;
                                if addr64 {
                                    let off_addr_hi = off_addr.wrapping_add(4);
                                    let daddr_hi = pci_read(bus, slot, func, (off_addr_hi & 0xFC));
                                    let shift_hi = (((off_addr_hi as usize) & 3) * 8) as u32;
                                    msg_addr_high = ((daddr_hi >> shift_hi) & 0xFFFF_FFFF) as u32;
                                    // message data follows at off_addr+8
                                    let off_data = off_addr.wrapping_add(8);
                                    let ddata = pci_read(bus, slot, func, (off_data & 0xFC));
                                    let shift_data = (((off_data as usize) & 3) * 8) as u32;
                                    msg_data = ((ddata >> shift_data) & 0xFFFF) as u16;
                                } else {
                                    // 32-bit address: message data at off_addr+4
                                    let off_data = off_addr.wrapping_add(4);
                                    let ddata = pci_read(bus, slot, func, (off_data & 0xFC));
                                    let shift_data = (((off_data as usize) & 3) * 8) as u32;
                                    msg_data = ((ddata >> shift_data) & 0xFFFF) as u16;
                                }

                                // Canonicalize message address into u64
                                let msg_addr: u64 = if addr64 {
                                    ((msg_addr_high as u64) << 32) | (msg_addr_low as u64)
                                } else {
                                    (msg_addr_low as u64)
                                };
                                resources.push(Resource { kind: ResourceKind::Msi { vectors, addr64, maskable, msg_addr, msg_data }, addr: 0, len: 0 });
                            }
                            0x10 => {
                                // PCI Express capability (cap id 0x10)
                                let d0 = pci_read(bus, slot, func, (cap_ptr & 0xFC));
                                let d1 = pci_read(bus, slot, func, ((cap_ptr).wrapping_add(4) & 0xFC));
                                capabilities.push(crate::driver_framework::device::Capability::PciExpress { header: d0, device_cap: d1 });
                            }
                            0x11 => {
                                // MSI-X
                                // MSI-X capability layout: table offset/BIR at cap_ptr+4
                                let dword1 = pci_read(bus, slot, func, ((cap_ptr).wrapping_add(4) & 0xFC));
                                // extract BIR (bits 0-2) and offset (bits 3-31)
                                let shift_d1 = (((cap_ptr as usize + 4) & 3) * 8) as u32;
                                let dword1_shifted = dword1 >> shift_d1;
                                let bir = (dword1_shifted & 0x7) as u8;
                                let table_offset = (dword1_shifted & 0xFFFF_FFF8) as u32;
                                // Table size is at cap_ptr+2 lower 11 bits
                                let dword0 = pci_read(bus, slot, func, ((cap_ptr).wrapping_add(2) & 0xFC));
                                let shift_ts = (((cap_ptr as usize + 2) & 3) * 8) as u32;
                                let table_size_field = ((dword0 >> shift_ts) & 0x7FF) as u16;
                                let table_size = table_size_field + 1;
                                // Attempt to probe the MSI-X table in device memory if we have a physical memory offset
                                let mut table_present = false;
                                let mut first_entry_masked = false;
                                if physical_memory_offset != 0 {
                                    // Find corresponding BAR base for bir. Use the bir-th MemoryMapped BAR.
                                    let mut mmio_bars: alloc::vec::Vec<&Resource> = alloc::vec::Vec::new();
                                    for r in resources.iter() {
                                        if let ResourceKind::MemoryMapped = r.kind { mmio_bars.push(r); }
                                    }
                                    if (bir as usize) < mmio_bars.len() {
                                        let bar_base = mmio_bars[bir as usize].addr;
                                        let table_phys = bar_base.wrapping_add(table_offset as u64);
                                        let virt = physical_memory_offset.wrapping_add(table_phys);
                                        // Safety: read u32 at virt + 12 (Vector Control of first entry)
                                        unsafe {
                                            let ptr = virt as *const u32;
                                            let vctrl = ptr.add(3).read_volatile();
                                            table_present = true;
                                            first_entry_masked = (vctrl & 0x1) != 0;
                                        }
                                    }
                                }
                                resources.push(Resource { kind: ResourceKind::Msix { table_bar: bir, table_offset, table_size, table_present, first_entry_masked }, addr: 0, len: 0 });
                            }
                            _ => {
                                // Other capability: store raw dwords
                                let r0 = pci_read(bus, slot, func, (cap_ptr & 0xFC));
                                let r1 = pci_read(bus, slot, func, ((cap_ptr).wrapping_add(4) & 0xFC));
                                capabilities.push(crate::driver_framework::device::Capability::Other { id: cap_id, raw0: r0, raw1: r1 });
                            }
                        }

                        cap_ptr = next_ptr;
                        caps_searched += 1;
                    }
                }

                let info = DeviceInfo {
                    vendor_id: vendor,
                    device_id: device,
                    class,
                    subclass,
                    prog_if,
                    resources,
                    capabilities,
                    description: String::from(format!("PCI {:02x}:{:02x}.{:x}", bus, slot, func)),
                };

                let id = GLOBAL_MANAGER.register_device(info);
                println!("PCI: registered device id={} {:04x}:{:04x} @ {}:{}:{}", id, vendor, device, bus, slot, func);
            }
        }
    }
}
