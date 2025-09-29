use alloc::boxed::Box;
use alloc::vec::Vec;
use alloc::format;
use alloc::string::String;
use spin::Mutex;
use core::sync::atomic::{AtomicUsize, Ordering};
use crate::driver_framework::device::{Device, DeviceHandle, DeviceInfo};
use crate::driver_framework::driver::{DriverBox};
pub use crate::*;

static NEXT_DEVICE_ID: AtomicUsize = AtomicUsize::new(1);

/// Simple registry entry for devices.
pub struct RegistryEntry {
	pub device: DeviceHandle,
	pub driver: Option<DriverBox>,
}

pub struct DeviceManager {
	pub devices: Mutex<Vec<RegistryEntry>>,
}

impl DeviceManager {
	pub const fn new() -> Self {
		DeviceManager { devices: Mutex::new(Vec::new()) }
	}

	/// Allocate and register a new device from DeviceInfo. Returns the
	/// assigned device id.
	pub fn register_device(&self, info: DeviceInfo) -> usize {
		let id = NEXT_DEVICE_ID.fetch_add(1, Ordering::SeqCst);
		let dev = Box::new(Device::new(id, info));
		let entry = RegistryEntry { device: dev, driver: None };
		self.devices.lock().push(entry);
		id
	}

	/// Attach a driver to a device id. The manager calls probe, then start.
	pub fn attach_driver(&self, device_id: usize, driver: DriverBox) -> Result<(), String> {
		let mut devices = self.devices.lock();
		if let Some(entry) = devices.iter_mut().find(|e| e.device.id == device_id) {
			if entry.driver.is_some() {
				return Err(format!("device {} already has a driver", device_id));
			}
			// Call probe
			match driver.probe(&entry.device) {
				Ok(()) => {
					// start device
					match driver.start(&entry.device) {
						Ok(()) => {
							entry.driver = Some(driver);
							Ok(())
						}
						Err(e) => Err(format!("start failed: {}", e)),
					}
				}
				Err(e) => Err(format!("probe failed: {}", e)),
			}
		} else {
			Err(format!("no device with id {}", device_id))
		}
	}

	/// Detach driver from device and call release.
	pub fn detach_driver(&self, device_id: usize) -> Result<(), String> {
		let mut devices = self.devices.lock();
		if let Some(entry) = devices.iter_mut().find(|e| e.device.id == device_id) {
			if let Some(driver) = entry.driver.take() {
				driver.stop(&entry.device);
				driver.release(&entry.device);
				Ok(())
			} else {
				Err(format!("device {} has no driver", device_id))
			}
		} else {
			Err(format!("no device with id {}", device_id))
		}
	}

	/// Find devices by vendor/device id; returns a vector of ids.
	pub fn find_by_vid_pid(&self, vendor: u16, device: u16) -> Vec<usize> {
		let devices = self.devices.lock();
		devices.iter()
			.filter(|e| {
				let info = e.device.info.lock();
				info.vendor_id == vendor && info.device_id == device
			})
			.map(|e| e.device.id)
			.collect()
	}

	/// Provides a debug listing
	pub fn list_devices(&self) {
		let devices = self.devices.lock();
		println!("DeviceManager: {} devices registered", devices.len());
		for e in devices.iter() {
			let info = e.device.info.lock();
			let class_str = crate::driver_framework::device::class_subclass_to_string(info.class, info.subclass, info.prog_if);
			// Truncate description to 32 chars for neat output
			let mut desc = info.description.clone();
			if desc.len() > 32 {
				desc.truncate(29);
				desc.push_str("...");
			}
			// Build resource string compactly
			let mut res_str = alloc::string::String::new();
			for r in info.resources.iter() {
				match r.kind {
					crate::driver_framework::device::ResourceKind::MemoryMapped => {
						res_str.push_str(&alloc::format!("MMIO@{:#x}:{:#x} ", r.addr, r.len));
					}
					crate::driver_framework::device::ResourceKind::IO => {
						res_str.push_str(&alloc::format!("IO@{:#x}:{:#x} ", r.addr, r.len));
					}
					crate::driver_framework::device::ResourceKind::Interrupt(l) => {
						res_str.push_str(&alloc::format!("IRQ@{} ", l));
					}
					crate::driver_framework::device::ResourceKind::Msi { vectors, .. } => {
						res_str.push_str(&alloc::format!("MSI(v{}) ", vectors));
					}
					crate::driver_framework::device::ResourceKind::Msix { table_bar, table_offset, table_size, table_present, first_entry_masked } => {
						res_str.push_str(&alloc::format!("MSI-X[bar{}@{:#x},n={},present={},masked={}] ", table_bar, table_offset, table_size, table_present, first_entry_masked));
					}
				}
			}
			// Build capability summary
			let mut cap_str = alloc::string::String::new();
			for c in info.capabilities.iter() {
				match c {
					crate::driver_framework::device::Capability::PowerManagement { .. } => cap_str.push_str("PM "),
					crate::driver_framework::device::Capability::PciExpress { .. } => cap_str.push_str("PCIe "),
					crate::driver_framework::device::Capability::Other { id, .. } => cap_str.push_str(&alloc::format!("C{:02x} ", id)),
				}
			}
			println!(" - id={:>3} {:04x}:{:04x} | {:30} | {:32} | {} caps={} driver={}",
					e.device.id,
					info.vendor_id,
					info.device_id,
					class_str,
					desc,
					res_str,
					cap_str,
					if e.driver.is_some() {"yes"} else {"no"});
		}
	}
}

use lazy_static::lazy_static;

lazy_static! {
	pub static ref GLOBAL_MANAGER: DeviceManager = DeviceManager::new();
}
