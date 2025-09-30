use alloc::boxed::Box;
use alloc::string::String;
use alloc::vec::Vec;
use core::fmt;
use spin::Mutex;
use alloc::format;

/// Representation of resources a device may expose (BARs, IRQs, IO ports)
#[derive(Debug, Clone)]
pub struct Resource {
	pub kind: ResourceKind,
	pub addr: u64,
	pub len: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResourceKind {
	MemoryMapped,
	IO,
	Interrupt(u8),
	/// MSI (Message Signaled Interrupts): number of vectors supported
	/// msg_addr is the canonical 64-bit message address to write to
	/// msg_data is the 16-bit payload value to write
	Msi { vectors: u8, addr64: bool, maskable: bool, msg_addr: u64, msg_data: u16 },
	/// MSI-X: table is located in BAR `table_bar` at `table_offset`, table_size entries
	Msix { table_bar: u8, table_offset: u32, table_size: u16, table_present: bool, first_entry_masked: bool },
}

/// Parsed capability entries from the PCI capability list.
#[derive(Clone, Debug)]
pub enum Capability {
	PowerManagement { pm_cap: u16, pmcsr: u16 },
	PciExpress { header: u32, device_cap: u32 },
	Other { id: u8, raw0: u32, raw1: u32 },
}

/// Portable device information. Drivers should use this to probe and attach.
#[derive(Clone)]
pub struct DeviceInfo {
	pub vendor_id: u16,
	pub device_id: u16,
	pub class: u8,
	pub subclass: u8,
	pub prog_if: u8,
	pub resources: Vec<Resource>,
	pub capabilities: Vec<Capability>,
	pub description: String,
}

impl fmt::Debug for DeviceInfo {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		write!(f, "DeviceInfo {{ {:04x}:{:04x} class={:02x}/{:02x} desc='{}' res={} caps={} }}",
			self.vendor_id, self.device_id, self.class, self.subclass, self.description, self.resources.len(), self.capabilities.len())
	}
}

/// A device handle given to drivers. It owns the DeviceInfo and provides
/// controlled operations like claim/release.
pub struct Device {
	pub id: usize,
	pub info: Mutex<DeviceInfo>,
}

impl Device {
	pub fn new(id: usize, info: DeviceInfo) -> Self {
		Device { id, info: Mutex::new(info) }
	}

	pub fn id(&self) -> usize { self.id }

	pub fn info(&self) -> DeviceInfo {
		self.info.lock().clone()
	}

	/// Return all MSI resources for this device (cloned). Useful for drivers to
	/// quickly locate MSI configuration (msg_addr/msg_data) without walking
	/// resources manually.
	pub fn msi_resources(&self) -> Vec<Resource> {
		let info = self.info.lock();
		info.resources.iter().filter(|r| matches!(r.kind, ResourceKind::Msi { .. })).cloned().collect()
	}

	/// Return all MSI-X resources for this device (cloned).
	pub fn msix_resources(&self) -> Vec<Resource> {
		let info = self.info.lock();
		info.resources.iter().filter(|r| matches!(r.kind, ResourceKind::Msix { .. })).cloned().collect()
	}
}

pub type DeviceHandle = Box<Device>;

/// Convert PCI class/subclass into a human-readable string. This covers
/// common classes; unknown combinations fall back to a hex description.
pub fn class_subclass_to_string(class: u8, subclass: u8, prog_if: u8) -> String {
    // helper for Ethernet prog-if values
    fn ethernet_prog_if_to_str(prog_if: u8) -> &'static str {
        match prog_if {
            0x00 => "Ethernet: Ethernet controller",
            0x01 => "Ethernet: IEEE 802.3u (100BASE-TX)",
            0x02 => "Ethernet: IEEE 802.3ab (1000BASE-T)",
            _ => "Ethernet: other/prog-if",
        }
    }

    // helper for USB prog-if under Serial Bus class/subclass
    fn usb_prog_if_to_str(prog_if: u8) -> &'static str {
        match prog_if {
            0x00 => "USB: UHCI (Universal Host Controller)",
            0x10 => "USB: OHCI (Open Host Controller)",
            0x20 => "USB: EHCI (Enhanced Host Controller)",
            0x30 => "USB: XHCI (Extensible Host Controller)",
            _ => "USB: other/prog-if",
        }
    }

    match class {
		0x00 => match subclass {
			0x00 => String::from("Unclassified: Non-VGA-compatible device"),
			0x01 => String::from("Unclassified: VGA-compatible device"),
			s => format!("Unclassified subclass 0x{:02x}", s),
		},
		0x01 => match subclass {
			0x00 => String::from("Mass Storage: SCSI"),
			0x01 => String::from("Mass Storage: IDE"),
			0x02 => String::from("Mass Storage: Floppy"),
			0x06 => {
				// SATA - try to interpret prog-if (e.g., AHCI)
				match prog_if {
					0x00 => String::from("Mass Storage: SATA (vendor/legacy)"),
					0x01 => String::from("Mass Storage: SATA (AHCI)") ,
					_ => format!("Mass Storage: SATA prog-if 0x{:02x}", prog_if),
				}
			}
			s => format!("Mass Storage subclass 0x{:02x}", s),
		},
		0x02 => match subclass {
			0x00 => format!("{}", ethernet_prog_if_to_str(prog_if)),
			0x80 => String::from("Network: Other"),
			s => format!("Network subclass 0x{:02x}", s),
		},
		0x03 => match subclass {
			0x00 => {
				// Display controller (VGA-compatible) - use prog-if to be more specific when possible
				match prog_if {
					0x00 => String::from("Display: VGA-compatible controller"),
					0x01 => String::from("Display: 3D controller (OpenGL)"),
					_ => String::from("Display: VGA-compatible controller"),
				}
			}
			0x80 => String::from("Display: Other"),
			s => format!("Display subclass 0x{:02x}", s),
		},
		0x04 => match subclass {
			0x00 => String::from("Multimedia: Video"),
			0x01 => String::from("Multimedia: Audio"),
			s => format!("Multimedia subclass 0x{:02x}", s),
		},
		0x05 => String::from("Memory Controller"),
		0x06 => match subclass {
			0x00 => String::from("Bridge: Host"),
			0x01 => String::from("Bridge: ISA"),
			0x04 => String::from("Bridge: PCI-to-PCI"),
			s => format!("Bridge subclass 0x{:02x}", s),
		},
		0x07 => String::from("Simple Communication Controller"),
		0x08 => String::from("Base System Peripheral"),
		0x09 => String::from("Input Device"),
		0x0A => String::from("Docking Station"),
		0x0B => String::from("Processor"),
		0x0C => match subclass {
			0x03 => format!("{}", usb_prog_if_to_str(prog_if)),
			0x05 => String::from("Serial Bus: SMBus"),
			s => format!("Serial Bus subclass 0x{:02x}", s),
		},
		0xFF => String::from("Unknown / Vendor-specific"),
		c => format!("Class 0x{:02x} subclass 0x{:02x}", c, subclass),
	}
}
