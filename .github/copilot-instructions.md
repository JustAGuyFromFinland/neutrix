## Neutrix — Quick agent guide

Purpose: give an AI coding agent the minimum, actionable context to be immediately productive in this bare-metal Rust kernel.

- Big picture
  - This is a small x86_64 kernel (no_std, no_main). The entrypoint is `entry_point!(kernel_main)` in `src/main.rs`.
  - Bootloader provides a `BootInfo` struct; the kernel uses `physical_memory_offset` + `memory_map` to initialize paging and the heap (`src/memory/paging.rs`, `src/memory/allocator.rs`).
  - Major subsystems live under `src/arch/` (GDT, IDT, exceptions, interrupts, task/executor), `src/memory/` (paging, allocator), `src/bootvga/` (VGA writer + macros), `src/devices/` (PIT, PS/2 keyboard), and `src/rlib/` (hand-optimized mem/memset/memcpy implementations).
    - Major subsystems live under `src/arch/` (GDT, IDT, exceptions, interrupts, task/executor), `src/memory/` (paging, allocator), `src/bootvga/` (VGA writer + macros), `src/devices/` (PIT, PS/2 keyboard, APIC drivers, device-specific subfolders), `hal/` (hardware abstraction layer), and `src/rlib/` (hand-optimized mem/memset/memcpy implementations).
    - Runtime model: early CPU setup (GDT/IDT), enable interrupts, init paging/heap, initialize HAL and platform devices (APIC/PIC, PIT, PS/2, ACPI parsing where present), then run a small async executor (`Executor::new()` / `Task::new(...)`) from `main.rs`.

- Critical developer workflows (discoverable here)
  - Tooling: `Cargo.toml` contains a `target.x86_64-blog_os` section (linker = `lld.exe`) and an `x86_64-blog_os.json` target file in repo root. Builds must target that JSON and have a suitable linker available on PATH.
  - The crate depends on `bootloader`/`x86_64` and uses unstable build-std features declared in `Cargo.toml`. Expect to use a nightly toolchain for some operations.
  - Common build path (adjust for your environment/tooling): build the kernel for the custom target file and produce a bootable image using the bootloader/bootimage workflow. If you maintain scripts or CI, ensure `lld` (or `lld.exe` on Windows) and the nightly toolchain are available.
    - Common build command (example):
      ```
      cargo +nightly bootimage --release --target x86_64-blog_os.json -Z build-std=core,compiler_builtins,alloc
      ```

- Project conventions & important patterns
  - no_std / alloc: the project enables `#![no_std]` and uses `alloc` — rely on the global allocator in `src/memory/allocator.rs` (constants `HEAP_START` and `HEAP_SIZE`).
  - Global writer: `src/bootvga/vga_buffer.rs` exposes a global `WRITER: Mutex<Writer>` via `lazy_static!`. Output is printed to VGA memory (0xb8000). Use the project's `println!` wrapper which writes to this writer.
  - Unsafe and low-level code is common: `src/rlib/mem.rs` provides SSE2-optimized `memcpy`, `memset`, `memcmp`, etc. Treat these as performance/ABI-sensitive; don't change calling conventions.
  - Interrupt and CPU setup: modifying `src/arch/*` (GDT/IDT/interrupts/exceptions) affects system stability; test in a VM (QEMU) before hardware.
  - When adding a new global module directory under `src/` (for example `src/my_driver/`), you MUST add and export it from `src/lib.rs` so the crate exposes the module consistently. Example lines to add to `src/lib.rs`:

      pub mod my_driver;
      pub use my_driver::*;
  - Project convention for new Rust source files: always include `use crate::*;` near the top of each new file (one of the first non-comment lines). Put it immediately after any `mod`/`pub mod` declarations or file-level doc comments. Example:

      // ...file-level comments/docs...
      use crate::*;

    This ensures common macros and crate-level re-exports are available to every module. Missing this line causes commonly-used macros and items to be unavailable in new files.

