use crate::*;
use core::ptr;
use core::str;
use x86_64::VirtAddr;

/// ACPI RSDP (Root System Description Pointer) structure
#[repr(C, packed)]
#[derive(Debug, Clone, Copy)]
pub struct Rsdp {
    pub signature: [u8; 8],
    pub checksum: u8,
    pub oem_id: [u8; 6],
    pub revision: u8,
    pub rsdt_address: u32,
    pub length: u32,
    pub xsdt_address: u64,
    pub extended_checksum: u8,
    pub reserved: [u8; 3],
}

impl Rsdp {
    /// Check if the RSDP signature is valid
    pub fn is_valid(&self) -> bool {
        &self.signature == b"RSD PTR "
    }

    /// Calculate checksum for validation
    pub fn checksum_valid(&self) -> bool {
        let bytes = unsafe { core::slice::from_raw_parts(self as *const Self as *const u8, 20) };
        bytes.iter().fold(0u8, |acc, &x| acc.wrapping_add(x)) == 0
    }

    /// Get the RSDT address (for ACPI 1.0) or XSDT address (for ACPI 2.0+)
    pub fn table_address(&self) -> u64 {
        if self.revision >= 2 {
            self.xsdt_address
        } else {
            self.rsdt_address as u64
        }
    }
}

/// ACPI table header (common to all ACPI tables)
#[repr(C, packed)]
#[derive(Debug, Clone, Copy)]
pub struct AcpiTableHeader {
    pub signature: [u8; 4],
    pub length: u32,
    pub revision: u8,
    pub checksum: u8,
    pub oem_id: [u8; 6],
    pub oem_table_id: [u8; 8],
    pub oem_revision: u32,
    pub creator_id: u32,
    pub creator_revision: u32,
}

impl AcpiTableHeader {
    /// Check if table checksum is valid
    pub fn checksum_valid(&self) -> bool {
        let len = self.length as usize;
        let bytes = unsafe { core::slice::from_raw_parts(self as *const Self as *const u8, len) };
        bytes.iter().fold(0u8, |acc, &x| acc.wrapping_add(x)) == 0
    }

    /// Get table signature as string
    pub fn signature_str(&self) -> &str {
        str::from_utf8(&self.signature).unwrap_or("INVALID")
    }
}

/// Find the RSDP in memory
pub fn find_rsdp(phys_offset: u64) -> Option<&'static Rsdp> {
    // Search in EBDA (Extended BIOS Data Area) first
    let ebda_start = 0x9FC00;
    let ebda_end = 0xA0000;

    if let Some(rsdp) = search_rsdp_in_range(ebda_start, ebda_end, phys_offset) {
        return Some(rsdp);
    }

    // Search in main BIOS area
    let bios_start = 0xE0000;
    let bios_end = 0x100000;

    search_rsdp_in_range(bios_start, bios_end, phys_offset)
}

/// Search for RSDP in a specific memory range
fn search_rsdp_in_range(start: usize, end: usize, phys_offset: u64) -> Option<&'static Rsdp> {
    let start_ptr = (start as u64 + phys_offset) as *const u8;
    let end_ptr = (end as u64 + phys_offset) as *const u8;

    let mut ptr = start_ptr;
    while ptr < end_ptr {
        let rsdp = unsafe { &*(ptr as *const Rsdp) };
        if rsdp.is_valid() && rsdp.checksum_valid() {
            return Some(rsdp);
        }
        ptr = unsafe { ptr.add(16) }; // RSDP is 16-byte aligned
    }
    None
}

/// Initialize ACPI
pub fn init_with_offset(phys_offset: VirtAddr) {
    println!("[ACPI] Initializing ACPI...");

    if let Some(rsdp) = find_rsdp(phys_offset.as_u64()) {
        println!("[ACPI] Found RSDP at {:p}", rsdp);
        println!("[ACPI] ACPI revision: {}", rsdp.revision);
        println!("[ACPI] RSDT/XSDT address: {:#x}", rsdp.table_address());

        // Parse RSDT/XSDT and list all tables
        parse_rsdt_xsdt(rsdp, phys_offset.as_u64());
    } else {
        println!("[ACPI] RSDP not found - ACPI not available");
    }
}

/// RSDT (Root System Description Table) structure
#[repr(C, packed)]
#[derive(Debug)]
pub struct Rsdt {
    pub header: AcpiTableHeader,
    // Followed by array of u32 table pointers
}

