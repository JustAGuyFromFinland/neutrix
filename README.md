# Neutrix

Neutrix is a small educational x86_64 operating system kernel written in Rust. It follows a bare-metal, no_std, no_main design and provides a compact foundation for experimenting with low-level OS concepts: paging, memory allocation, interrupt handling, device drivers, and a small async executor.

This README covers the project's goals, layout, build and run instructions (PowerShell on Windows), development notes, and how to contribute.

## Key points
- no_std / alloc based Rust kernel
- entrypoint: `entry_point!(kernel_main)` in `src/main.rs`
- Uses `bootimage` / `bootloader` and a custom target (`x86_64-blog_os.json`)
- VGA text output via `src/bootvga/`
- Paging, frame allocator, and a linked-list heap in `src/memory/`
- Device drivers in `src/devices/` and a small driver framework in `driver_framework/`

## Requirements
- Rust toolchain (nightly) — some build steps require nightly features
- `cargo` and `cargo-bootimage` (installable via `cargo install bootimage`)
- A modern `lld` linker available on PATH (on Windows: `lld.exe` or LLVM distribution)
- QEMU (recommended) for running the kernel in a VM

If you don't have the Rust nightly toolchain installed, add it with:

```powershell
rustup toolchain install nightly
rustup component add rust-src --toolchain nightly
cargo install bootimage
```

Ensure `lld` is installed and reachable from PowerShell. For example, installing LLVM or adding the linker that `bootimage` expects.

## Build (PowerShell)

This project uses a custom target JSON (`x86_64-blog_os.json`) and requires building with the nightly toolchain. From the repository root, run:

```powershell
# Build a bootable image (release)
cargo +nightly bootimage --release --target x86_64-blog_os.json -Z build-std=core,compiler_builtins,alloc

# Or for iterative development (debug)
cargo +nightly bootimage --target x86_64-blog_os.json -Z build-std=core,compiler_builtins,alloc
```

Notes:
- The command above produces a bootable image in `target/<target-triple>/release/` (or `debug/`) named like `bootimage-<crate>.bin`.
- If the build fails complaining about `lld`, ensure the LLVM toolchain is installed and `lld.exe` is on PATH.

## Run in QEMU

Run the generated bootimage with QEMU. Example (PowerShell):

```powershell
qemu-system-x86_64 -drive format=raw,file=target/x86_64-blog_os/release/bootimage-neutrix.bin -serial stdio -display none -m 512
```

Optional flags when debugging:
- `-serial stdio` forwards serial output to your terminal.
- `-display none` disables the graphical window; remove it to see VGA output.
- On Linux hosts you can add `-enable-kvm` for acceleration (not applicable on all Windows setups).

## Project layout (important files)

- `src/main.rs` — kernel entry and initialization flow
- `src/lib.rs` — crate exports and module wiring
- `src/arch/` — architecture-specific code (GDT, IDT, interrupts, exceptions, task/executor)
- `src/memory/` — paging, frame allocator, heap (`allocator.rs`), and virtual alloc helpers
- `src/bootvga/` — VGA text writer and helper macros for kernel prints
- `src/devices/` — device drivers and submodules (PCI, APIC, PIT, PS/2, ACPI)
- `driver_framework/` — lightweight driver manager and driver scaffolding
- `rlib/` — hand-optimized runtime helpers (memcpy, memset, etc.) used by low-level code
- `x86_64-blog_os.json` — custom JSON target used by the bootloader/bootimage

See `src/` for more detailed module-level comments and the `copilot-instructions.md` in `.github/` for a developer quick guide.

## Development notes
- This is a low-level kernel: test changes in QEMU or a VM before using real hardware.
- The heap is initialized early in `main.rs` using constants defined in `src/memory/allocator.rs` (`HEAP_START`, `HEAP_SIZE`).
- When adding new top-level modules under `src/`, export them in `src/lib.rs` per project convention.
- Many low-level device and memory helpers assume a fixed `physical_memory_offset` mapping provided by `BootInfo`.

## Troubleshooting
- Build errors about missing nightly features: verify you used `+nightly` and installed `rust-src` for the nightly toolchain.
- Linker errors referencing `lld`: install an LLVM toolchain that provides `lld` and ensure it's on PATH.
- Kernel hangs or early panics: attach a serial console (`-serial stdio`) and/or enable VGA window to observe panic output. Use QEMU's logging to capture serial output.

## Contributing
- Open issues for bugs or feature requests.
- Create pull requests with concise, incremental changes. Low-level changes (memory, interrupts, paging) should include a short rationale and, when possible, a small reproducer or VM instructions to verify.
- Add or update unit tests where practical; run `cargo +nightly test` when applicable (note: many kernel subsystems are not unit-testable under host), and test in QEMU for integration changes.

## License
Licensed under MIT license year 2025
