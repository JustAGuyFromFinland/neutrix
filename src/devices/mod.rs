pub mod PIT;
pub use PIT::*;
pub mod acpi;
pub use acpi::*;
pub mod pci;
pub use pci::*;

// PS/2 keyboard driver is implemented as a KMDF-style driver under
// `driver_framework::drivers::ps2kbd` and registered manually by `main.rs`.