/// XSDT (Extended System Description Table) structure  
#[repr(C, packed)]
#[derive(Debug)]
pub struct Xsdt {
    pub header: AcpiTableHeader,
    // Followed by array of u64 table pointers
}

/// FACP (Fixed ACPI Description Table) structure
#[repr(C, packed)]
#[derive(Debug, Clone, Copy)]
pub struct Facp {
    pub header: AcpiTableHeader,
    pub firmware_ctrl: u32,
    pub dsdt: u32,
    pub reserved: u8,
    pub preferred_pm_profile: u8,
    pub sci_int: u16,
    pub smi_cmd: u32,
    pub acpi_enable: u8,
    pub acpi_disable: u8,
    pub s4bios_req: u8,
    pub pstate_cnt: u8,
    pub pm1a_evt_blk: u32,
    pub pm1b_evt_blk: u32,
    pub pm1a_cnt_blk: u32,
    pub pm1b_cnt_blk: u32,
    pub pm2_cnt_blk: u32,
    pub pm_tmr_blk: u32,
    pub gpe0_blk: u32,
    pub gpe1_blk: u32,
    pub pm1_evt_len: u8,
    pub pm1_cnt_len: u8,
    pub pm2_cnt_len: u8,
    pub pm_tmr_len: u8,
    pub gpe0_blk_len: u8,
    pub gpe1_blk_len: u8,
    pub gpe1_base: u8,
    pub cst_cnt: u8,
    pub p_lvl2_lat: u16,
    pub p_lvl3_lat: u16,
    pub flush_size: u16,
    pub flush_stride: u16,
    pub duty_offset: u8,
    pub duty_width: u8,
    pub day_alrm: u8,
    pub mon_alrm: u8,
    pub century: u8,
    pub iapc_boot_arch: u16,
    pub reserved2: u8,
    pub flags: u32,
    pub reset_reg: GenericAddressStructure,
    pub reset_value: u8,
    pub reserved3: [u8; 3],
    pub x_firmware_ctrl: u64,
    pub x_dsdt: u64,
    pub x_pm1a_evt_blk: GenericAddressStructure,
    pub x_pm1b_evt_blk: GenericAddressStructure,
    pub x_pm1a_cnt_blk: GenericAddressStructure,
    pub x_pm1b_cnt_blk: GenericAddressStructure,
    pub x_pm2_cnt_blk: GenericAddressStructure,
    pub x_pm_tmr_blk: GenericAddressStructure,
    pub x_gpe0_blk: GenericAddressStructure,
    pub x_gpe1_blk: GenericAddressStructure,
}

/// Generic Address Structure used in ACPI
#[repr(C, packed)]
#[derive(Debug, Clone, Copy)]
pub struct GenericAddressStructure {
    pub address_space: u8,
    pub bit_width: u8,
    pub bit_offset: u8,
    pub access_size: u8,
    pub address: u64,
}

/// MADT (Multiple APIC Description Table) structure
#[repr(C, packed)]
#[derive(Debug)]
pub struct Madt {
    pub header: AcpiTableHeader,
    pub local_apic_addr: u32,
    pub flags: u32,
    // Followed by variable number of interrupt controller structures
}

/// MADT entry types
#[repr(u8)]
#[derive(Debug, Clone, Copy)]
pub enum MadtEntryType {
    LocalApic = 0,
    IoApic = 1,
    InterruptSourceOverride = 2,
    NmiSource = 3,
    LocalApicNmi = 4,
    LocalApicAddressOverride = 5,
    IoSapic = 6,
    LocalSapic = 7,
    PlatformInterruptSources = 8,
    LocalX2Apic = 9,
    LocalX2ApicNmi = 10,
    GicCpuInterface = 11,
    GicDistributor = 12,
    GicMsiFrame = 13,
    GicRedistributor = 14,
    GicInterruptTranslationService = 15,
}

/// MADT entry header
#[repr(C, packed)]
#[derive(Debug, Clone, Copy)]
pub struct MadtEntryHeader {
    pub entry_type: u8,
    pub length: u8,
}

/// Local APIC entry
#[repr(C, packed)]
#[derive(Debug, Clone, Copy)]
pub struct MadtLocalApicEntry {
    pub header: MadtEntryHeader,
    pub processor_id: u8,
    pub apic_id: u8,
    pub flags: u32,
}

