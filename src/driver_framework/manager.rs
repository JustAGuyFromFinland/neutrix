use alloc::boxed::Box;
use alloc::vec::Vec;
use alloc::format;
use alloc::string::String;
use spin::Mutex;
use core::sync::atomic::{AtomicUsize, Ordering};
use crate::driver_framework::device::{Device, DeviceHandle, DeviceInfo};
use crate::driver_framework::driver::{DriverBox};
pub use crate::*;
use crate::alloc::string::ToString;

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

	/// Merge `info` into an existing device with the same vendor/device id if found.
	/// Returns Some(device_id) if merged, or None if no matching device exists.
	pub fn merge_or_register(&self, info: DeviceInfo) -> Option<usize> {
		let mut devices = self.devices.lock();
		if let Some(entry) = devices.iter_mut().find(|e| {
			let i = e.device.info.lock();
			i.vendor_id == info.vendor_id && i.device_id == info.device_id
		}) {
			// Merge resources and capabilities that are missing
			let mut existing = entry.device.info.lock();
			for r in info.resources.iter() {
				if !existing.resources.iter().any(|er| er.kind == r.kind && er.addr == r.addr) {
					existing.resources.push(r.clone());
				}
			}
			for c in info.capabilities.iter() {
				// naive dedupe by debug representation
				if !existing.capabilities.iter().any(|ec| format!("{:?}", ec) == format!("{:?}", c)) {
					existing.capabilities.push(c.clone());
				}
			}
			// Append to description if missing parts
			if !existing.description.contains(&info.description) {
				existing.description = alloc::format!("{}; {}", existing.description, info.description);
			}
			return Some(entry.device.id);
		}
		None
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
			// Map PCI class/subclass/prog_if to a concise canonical type name.
			let concise_type = {
				let c = info.class;
				let s = info.subclass;
				let p = info.prog_if;
				match c {
					0x00 => "Unclassified".to_string(),
					0x01 => match s {
						0x00 => "SCSI controller".to_string(),
						0x01 => "IDE controller".to_string(),
						0x02 => "Floppy controller".to_string(),
						0x06 => {
							// SATA - use prog_if for AHCI
							match p {
								0x01 => "SATA (AHCI)".to_string(),
								_ => "SATA controller".to_string(),
							}
						}
						_ => "Mass storage controller".to_string(),
					},
					0x02 => match s {
						0x00 => "Ethernet controller".to_string(),
						0x80 => "Network controller (other)".to_string(),
						_ => "Network controller".to_string(),
					},
					0x03 => match s {
						0x00 => "VGA compatible controller".to_string(),
						0x01 => "3D controller".to_string(),
						_ => "Display controller".to_string(),
					},
					0x04 => match s {
						0x00 => "Multimedia video controller".to_string(),
						0x01 => "Audio controller".to_string(),
						_ => "Multimedia controller".to_string(),
					},
					0x05 => "Memory controller".to_string(),
					0x06 => match s {
						0x04 => "PCI-to-PCI bridge".to_string(),
						_ => "Bridge".to_string(),
					},
					0x07 => "Simple communication controller".to_string(),
					0x08 => "Base system peripheral".to_string(),
					0x09 => match s {
						0x00 => "Keyboard".to_string(),
						0x01 => "Digitizer".to_string(),
						_ => "Input device".to_string(),
					},
					0x0C => match s {
						0x03 => match p {
							0x00 => "USB UHCI".to_string(),
							0x10 => "USB OHCI".to_string(),
							0x20 => "USB EHCI".to_string(),
							0x30 => "USB XHCI".to_string(),
							_ => "USB controller".to_string(),
						},
						_ => "Serial bus controller".to_string(),
					},
					0xFF => "Vendor-specific".to_string(),
					_ => "Unknown".to_string(),
				}
			};
			println!(" - id={:>3} {:04x}:{:04x} {}", e.device.id, info.vendor_id, info.device_id, concise_type);
		}
	}
}

use lazy_static::lazy_static;

lazy_static! {
	pub static ref GLOBAL_MANAGER: DeviceManager = DeviceManager::new();
}