- Integration points & cross-component notes
  - `BootInfo` -> `memory::init` in `src/memory/paging.rs` (maps physical memory to virtual via `physical_memory_offset`).
  - Heap init occurs in `main.rs`: `allocator::init_heap(&mut mapper, &mut frame_allocator)`; allocator uses `linked_list_allocator::LockedHeap`.
  - VGA text/memory mapping: code expects the VGA text buffer at `0xb8000` (see `vga_buffer.rs`); cursor updates send I/O to ports `0x3D4/0x3D5`.
  - Devices: keyboard and PIT drivers live under `src/devices/` — they use `pic8259` and `pc-keyboard` crates for interaction with IRQs.
 - Integration points & cross-component notes
  - `BootInfo` -> `memory::init` in `src/memory/paging.rs` (maps physical memory to virtual via `physical_memory_offset`).
  - Heap init occurs in `main.rs`: `allocator::init_heap(&mut mapper, &mut frame_allocator)`; allocator uses `linked_list_allocator::LockedHeap`.
  - VGA text/memory mapping: code expects the VGA text buffer at `0xb8000` (see `src/bootvga/vga_buffer.rs`); cursor updates send I/O to ports `0x3D4/0x3D5`.
  - HAL: a hardware abstraction layer lives under `hal/` (see `hal/mod.rs`, `hal/hal.rs`) and exposes platform-level helpers for APIC, I/O port helpers, CPU feature detection, and common device wiring. Prefer using HAL helpers for shared hardware tasks rather than duplicating low-level I/O.
  - APIC & PIC: APIC-related code and IRQ routing appear under `src/devices/apic/` and `hal/apic/` (if present). Legacy PIC code and `pic8259` interactions are used by some drivers; changing interrupt routing requires careful coordination across `src/arch/`, `hal/`, and `src/devices/`.
  - ACPI: ACPI parsing and table handling reside in `src/devices/acpi/` (`acpi.rs`) and are used to enumerate devices and power-management interfaces. When modifying device discovery or power management, consult this module.
  - Devices: drivers are organized under `src/devices/` with per-device subfolders: `PIT/`, `ps2keyboard/`, `vga/`, `acpi/`, etc. Initialization order and IRQ mapping are important; device init is typically invoked early from `main.rs` or via the HAL/platform init path.
  - rlib: `src/rlib/mem.rs` exports low-level optimized routines (`memcpy`, `memset`, `memcmp`, `memmove`) that are ABI- and performance-sensitive. Avoid changing signatures or calling conventions.

- Examples (concrete call sites to reference)
  - Kernel entry: `src/main.rs` -> `entry_point!(kernel_main)` and `kernel_main(boot_info: &BootInfo)`
  - Heap constants: `src/memory/allocator.rs` -> `HEAP_START`, `HEAP_SIZE` and `init_heap(...)` implementation
  - VGA writer: `src/bootvga/vga_buffer.rs` -> `WRITER`, `Writer::write_byte`, and `update_cursor()` which uses `outb` I/O
  - Optimized mem: `src/rlib/mem.rs` (`memcpy`, `memset`, `memcmp`, `memmove`) — these are unsafe extern "C" symbols used for low-level memory ops.
 - Examples (concrete call sites to reference)
  - Kernel entry: `src/main.rs` -> `entry_point!(kernel_main)` and `kernel_main(boot_info: &BootInfo)`
  - Heap constants: `src/memory/allocator.rs` -> `HEAP_START`, `HEAP_SIZE` and `init_heap(...)` implementation
  - HAL entry points: `hal/mod.rs`, `hal/hal.rs` provide helpers and initialization paths for platform features (APIC, I/O, CPU). Use these for cross-cutting hardware setup.
  - ACPI: `src/devices/acpi/acpi.rs` -> table parsing and device discovery APIs
  - APIC/PIC: `src/devices/apic/` and `hal/apic/` (if present) -> APIC init, EOI handling, and IRQ vector setup
  - VGA writer: `src/bootvga/vga_buffer.rs` -> `WRITER`, `Writer::write_byte`, and `update_cursor()` which uses `outb` I/O
  - Optimized mem: `src/rlib/mem.rs` (`memcpy`, `memset`, `memcmp`, `memmove`) — these are unsafe extern "C" symbols used for low-level memory ops.