/// I/O APIC entry
#[repr(C, packed)]
#[derive(Debug, Clone, Copy)]
pub struct MadtIoApicEntry {
    pub header: MadtEntryHeader,
    pub io_apic_id: u8,
    pub reserved: u8,
    pub io_apic_addr: u32,
    pub global_system_interrupt_base: u32,
}

/// HPET (High Precision Event Timer) structure
#[repr(C, packed)]
#[derive(Debug, Clone, Copy)]
pub struct Hpet {
    pub header: AcpiTableHeader,
    pub hardware_rev_id: u8,
    pub comparator_count: u8,
    pub counter_size: u8,
    pub reserved: u8,
    pub legacy_replacement: u8,
    pub pci_vendor_id: u16,
    pub hpet_number: u8,
    pub minimum_tick: u16,
    pub page_protection: u8,
    pub address: GenericAddressStructure,
    pub hpet_block_id: u32,
    pub period: u32,
}

/// MCFG (PCI Express memory mapped configuration) structure
#[repr(C, packed)]
#[derive(Debug)]
pub struct Mcfg {
    pub header: AcpiTableHeader,
    pub reserved: [u8; 8],
    // Followed by variable number of configuration space allocations
}

/// MCFG configuration space allocation
#[repr(C, packed)]
#[derive(Debug, Clone, Copy)]
pub struct McfgAllocation {
    pub base_address: u64,
    pub pci_segment_group: u16,
    pub start_bus: u8,
    pub end_bus: u8,
    pub reserved: [u8; 4],
}

impl Rsdt {
    /// Get the number of table entries
    pub fn entry_count(&self) -> usize {
        let length = self.header.length;
        (length as usize - core::mem::size_of::<AcpiTableHeader>()) / 4
    }

    /// Get table address at index
    pub fn table_address(&self, index: usize) -> Option<u32> {
        if index >= self.entry_count() {
            return None;
        }
        let entries = unsafe {
            core::slice::from_raw_parts(
                (self as *const Self as usize + core::mem::size_of::<AcpiTableHeader>()) as *const u32,
                self.entry_count()
            )
        };
        Some(entries[index])
    }
}

impl Xsdt {
    /// Get the number of table entries
    pub fn entry_count(&self) -> usize {
        let length = self.header.length;
        (length as usize - core::mem::size_of::<AcpiTableHeader>()) / 8
    }

    /// Get table address at index
    pub fn table_address(&self, index: usize) -> Option<u64> {
        if index >= self.entry_count() {
            return None;
        }
        let entries = unsafe {
            core::slice::from_raw_parts(
                (self as *const Self as usize + core::mem::size_of::<AcpiTableHeader>()) as *const u64,
                self.entry_count()
            )
        };
        Some(entries[index])
    }
}

/// Parse RSDT/XSDT and list all ACPI tables
pub fn parse_rsdt_xsdt(rsdp: &Rsdp, phys_offset: u64) {
    let table_addr = rsdp.table_address();
    let table_virt_addr = (table_addr + phys_offset) as *const AcpiTableHeader;
    
    let header = unsafe { &*table_virt_addr };
    
    if !header.checksum_valid() {
        println!("[ACPI] RSDT/XSDT checksum invalid!");
        return;
    }

    let sig_bytes = header.signature;
    let entry_count = if sig_bytes == *b"RSDT" {
        let rsdt = unsafe { &*(table_virt_addr as *const Rsdt) };
        rsdt.entry_count()
    } else if sig_bytes == *b"XSDT" {
        let xsdt = unsafe { &*(table_virt_addr as *const Xsdt) };
        xsdt.entry_count()
    } else {
        println!("[ACPI] Unknown RSDT/XSDT signature");
        return;
    };

    let signature = core::str::from_utf8(&sig_bytes).unwrap_or("UNKNOWN");
    println!("[ACPI] Found {} with {} entries", signature, entry_count);

    // Parse and list all tables
    if sig_bytes == *b"RSDT" {
        let rsdt = unsafe { &*(table_virt_addr as *const Rsdt) };
        for i in 0..rsdt.entry_count() {
            if let Some(table_phys_addr) = rsdt.table_address(i) {
                print_table_info(table_phys_addr as u64, phys_offset);
            }
        }
    } else if sig_bytes == *b"XSDT" {
        let xsdt = unsafe { &*(table_virt_addr as *const Xsdt) };
        for i in 0..xsdt.entry_count() {
            if let Some(table_phys_addr) = xsdt.table_address(i) {
                print_table_info(table_phys_addr, phys_offset);
            }
        }
    }
}

