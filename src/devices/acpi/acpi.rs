use crate::*;
use core::str;
use core::ptr;
use x86_64::VirtAddr;
use core::sync::atomic::{AtomicU64, Ordering};
use alloc::vec::Vec;
use spin::Mutex;
use crate::driver_framework::manager::GLOBAL_MANAGER;
use crate::driver_framework::device::{DeviceInfo, Resource, ResourceKind, Capability};
// note: `core::ptr` already imported above

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
    if let Some(rsdp) = find_rsdp(phys_offset.as_u64()) {
        // Parse RSDT/XSDT and list all tables
        parse_rsdt_xsdt(rsdp, phys_offset.as_u64());
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
        return;
    };

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
        // Copy packed fields to locals to avoid unaligned references
        let mut signature = [0u8; 4];
        signature.copy_from_slice(&header.signature);
        let length = header.length as u64;

        // Register a generic device representing this ACPI table so the
        // device manager is aware of ACPI-provided components.
        let table_desc = alloc::format!("ACPI table: {}", header.signature_str());
        let info = DeviceInfo {
            vendor_id: 0xffff,
            device_id: 0xffff,
            class: 0xFF, // vendor/system-specific
            subclass: 0x00,
            prog_if: 0x00,
            resources: {
                let mut v = Vec::new();
                v.push(Resource { kind: ResourceKind::MemoryMapped, addr: table_phys_addr, len: length });
                v
            },
            capabilities: Vec::new(),
            description: table_desc,
        };
        let id = GLOBAL_MANAGER.register_device(info);
        println!("ACPI: registered table device id={} sig={:?} @ {:#x}", id, signature, table_phys_addr);

        // Parse specific table types
        parse_specific_table(&signature, table_phys_addr, phys_offset);
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

    // Enable ACPI using the FACP information
    enable_acpi(facp);
}

/// Parse MADT (Multiple APIC Description Table)
fn parse_madt(table_ptr: *const u8) {
    // Parse MADT and store useful information such as the Local APIC base address
    if table_ptr.is_null() {
        return;
    }
    let madt = unsafe { &*(table_ptr as *const Madt) };
    // Validate checksum
    if !madt.header.checksum_valid() {
        return;
    }

    // Store the local APIC address for other subsystems to use
    MADT_LOCAL_APIC_ADDR.store(madt.local_apic_addr as u64, Ordering::SeqCst);

    // Parse MADT entries that follow the header: variable-length structures
    let table_len = madt.header.length as usize;
    let mut offset = core::mem::size_of::<Madt>();
    while offset < table_len {
        let entry_ptr = unsafe { table_ptr.add(offset) } as *const MadtEntryHeader;
        if entry_ptr.is_null() {
            break;
        }
        let entry_header = unsafe { &*entry_ptr };
        let entry_len = entry_header.length as usize;
        if entry_len == 0 {
            break; // malformed
        }

        match entry_header.entry_type {
            1 => {
                // IO APIC
                if entry_len >= core::mem::size_of::<MadtIoApicEntry>() {
                    let ioapic = unsafe { &*(entry_ptr as *const MadtIoApicEntry) };
                    // Copy packed fields to locals to avoid unaligned references
                    let apic_id = ioapic.io_apic_id;
                    let apic_addr = ioapic.io_apic_addr;
                    let gsi_base = ioapic.global_system_interrupt_base;

                    IOAPICS.lock().push(IoApicInfo {
                        id: apic_id,
                        addr: apic_addr,
                        gsi_base: gsi_base,
                    });
                    // Register IOAPIC as a device so drivers/consumers can bind to it.
                    let info = DeviceInfo {
                        vendor_id: 0xffff,
                        device_id: apic_id as u16,
                        class: 0x08, // Base System Peripheral
                        subclass: 0x00,
                        prog_if: 0x00,
                        resources: {
                            let mut v = Vec::new();
                            // IOAPIC registers are typically MMIO; length is conservative
                            v.push(Resource { kind: ResourceKind::MemoryMapped, addr: apic_addr as u64, len: 0x100 });
                            v
                        },
                        capabilities: Vec::new(),
                        description: alloc::format!("ACPI IOAPIC id={} gsi_base={}", apic_id, gsi_base),
                    };
                    let id = GLOBAL_MANAGER.register_device(info);
                    println!("ACPI: registered IOAPIC device id={} apic_id={} gsi_base={} @ {:#x}", id, apic_id, gsi_base, apic_addr);
                }
            }
            2 => {
                // Interrupt Source Override
                if entry_len >= core::mem::size_of::<MadtInterruptSourceOverride>() {
                    let iso = unsafe { &*(entry_ptr as *const MadtInterruptSourceOverride) };
                    ISOS.lock().push(IsoInfo {
                        bus: iso.bus,
                        source: iso.source,
                        gsi: iso.gsi,
                        flags: iso.flags,
                    });
                }
            }
            _ => {
                // Other MADT entries currently ignored
            }
        }

        offset += entry_len;
    }
}

