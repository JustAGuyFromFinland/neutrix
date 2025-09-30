// Simple kernel allocation helpers (kmalloc-style) for common patterns.
// These are small, zero-dependency wrappers around the global allocator
// exposing a few convenient constructors that mirror typical kernel APIs.

use crate::*;
use core::mem::MaybeUninit;
use alloc::boxed::Box;
use alloc::vec::Vec;

/// Allocate a boxed instance of `T` on the kernel heap and return it.
///
/// Equivalent to `Box::new(value)` but provides a named kernel-style helper.
pub fn kmalloc<T>(value: T) -> Box<T> {
    Box::new(value)
}

/// Allocate an uninitialized boxed `T` (returned as `Box<MaybeUninit<T>>`).
/// Useful when a driver needs to allocate space and then initialize in-place
/// (for example, filling from DMA buffers or hardware-provided data).
pub fn kmalloc_uninit<T>() -> Box<MaybeUninit<T>> {
    // SAFETY: Box::new_uninit is stable as Box::new_uninit() on newer Rust;
    // fall back to boxing MaybeUninit::uninit() for compatibility.
    Box::new(MaybeUninit::uninit())
}

/// Allocate a Vec<T> with the requested capacity using the kernel heap.
pub fn kvec_with_capacity<T>(cap: usize) -> Vec<T> {
    Vec::with_capacity(cap)
}

/// Convenience: allocate a boxed default T (requires Default).
pub fn kmalloc_default<T: Default>() -> Box<T> {
    Box::new(T::default())
}
