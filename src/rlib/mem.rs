use core::ptr::write_bytes;
use core::arch::asm;
use core::arch::x86_64::*;

#[unsafe(no_mangle)]
pub unsafe extern "C" fn memcpy(mut dst: *mut u8, mut src: *const u8, mut len: usize) {
    if len == 0 {
        return;
    }

    // Align dst for best performance
    while (dst.addr() & 15) != 0 && len > 0 {
        *dst = *src;
        dst = dst.add(1);
        src = src.add(1);
        len -= 1;
    }

    // Copy 64B per loop
    while len >= 64 {
        let c0 = _mm_loadu_si128(src as *const __m128i);
        let c1 = _mm_loadu_si128(src.add(16) as *const __m128i);
        let c2 = _mm_loadu_si128(src.add(32) as *const __m128i);
        let c3 = _mm_loadu_si128(src.add(48) as *const __m128i);

        _mm_storeu_si128(dst as *mut __m128i, c0);
        _mm_storeu_si128(dst.add(16) as *mut __m128i, c1);
        _mm_storeu_si128(dst.add(32) as *mut __m128i, c2);
        _mm_storeu_si128(dst.add(48) as *mut __m128i, c3);

        src = src.add(64);
        dst = dst.add(64);
        len -= 64;
    }

    // Copy 16B
    while len >= 16 {
        let c = _mm_loadu_si128(src as *const __m128i);
        _mm_storeu_si128(dst as *mut __m128i, c);
        src = src.add(16);
        dst = dst.add(16);
        len -= 16;
    }

    // Final tail
    while len > 0 {
        *dst = *src;
        dst = dst.add(1);
        src = src.add(1);
        len -= 1;
    }
}


/// SSE2 optimized memset
#[unsafe(no_mangle)]
pub unsafe extern "C" fn memset(mut dst: *mut u8, value: u8, mut len: usize) {
    if len == 0 {
        return;
    }

    // Broadcast the value into a 128-bit register
    let fill = _mm_set1_epi8(value as i8);

    // Align dst to 16 bytes
    while (dst.addr() & 15) != 0 && len > 0 {
        *dst = value;
        dst = dst.add(1);
        len -= 1;
    }

    // Write 64 bytes (4x 16B) per loop
    while len >= 64 {
        _mm_storeu_si128(dst as *mut __m128i, fill);
        _mm_storeu_si128(dst.add(16) as *mut __m128i, fill);
        _mm_storeu_si128(dst.add(32) as *mut __m128i, fill);
        _mm_storeu_si128(dst.add(48) as *mut __m128i, fill);
        dst = dst.add(64);
        len -= 64;
    }

    // Tail 16B
    while len >= 16 {
        _mm_storeu_si128(dst as *mut __m128i, fill);
        dst = dst.add(16);
        len -= 16;
    }

    // Final tail
    while len > 0 {
        *dst = value;
        dst = dst.add(1);
        len -= 1;
    }
}