// Atomic holder for the MADT local APIC address (0 = unknown/not set)
static MADT_LOCAL_APIC_ADDR: AtomicU64 = AtomicU64::new(0);

/// Return the Local APIC physical address discovered from the MADT, if any
pub fn get_local_apic_address() -> Option<u32> {
    let v = MADT_LOCAL_APIC_ADDR.load(Ordering::SeqCst);
    if v == 0 {
        None
    } else {
        Some(v as u32)
    }
}

// --- IOAPIC / ISO storage and types ---
#[derive(Debug, Clone, Copy)]
pub struct IoApicInfo {
    pub id: u8,
    pub addr: u32,
    pub gsi_base: u32,
}

#[derive(Debug, Clone, Copy)]
pub struct IsoInfo {
    pub bus: u8,
    pub source: u8,
    pub gsi: u32,
    pub flags: u16,
}

static IOAPICS: Mutex<Vec<IoApicInfo>> = Mutex::new(Vec::new());
static ISOS: Mutex<Vec<IsoInfo>> = Mutex::new(Vec::new());

// Store discovered HPET base address and period (femtoseconds)
static HPET_BASE: Mutex<Option<u64>> = Mutex::new(None);
static HPET_PERIOD_FS: AtomicU64 = AtomicU64::new(0);

/// Return HPET base physical address if discovered
pub fn get_hpet_address() -> Option<u64> {
    HPET_BASE.lock().clone()
}

/// Return HPET period in femtoseconds (as discovered from ACPI), or 0 if unknown
pub fn get_hpet_period_fs() -> u64 {
    HPET_PERIOD_FS.load(Ordering::SeqCst)
}

/// Return a cloned list of discovered IOAPICs (id, addr, gsi_base)
pub fn get_ioapics() -> Vec<IoApicInfo> {
    IOAPICS.lock().clone()
}

/// Return a cloned list of Interrupt Source Overrides
pub fn get_isos() -> Vec<IsoInfo> {
    ISOS.lock().clone()
}

/// Packed MADT Interrupt Source Override structure
#[repr(C, packed)]
#[derive(Debug, Clone, Copy)]
pub struct MadtInterruptSourceOverride {
    pub header: MadtEntryHeader,
    pub bus: u8,
    pub source: u8,
    pub gsi: u32,
    pub flags: u16,
}

/// Parse HPET (High Precision Event Timer)
fn parse_hpet(_table_ptr: *const u8) {
    if _table_ptr.is_null() {
        return;
    }
    // Safe read of HPET header and GenericAddressStructure (packed)
    let hpet = unsafe { &*(_table_ptr as *const Hpet) };
    if !hpet.header.checksum_valid() {
        return;
    }
    // Copy GAS (GenericAddressStructure) using read_unaligned to avoid UB
    let gas_ptr = unsafe { &hpet.address as *const GenericAddressStructure } as *const GenericAddressStructure;
    let gas = unsafe { ptr::read_unaligned(gas_ptr) };
    if gas.address != 0 {
        let addr = gas.address;
        // store discovered HPET base and period for other subsystems
        HPET_BASE.lock().replace(addr);
        HPET_PERIOD_FS.store(hpet.period as u64, Ordering::SeqCst);
        let info = DeviceInfo {
            vendor_id: 0xffff,
            device_id: 0xffff,
            class: 0x04, // Multimedia/Timer
            subclass: 0x00,
            prog_if: 0x00,
            resources: {
                let mut v = Vec::new();
                v.push(Resource { kind: ResourceKind::MemoryMapped, addr: addr, len: 0x1000 });
                v
            },
            capabilities: Vec::new(),
            description: alloc::format!("ACPI HPET @ {:#x}", addr),
        };
        let id = GLOBAL_MANAGER.register_device(info);
        println!("ACPI: registered HPET device id={} @ {:#x}", id, addr);
    }
}

