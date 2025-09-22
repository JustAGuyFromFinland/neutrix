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
    } else {
        println!("[ACPI] Invalid table at {:#x}", table_phys_addr);
    }
}