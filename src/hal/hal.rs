//! Hardware Abstraction Layer Implementation
//!
//! Provides high-level functions for CPU and ACPI initialization.

use x86_64::VirtAddr;
use crate::*;

/// CPU feature information returned by initialization
#[derive(Debug, Clone)]
pub struct CpuInfo {
    pub vendor: [u8; 12],
    pub features: crate::arch::processor::CpuFeatures,
}

/// ACPI initialization result
#[derive(Debug)]
pub enum AcpiStatus {
    /// ACPI successfully initialized and enabled
    Enabled,
    /// ACPI tables found but enable failed
    TablesFound,
    /// ACPI not available on this system
    NotAvailable,
}

/// Initialize all CPU features
/// Returns information about detected CPU features
pub fn init_cpu() -> CpuInfo {
    println!("[HAL] Initializing CPU features...");

    // Enable SSE (required for most modern operations)
    crate::enable_sse();

    // Detect CPU features
    let features = crate::arch::detect_cpu_features();
    println!("[HAL] CPU Vendor: {}", core::str::from_utf8(&features.vendor).unwrap_or("Unknown"));

    // Enable detected features
    crate::arch::enable_cpu_features(&features);

    println!("[HAL] CPU features initialized successfully");

    CpuInfo {
        vendor: features.vendor,
        features,
    }
}

/// Initialize ACPI subsystem
/// Returns the status of ACPI initialization
pub fn init_acpi(phys_offset: VirtAddr) -> AcpiStatus {
    println!("[HAL] Initializing ACPI subsystem...");

    // Check if ACPI is available
    if crate::devices::acpi::find_rsdp(phys_offset.as_u64()).is_some() {
        // Initialize ACPI (this will also enable it via FACP parsing)
        crate::devices::acpi::init_with_offset(phys_offset);
        println!("[HAL] ACPI initialized and enabled");
        AcpiStatus::Enabled
    } else {
        println!("[HAL] ACPI not available on this system");
        AcpiStatus::NotAvailable
    }
}

/// Initialize essential hardware components
/// This is the main HAL initialization function that should be called early in kernel boot
pub fn init_hardware(phys_offset: VirtAddr) -> (CpuInfo, AcpiStatus) {
    println!("[HAL] =========================================");
    println!("[HAL] Hardware Abstraction Layer Initialization");
    println!("[HAL] =========================================");

    // Initialize CPU features first
    let cpu_info = init_cpu();

    // Initialize ACPI
    let acpi_status = init_acpi(phys_offset);

    // Initialize Local APIC if possible
    if crate::hal::apic::init_from_acpi(phys_offset) {
        println!("[HAL] Local APIC initialized");
    } else {
        println!("[HAL] Local APIC not initialized or not present");
    }

    // Initialize IOAPICs discovered via ACPI MADT (if any)
    crate::hal::ioapic::init_from_acpi(phys_offset);
    // Apply Interrupt Source Overrides (ISOs) to IOAPIC configuration (logged/stubbed)
    crate::hal::ioapic::apply_isos_from_acpi(phys_offset);
    // As a fallback, ensure legacy ISA IRQs 0..15 are programmed into IOAPIC
    // to the conventional vectors (masked). This makes keyboard (IRQ1)
    // deliver to vector 0x21 when APIC-only interrupts are used.
    crate::hal::ioapic::apply_legacy_isa_fallback(phys_offset);

    println!("[HAL] =========================================");
    println!("[HAL] Hardware initialization complete");
    println!("[HAL] =========================================");

    (cpu_info, acpi_status)
}

/// Check if ACPI is available on this system
pub fn is_acpi_available(phys_offset: VirtAddr) -> bool {
    crate::devices::acpi::find_rsdp(phys_offset.as_u64()).is_some()
}

/// Get CPU vendor string as a readable string
pub fn get_cpu_vendor_string(vendor: &[u8; 12]) -> &str {
    core::str::from_utf8(vendor).unwrap_or("Unknown")
}