- Safe edit guidance (do this before changing behavior)
  - Run changes in an emulator (QEMU) or VM; kernel code can hang or crash the host when run on bare metal.
  - For paging/heap/interrupt changes, create minimal reproducer tasks in `main.rs` or small unit tests (where possible) to validate before wiring into boot path.
  - Preserve external ABI for rlib functions and avoid changing global layout (e.g., `HEAP_START`) without migrating mapped pages.
 - Safe edit guidance (do this before changing behavior)
  - Run changes in an emulator (QEMU) or VM; kernel code can hang or crash the host when run on bare metal.
  - For paging/heap/interrupt changes, create minimal reproducer tasks in `main.rs` or small unit tests (where possible) to validate before wiring into boot path.
  - Preserve external ABI for rlib functions and avoid changing global layout (e.g., `HEAP_START`) without migrating mapped pages.
  - When modifying interrupt handling, APIC, or PIC code, test in QEMU with a serial console and the same PIC/APIC configuration you expect on target hardware. Changing IRQ vectors, EOIs, or mask behavior can deadlock the VM.
  - For ACPI and device enumeration changes, add unit-style tests that parse a small known ACPI table blob (or a trimmed table file) to validate parsing code before running on hardware.
  - When adding or changing HAL APIs, keep them small, well-documented, and backward-compatible. Document ownership, inputs/outputs, and concurrency expectations. Prefer adding new helpers over mutating widely-used ones.

If anything above is unclear or you want more detail (build commands for your exact setup, CI scripts, or examples of making a small change), tell me which part to expand and I will iterate.

Additional developer notes — driver framework & PCI
  - A small KMDF-like driver framework and PCI enumerator were added under `src/driver_framework/` and `src/devices/pci.rs`.
  - Driver framework:
    - `src/driver_framework/device.rs` contains `DeviceInfo`, `Resource` and `ResourceKind` and helper functions. `DeviceInfo` now includes `capabilities: Vec<Capability>`.
    - `ResourceKind::Msi { vectors, addr64, maskable, msg_addr: u64, msg_data: u16 }` provides a canonical MSI message target for drivers. Use `Device::msi_resources()` to find MSI resources quickly.
    - `ResourceKind::Msix { table_bar: u8, table_offset: u32, table_size: u16, table_present: bool, first_entry_masked: bool }` describes MSI-X table location and basic probe results.
    - `Device::msix_resources()` returns cloned MSI-X resource entries for driver use.
  - When adding new global modules, it is mandatory to export them from `src/lib.rs` (see example above) and include `use crate::*;` at the top of each new source file.
  - PCI enumerator summary (`src/devices/pci.rs`):
    - Scans buses/slots/functions using legacy port-based config (0xCF8/0xCFC). It reads BARs, sizes them (32/64-bit), and records MMIO/IO resources.
    - Parses the capability list (when Status.capabilities_list is set) and records Power Management, PCI Express, MSI, MSI-X, and unknown capabilities into `DeviceInfo.capabilities`.
    - MSI parsing normalizes the message address into a 64-bit `msg_addr` and 16-bit `msg_data` so drivers can program MSIs immediately.
    - MSI-X parsing records the BIR (BAR indicator) and table offset/size. The scanner attempts to probe the first MSI-X entry by mapping the identified BAR using the kernel's `physical_memory_offset` heuristic and reading the first Vector Control — this is a heuristic probe and may be replaced with a mapper-based mapping for safety.
  - Mapping & safety guidance:
    - For robust MSI-X handling, map the exact BAR range using the kernel mapper (`memory::init` returns an `OffsetPageTable`) and a `FrameAllocator` before reading MSI-X tables. The PCI enumerator contains a fallback heuristic that computes virt = physical_memory_offset + phys.
    - If you modify `scan_and_register_with_phys_offset` to accept a mapper/allocator, ensure caller sites (e.g., `main.rs`) pass the initialized `mapper` and `frame_allocator` (both are available after `memory::init` and `BootInfoFrameAllocator::init`). Mapping must be page-aligned and flushed.
  - Driver author tips:
    - Prefer using `GLOBAL_MANAGER` in `crate::driver_framework::manager` to find and attach drivers. Use `Device::info()` to snapshot device metadata and `msi_resources()` / `msix_resources()` for interrupt setup.
    - Treat reads of device MMIO memory as unsafe — prefer using the kernel mapper to create a safe virtual mapping instead of relying on `physical_memory_offset + phys` heuristics.
    - When building interrupt handlers that use MSI-X, validate `first_entry_masked` and `table_present` from the `Msix` resource before claiming entries.

