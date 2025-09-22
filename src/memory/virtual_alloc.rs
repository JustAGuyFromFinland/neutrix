#![no_std]

use x86_64::structures::paging::{Page, Size4KiB};
use x86_64::VirtAddr;

/// A very small bump-style virtual page allocator.
/// It hands out single pages from a fixed virtual range.
pub struct VirtualPageAllocator {
    start: VirtAddr,
    end: VirtAddr,
    next: VirtAddr,
}

impl VirtualPageAllocator {
    /// Create a new allocator over [start, end). Both addresses must be page-aligned.
    pub const fn new(start: VirtAddr, end: VirtAddr) -> Self {
        VirtualPageAllocator { start, end, next: start }
    }

    /// Allocate one page and return the Page object.
    pub fn allocate_page(&mut self) -> Option<Page<Size4KiB>> {
        if self.next >= self.end { return None; }
        let page = Page::containing_address(self.next);
        self.next = self.next + 4096u64;
        Some(page)
    }

    /// Reset allocator (for testing/early boot only).
    pub fn reset(&mut self) { self.next = self.start; }
}
