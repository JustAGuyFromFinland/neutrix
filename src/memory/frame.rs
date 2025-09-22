#![no_std]

use bootloader::bootinfo::MemoryMap;
use bootloader::bootinfo::MemoryRegionType;
use core::{ptr, slice};
use x86_64::{
    VirtAddr,
    PhysAddr,
    structures::paging::{PhysFrame, Size4KiB, FrameAllocator},
};
use crate::*;

pub struct BootInfoFrameAllocator {
    memory_map: &'static MemoryMap,
    phys_offset: VirtAddr,
    kernel_reserved_end: Option<u64>,

    // Bitmap info
    bitmap_phys_start: u64,
    bitmap_bytes: usize,
    num_frames: usize,

    // runtime state
    next_search: usize,
}

impl BootInfoFrameAllocator {
    /// Create a bitmap-backed FrameAllocator.
    ///
    /// The bitmap will be placed in the first usable region found after
    /// `kernel_reserved_end` (if provided). The bitmap covers physical frames
    /// from address 0 up to the highest usable physical address reported in the memory map.
    ///
    /// This function is unsafe because it creates raw pointers into physical memory
    /// (mapped via `phys_offset`) and the caller must guarantee the memory map is valid.
    pub unsafe fn init(
        memory_map: &'static MemoryMap,
        phys_offset: VirtAddr,
        kernel_reserved_end: Option<u64>,
    ) -> Self {
        // determine highest physical address among usable regions
        let mut max_addr: u64 = 0;
        for region in memory_map.iter() {
            if region.region_type == MemoryRegionType::Usable {
                let end = region.range.end_addr();
                if end as u64 > max_addr {
                    max_addr = end as u64;
                }
            }
        }

        if max_addr == 0 {
            // no usable memory found; fallback to empty allocator
            return BootInfoFrameAllocator {
                memory_map,
                phys_offset,
                kernel_reserved_end,
                bitmap_phys_start: 0,
                bitmap_bytes: 0,
                num_frames: 0,
                next_search: 0,
            };
        }

        let num_frames = ((max_addr + 0xFFF) / 0x1000) as usize;
        let bitmap_bytes = (num_frames + 7) / 8;

        // find a usable region to hold the bitmap
        let mut bitmap_phys_start: u64 = 0;
        for region in memory_map.iter() {
            if region.region_type != MemoryRegionType::Usable {
                continue;
            }
            let mut start = region.range.start_addr() as u64;
            let end = region.range.end_addr() as u64;
            // skip regions that are before kernel_reserved_end
            if let Some(kend) = kernel_reserved_end {
                if end <= kend {
                    continue;
                }
                if start < kend {
                    start = (kend + 0xFFF) & !0xFFFu64;
                }
            }

            // align start to page
            let aligned = (start + 0xFFF) & !0xFFFu64;
            if aligned + bitmap_bytes as u64 <= end {
                bitmap_phys_start = aligned;
                break;
            }
        }

        // If we didn't find a place for the bitmap, try to place at the first usable region (may overlap kernel)
        if bitmap_phys_start == 0 {
            for region in memory_map.iter() {
                if region.region_type != MemoryRegionType::Usable { continue; }
                let aligned = (region.range.start_addr() + 0xFFF) & !0xFFFu64;
                let end = region.range.end_addr() as u64;
                if aligned + bitmap_bytes as u64 <= end {
                    bitmap_phys_start = aligned as u64;
                    break;
                }
            }
        }

        // if still zero, we cannot build the bitmap; return minimal allocator that fails allocations
        if bitmap_phys_start == 0 {
            return BootInfoFrameAllocator {
                memory_map,
                phys_offset,
                kernel_reserved_end,
                bitmap_phys_start: 0,
                bitmap_bytes: 0,
                num_frames,
                next_search: 0,
            };
        }

        // Map bitmap physical to virtual and zero it out
        let virt_u64 = phys_offset.as_u64().wrapping_add(bitmap_phys_start);
        let bitmap_ptr = virt_u64 as *mut u8;
        let bitmap_slice = slice::from_raw_parts_mut(bitmap_ptr, bitmap_bytes);
        for b in bitmap_slice.iter_mut() { ptr::write_volatile(b, 0xFF); }

        // initialize: mark all frames as used, then mark usable frames as free
        // set all bits to 1
        // then clear bits corresponding to usable regions
        // (we already wrote 0xFF above)

        // Helper closures
        let mut set_bit = |idx: usize, val: bool| {
            if idx >= num_frames { return; }
            let byte = idx / 8;
            let bit = idx % 8;
            unsafe {
                let p = bitmap_ptr.add(byte);
                let cur = ptr::read_volatile(p);
                let new = if val { cur | (1 << bit) } else { cur & !(1 << bit) };
                ptr::write_volatile(p, new);
            }
        };

        let mut clear_range = |start_idx: usize, end_idx: usize| {
            for i in start_idx..end_idx {
                set_bit(i, false);
            }
        };

        // mark non-usable frames as used by leaving them set to 1; now clear usable ranges
        for region in memory_map.iter() {
            if region.region_type == MemoryRegionType::Usable {
                let start = region.range.start_addr() as u64;
                let end = region.range.end_addr() as u64;
                let start_idx = (start / 0x1000) as usize;
                let end_idx = ((end + 0xFFF) / 0x1000) as usize;
                clear_range(start_idx, end_idx);
            }
        }

        // mark kernel reserved frames as used if requested
        if let Some(kend) = kernel_reserved_end {
            let end_idx = ((kend + 0xFFF) / 0x1000) as usize;
            // mark frames [0, end_idx) as used
            for i in 0..end_idx {
                set_bit(i, true);
            }
        }

        // mark the bitmap's own frames as used so they are not allocated
        let bmp_start_idx = (bitmap_phys_start / 0x1000) as usize;
        let bmp_end_idx = ((bitmap_phys_start + bitmap_bytes as u64 + 0xFFF) / 0x1000) as usize;
        for i in bmp_start_idx..bmp_end_idx {
            set_bit(i, true);
        }

        BootInfoFrameAllocator {
            memory_map,
            phys_offset,
            kernel_reserved_end,
            bitmap_phys_start,
            bitmap_bytes,
            num_frames,
            next_search: 0,
        }
    }