/// Print information about an ACPI table
fn print_table_info(table_phys_addr: u64, phys_offset: u64) {
    let table_virt_addr = (table_phys_addr + phys_offset) as *const AcpiTableHeader;
    
    // Check if address is valid (basic bounds check)
    if table_virt_addr.is_null() {
        return;
    }
    
    let header = unsafe { &*table_virt_addr };
    
    if header.checksum_valid() {
        // Copy packed fields to avoid alignment issues
        let signature = header.signature;
        let length = header.length;
        
        println!("[ACPI] Table: {} at {:#x} ({} bytes)", 
                 core::str::from_utf8(&signature).unwrap_or("INVALID"), 
                 table_phys_addr,
                 length);

        // Parse specific table types
        parse_specific_table(&signature, table_phys_addr, phys_offset);
    } else {
        println!("[ACPI] Invalid table at {:#x}", table_phys_addr);
    }
}

/// Parse specific ACPI table types
fn parse_specific_table(signature: &[u8; 4], table_phys_addr: u64, phys_offset: u64) {
    let table_virt_addr = (table_phys_addr + phys_offset) as *const u8;
    
    match signature {
        b"FACP" => parse_facp(table_virt_addr),
        b"APIC" => parse_madt(table_virt_addr),
        b"HPET" => parse_hpet(table_virt_addr),
        b"MCFG" => parse_mcfg(table_virt_addr),
        _ => {} // Unknown table type, skip parsing
    }
}

/// Parse FACP (Fixed ACPI Description Table)
fn parse_facp(table_ptr: *const u8) {
    let facp = unsafe { &*(table_ptr as *const Facp) };
    
    // Copy packed fields to avoid alignment issues
    let firmware_ctrl = facp.firmware_ctrl;
    let dsdt = facp.dsdt;
    let sci_int = facp.sci_int;
    let smi_cmd = facp.smi_cmd;
    let acpi_enable = facp.acpi_enable;
    let acpi_disable = facp.acpi_disable;
    let pm1a_evt_blk = facp.pm1a_evt_blk;
    let pm1a_cnt_blk = facp.pm1a_cnt_blk;
    let pm_tmr_blk = facp.pm_tmr_blk;
    let iapc_boot_arch = facp.iapc_boot_arch;
    let flags = facp.flags;
    let x_firmware_ctrl = facp.x_firmware_ctrl;
    let x_dsdt = facp.x_dsdt;
    
    println!("  [FACP] Firmware Control: {:#x}", firmware_ctrl);
    println!("  [FACP] DSDT: {:#x}", dsdt);
    println!("  [FACP] SCI Interrupt: {}", sci_int);
    println!("  [FACP] SMI Command: {:#x}", smi_cmd);
    println!("  [FACP] ACPI Enable: {:#x}", acpi_enable);
    println!("  [FACP] ACPI Disable: {:#x}", acpi_disable);
    println!("  [FACP] PM1a Event Block: {:#x}", pm1a_evt_blk);
    println!("  [FACP] PM1a Control Block: {:#x}", pm1a_cnt_blk);
    println!("  [FACP] PM Timer Block: {:#x}", pm_tmr_blk);
    println!("  [FACP] Boot Architecture Flags: {:#x}", iapc_boot_arch);
    println!("  [FACP] Flags: {:#x}", flags);
    
    // Check for extended addresses (ACPI 2.0+)
    if facp.header.revision >= 2 {
        println!("  [FACP] X_Firmware Control: {:#x}", x_firmware_ctrl);
        println!("  [FACP] X_DSDT: {:#x}", x_dsdt);
    }
}

