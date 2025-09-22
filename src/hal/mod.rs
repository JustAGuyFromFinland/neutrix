//! Hardware Abstraction Layer (HAL)
//!
//! This module provides a unified interface for hardware initialization,
//! including CPU feature detection/enabling and ACPI management.

pub mod hal;
pub use hal::*;