    /// Mark a physical frame as free (for debugging / deallocation support).
    /// This is safe only if the caller guarantees the frame was previously allocated.
    pub unsafe fn free_frame(&mut self, frame: PhysFrame) {
        if self.bitmap_bytes == 0 { return; }
        let idx = (frame.start_address().as_u64() / 0x1000) as usize;
        let virt_u64 = self.phys_offset.as_u64().wrapping_add(self.bitmap_phys_start);
        let p = (virt_u64 as *mut u8).add(idx / 8);
        let cur = ptr::read_volatile(p);
        let new = cur & !(1 << (idx % 8));
        ptr::write_volatile(p, new);
    }

    fn test_bit(&self, idx: usize) -> bool {
        if self.bitmap_bytes == 0 || idx >= self.num_frames { return true; }
        let virt_u64 = self.phys_offset.as_u64().wrapping_add(self.bitmap_phys_start);
        let p = (virt_u64 as *const u8).wrapping_add(idx / 8);
        unsafe {
            let cur = ptr::read_volatile(p);
            (cur & (1 << (idx % 8))) != 0
        }
    }

    fn set_bit_runtime(&mut self, idx: usize, val: bool) {
        if self.bitmap_bytes == 0 || idx >= self.num_frames { return; }
        let virt_u64 = self.phys_offset.as_u64().wrapping_add(self.bitmap_phys_start);
        let p = (virt_u64 as *mut u8).wrapping_add(idx / 8);
        unsafe {
            let cur = ptr::read_volatile(p);
            let new = if val { cur | (1 << (idx % 8)) } else { cur & !(1 << (idx % 8)) };
            ptr::write_volatile(p, new);
        }
    }
}

unsafe impl FrameAllocator<Size4KiB> for BootInfoFrameAllocator {
    fn allocate_frame(&mut self) -> Option<PhysFrame> {
        if self.bitmap_bytes == 0 {
            return None;
        }

        // simple first-fit search starting from next_search
        let mut i = self.next_search;
        while i < self.num_frames {
            if !self.test_bit(i) {
                // mark used
                self.set_bit_runtime(i, true);
                self.next_search = i + 1;
                let addr = (i as u64) * 0x1000u64;
                return Some(PhysFrame::containing_address(PhysAddr::new(addr)));
            }
            i += 1;
        }

        // wrap-around search
        i = 0;
        while i < self.next_search {
            if !self.test_bit(i) {
                self.set_bit_runtime(i, true);
                self.next_search = i + 1;
                let addr = (i as u64) * 0x1000u64;
                return Some(PhysFrame::containing_address(PhysAddr::new(addr)));
            }
            i += 1;
        }

        None
    }
}