/// Parse MADT (Multiple APIC Description Table)
fn parse_madt(table_ptr: *const u8) {
    let madt = unsafe { &*(table_ptr as *const Madt) };
    
    // Copy packed fields to avoid alignment issues
    let local_apic_addr = madt.local_apic_addr;
    let flags = madt.flags;
    
    println!("  [MADT] Local APIC Address: {:#x}", local_apic_addr);
    println!("  [MADT] Flags: {:#x}", flags);
    
    // Parse MADT entries
    let entries_start = table_ptr as usize + core::mem::size_of::<Madt>();
    let entries_end = table_ptr as usize + madt.header.length as usize;
    
    let mut offset = entries_start;
    while offset < entries_end {
        let entry_header = unsafe { &*(offset as *const MadtEntryHeader) };
        let entry_type = entry_header.entry_type;
        let length = entry_header.length;
        
        match entry_type {
            0 => { // Local APIC
                let entry = unsafe { &*(offset as *const MadtLocalApicEntry) };
                let processor_id = entry.processor_id;
                let apic_id = entry.apic_id;
                let flags = entry.flags;
                println!("    [MADT] Local APIC - Processor ID: {}, APIC ID: {}, Flags: {:#x}",
                         processor_id, apic_id, flags);
            }
            1 => { // I/O APIC
                let entry = unsafe { &*(offset as *const MadtIoApicEntry) };
                let io_apic_id = entry.io_apic_id;
                let io_apic_addr = entry.io_apic_addr;
                let global_system_interrupt_base = entry.global_system_interrupt_base;
                println!("    [MADT] I/O APIC - ID: {}, Address: {:#x}, Global System Interrupt Base: {}",
                         io_apic_id, io_apic_addr, global_system_interrupt_base);
            }
            _ => {
                println!("    [MADT] Unknown entry type: {}", entry_type);
            }
        }
        
        offset += length as usize;
    }
}

/// Parse HPET (High Precision Event Timer)
fn parse_hpet(table_ptr: *const u8) {
    let hpet = unsafe { &*(table_ptr as *const Hpet) };
    
    // Copy packed fields to avoid alignment issues
    let hardware_rev_id = hpet.hardware_rev_id;
    let comparator_count = hpet.comparator_count;
    let counter_size = hpet.counter_size;
    let legacy_replacement = hpet.legacy_replacement;
    let pci_vendor_id = hpet.pci_vendor_id;
    let hpet_number = hpet.hpet_number;
    let minimum_tick = hpet.minimum_tick;
    let address = hpet.address;
    let hpet_block_id = hpet.hpet_block_id;
    let period = hpet.period;
    
    // Copy address structure fields to avoid alignment issues
    let address_address = address.address;
    let address_space = address.address_space;
    
    println!("  [HPET] Hardware Revision ID: {}", hardware_rev_id);
    println!("  [HPET] Comparator Count: {}", comparator_count);
    println!("  [HPET] Counter Size: {} bit", if counter_size == 1 { 64 } else { 32 });
    println!("  [HPET] Legacy Replacement: {}", legacy_replacement);
    println!("  [HPET] PCI Vendor ID: {:#x}", pci_vendor_id);
    println!("  [HPET] HPET Number: {}", hpet_number);
    println!("  [HPET] Minimum Tick: {}", minimum_tick);
    println!("  [HPET] Address: {:#x} (Space: {})", 
             address_address, address_space);
    println!("  [HPET] HPET Block ID: {:#x}", hpet_block_id);
    println!("  [HPET] Period: {} fs", period);
}

/// Parse MCFG (PCI Express memory mapped configuration)
fn parse_mcfg(table_ptr: *const u8) {
    let mcfg = unsafe { &*(table_ptr as *const Mcfg) };
    
    println!("  [MCFG] PCI Express Configuration");
    
    // Parse configuration space allocations
    let allocations_start = table_ptr as usize + core::mem::size_of::<Mcfg>();
    let allocations_end = table_ptr as usize + mcfg.header.length as usize;
    
    let mut offset = allocations_start;
    let mut alloc_count = 0;
    
    while offset + core::mem::size_of::<McfgAllocation>() <= allocations_end {
        let allocation = unsafe { &*(offset as *const McfgAllocation) };
        
        // Copy packed fields to avoid alignment issues
        let base_address = allocation.base_address;
        let pci_segment_group = allocation.pci_segment_group;
        let start_bus = allocation.start_bus;
        let end_bus = allocation.end_bus;
        
        println!("    [MCFG] Allocation {}: Base {:#x}, Segment {}, Buses {}-{}",
                 alloc_count,
                 base_address,
                 pci_segment_group,
                 start_bus,
                 end_bus);
        
        offset += core::mem::size_of::<McfgAllocation>();
        alloc_count += 1;
    }
}