/// Compare two memory regions.
///
/// Returns:
/// - 0 if equal,
/// - <0 if `a < b`,
/// - >0 if `a > b`.
///
/// # Safety
/// - `a` and `b` must be valid for `len` bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn memcmp(mut a: *const u8, mut b: *const u8, mut len: usize) -> i32 {
    if len == 0 {
        return 0;
    }

    // Compare 64B at a time
    while len >= 64 {
        let a0 = _mm_loadu_si128(a as *const __m128i);
        let b0 = _mm_loadu_si128(b as *const __m128i);
        let m0 = _mm_cmpeq_epi8(a0, b0);
        let mask0 = _mm_movemask_epi8(m0);

        if mask0 != -1 {
            return slow_byte_cmp(a, b, 16);
        }

        let a1 = _mm_loadu_si128(a.add(16) as *const __m128i);
        let b1 = _mm_loadu_si128(b.add(16) as *const __m128i);
        let m1 = _mm_cmpeq_epi8(a1, b1);
        let mask1 = _mm_movemask_epi8(m1);

        if mask1 != -1 {
            return slow_byte_cmp(a.add(16), b.add(16), 16);
        }

        let a2 = _mm_loadu_si128(a.add(32) as *const __m128i);
        let b2 = _mm_loadu_si128(b.add(32) as *const __m128i);
        let m2 = _mm_cmpeq_epi8(a2, b2);
        let mask2 = _mm_movemask_epi8(m2);

        if mask2 != -1 {
            return slow_byte_cmp(a.add(32), b.add(32), 16);
        }

        let a3 = _mm_loadu_si128(a.add(48) as *const __m128i);
        let b3 = _mm_loadu_si128(b.add(48) as *const __m128i);
        let m3 = _mm_cmpeq_epi8(a3, b3);
        let mask3 = _mm_movemask_epi8(m3);

        if mask3 != -1 {
            return slow_byte_cmp(a.add(48), b.add(48), 16);
        }

        a = a.add(64);
        b = b.add(64);
        len -= 64;
    }

    // 16B chunks
    while len >= 16 {
        let va = _mm_loadu_si128(a as *const __m128i);
        let vb = _mm_loadu_si128(b as *const __m128i);
        let cmp = _mm_cmpeq_epi8(va, vb);
        let mask = _mm_movemask_epi8(cmp);

        if mask != -1 {
            return slow_byte_cmp(a, b, 16);
        }

        a = a.add(16);
        b = b.add(16);
        len -= 16;
    }

    // Tail
    while len > 0 {
        let byte_a = *a;
        let byte_b = *b;
        if byte_a != byte_b {
            return (byte_a as i32) - (byte_b as i32);
        }
        a = a.add(1);
        b = b.add(1);
        len -= 1;
    }

    0
}

unsafe fn slow_byte_cmp(a: *const u8, b: *const u8, n: usize) -> i32 {
    for i in 0..n {
        let aa = *a.add(i);
        let bb = *b.add(i);
        if aa != bb {
            return (aa as i32) - (bb as i32);
        }
    }
    0
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn memmove(dest: *mut u8, src: *const u8, n: usize) -> *mut u8 {
    if dest as usize == src as usize || n == 0 {
        return dest;
    }

    if (dest as usize) < (src as usize) {
        // Forward copy
        let mut offset = 0;
        // Align destination to 16 bytes
        while offset < n && (dest.add(offset) as usize & 0xF) != 0 {
            *dest.add(offset) = *src.add(offset);
            offset += 1;
        }

        let chunks = (n - offset) / 16;
        let remainder = (n - offset) % 16;

        for i in 0..chunks {
            let ptr_src = src.add(offset + i*16) as *const __m128i;
            let ptr_dst = dest.add(offset + i*16) as *mut __m128i;
            let data = _mm_loadu_si128(ptr_src);
            _mm_storeu_si128(ptr_dst, data);
        }

        for i in 0..remainder {
            *dest.add(n - remainder + i) = *src.add(n - remainder + i);
        }
    } else {
        // Backward copy
        let mut offset = 0;
        while offset < n && ((dest.add(n - offset - 1) as usize & 0xF) != 15) {
            *dest.add(n - offset - 1) = *src.add(n - offset - 1);
            offset += 1;
        }

        let chunks = (n - offset) / 16;
        let remainder = (n - offset) % 16;

        for i in 0..chunks {
            let ptr_src = src.add(n - offset - (i+1)*16) as *const __m128i;
            let ptr_dst = dest.add(n - offset - (i+1)*16) as *mut __m128i;
            let data = _mm_loadu_si128(ptr_src);
            _mm_storeu_si128(ptr_dst, data);
        }

        for i in 0..remainder {
            *dest.add(remainder - i - 1) = *src.add(remainder - i - 1);
        }
    }

    dest
}