/// Parse MCFG (PCI Express memory mapped configuration)
fn parse_mcfg(_table_ptr: *const u8) {
    // Parse MCFG to register PCI ECAM regions as devices so the device
    // manager knows about PCI root segments discovered via ACPI.
    if _table_ptr.is_null() {
        return;
    }
    // The MCFG header is followed by one or more McfgAllocation entries.
    let header = unsafe { &*(_table_ptr as *const Mcfg) };
    let total_len = header.header.length as usize;
    let mut offset = core::mem::size_of::<Mcfg>();
    while offset + core::mem::size_of::<McfgAllocation>() <= total_len {
        let alloc_ptr = unsafe { _table_ptr.add(offset) } as *const McfgAllocation;
        let alloc = unsafe { &*alloc_ptr };
        // copy fields to locals to avoid packed/unaligned access issues
        let base = alloc.base_address;
        let seg = alloc.pci_segment_group;
        let start_bus = alloc.start_bus;
        let end_bus = alloc.end_bus;

        // Register a device representing this PCI ECAM region
        let info = DeviceInfo {
            vendor_id: 0xffff,
            device_id: seg,
            class: 0x06, // Bridge / system
            subclass: 0x00,
            prog_if: 0x00,
            resources: {
                let mut v = Vec::new();
                v.push(Resource { kind: ResourceKind::MemoryMapped, addr: base, len: 0 });
                v
            },
            capabilities: Vec::new(),
            description: alloc::format!("ACPI MCFG ECAM seg={} buses={}..{} @ {:#x}", seg, start_bus, end_bus, base),
        };
        let id = GLOBAL_MANAGER.register_device(info);
        // push to global MCFG list for later ECAM-based PCI scanning
        MCFG_ALLOCS.lock().push(*alloc);
        println!("ACPI: registered MCFG ECAM id={} seg={} buses={}..{} @ {:#x}", id, seg, start_bus, end_bus, base);

        offset += core::mem::size_of::<McfgAllocation>();
    }
}

/// Enable ACPI by disabling legacy power management and enabling ACPI mode
/// This function should be called after parsing the FACP table
pub fn enable_acpi(facp: &Facp) {
    use crate::arch::ports::{outb, inb};

    // Copy packed fields to avoid alignment issues
    let smi_cmd = facp.smi_cmd;
    let acpi_enable = facp.acpi_enable;
    let acpi_disable = facp.acpi_disable;
    let pm1a_cnt_blk = facp.pm1a_cnt_blk;

    // Ensure ACPI is disabled first (write disable value to SMI command port)
    if smi_cmd != 0 && acpi_disable != 0 {
        unsafe { outb(smi_cmd as u16, acpi_disable) };
    }

    // Small delay to let the disable command take effect
    for _ in 0..10000 {
        unsafe { core::arch::asm!("nop") };
    }

    // Enable ACPI (write enable value to SMI command port)
    if smi_cmd != 0 && acpi_enable != 0 {
        unsafe { outb(smi_cmd as u16, acpi_enable) };
    }

    // Wait for ACPI to be enabled by checking SCI_EN bit in PM1 control register
    // SCI_EN is typically bit 0 in the PM1 control register
    if pm1a_cnt_blk != 0 {
        // Wait up to 3 seconds for ACPI to enable
        for _ in 0..3000000 {
            let pm1a_cnt = unsafe { inb(pm1a_cnt_blk as u16) };
            if (pm1a_cnt & 0x01) != 0 { // SCI_EN bit set
                return; // Success
            }
            // Small delay
            for _ in 0..10 {
                unsafe { core::arch::asm!("nop") };
            }
        }
        // Timeout - ACPI may not be enabled
    }
}

// Store MCFG allocations discovered from ACPI so other subsystems (PCI) can use ECAM ranges
static MCFG_ALLOCS: Mutex<Vec<McfgAllocation>> = Mutex::new(Vec::new());

/// Return a cloned list of MCFG allocations discovered by ACPI.
pub fn get_mcfg_allocs() -> Vec<McfgAllocation> {
    MCFG_ALLOCS.lock().clone()
}