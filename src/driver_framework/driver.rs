use crate::driver_framework::device::DeviceHandle;
use alloc::boxed::Box;

/// Minimal KMDF-like driver trait. Implementors should be able to probe,
/// start, stop and release devices.
pub trait Driver: Send + Sync {
	/// Called to ask the driver whether it supports the device described by
	/// `DeviceInfo`. Return Ok(()) to claim ownership, Err(str) to decline.
	fn probe(&self, device: &DeviceHandle) -> Result<(), &'static str>;

	/// Called when the device should be started (resources are available).
	fn start(&self, device: &DeviceHandle) -> Result<(), &'static str>;

	/// Called to stop the device but keep the device object around.
	fn stop(&self, device: &DeviceHandle);

	/// Release any remaining resources and prepare for device removal.
	fn release(&self, device: &DeviceHandle);
}

pub type DriverBox = Box<dyn Driver>;
