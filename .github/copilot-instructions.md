## Neutrix — Quick agent guide

Purpose: give an AI coding agent the minimum, actionable context to be immediately productive in this bare-metal Rust kernel.

- Big picture
  - This is a small x86_64 kernel (no_std, no_main). The entrypoint is `entry_point!(kernel_main)` in `src/main.rs`.
  - Bootloader provides a `BootInfo` struct; the kernel uses `physical_memory_offset` + `memory_map` to initialize paging and the heap (`src/memory/paging.rs`, `src/memory/allocator.rs`).
  - Major subsystems live under `src/arch/` (GDT, IDT, exceptions, interrupts, task/executor), `src/memory/` (paging, allocator), `src/bootvga/` (VGA writer + macros), `src/devices/` (PIT, PS/2 keyboard), and `src/rlib/` (hand-optimized mem/memset/memcpy implementations).
  - Runtime model: early CPU setup (GDT/IDT), enable interrupts, init paging/heap, then run a small async executor (`Executor::new()` / `Task::new(...)`) from `main.rs`.

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

- Integration points & cross-component notes
  - `BootInfo` -> `memory::init` in `src/memory/paging.rs` (maps physical memory to virtual via `physical_memory_offset`).
  - Heap init occurs in `main.rs`: `allocator::init_heap(&mut mapper, &mut frame_allocator)`; allocator uses `linked_list_allocator::LockedHeap`.
  - VGA text/memory mapping: code expects the VGA text buffer at `0xb8000` (see `vga_buffer.rs`); cursor updates send I/O to ports `0x3D4/0x3D5`.
  - Devices: keyboard and PIT drivers live under `src/devices/` — they use `pic8259` and `pc-keyboard` crates for interaction with IRQs.

- Examples (concrete call sites to reference)
  - Kernel entry: `src/main.rs` -> `entry_point!(kernel_main)` and `kernel_main(boot_info: &BootInfo)`
  - Heap constants: `src/memory/allocator.rs` -> `HEAP_START`, `HEAP_SIZE` and `init_heap(...)` implementation
  - VGA writer: `src/bootvga/vga_buffer.rs` -> `WRITER`, `Writer::write_byte`, and `update_cursor()` which uses `outb` I/O
  - Optimized mem: `src/rlib/mem.rs` (`memcpy`, `memset`, `memcmp`, `memmove`) — these are unsafe extern "C" symbols used for low-level memory ops.

- Safe edit guidance (do this before changing behavior)
  - Run changes in an emulator (QEMU) or VM; kernel code can hang or crash the host when run on bare metal.
  - For paging/heap/interrupt changes, create minimal reproducer tasks in `main.rs` or small unit tests (where possible) to validate before wiring into boot path.
  - Preserve external ABI for rlib functions and avoid changing global layout (e.g., `HEAP_START`) without migrating mapped pages.

If anything above is unclear or you want more detail (build commands for your exact setup, CI scripts, or examples of making a small change), tell me which part to expand and I will iterate.
