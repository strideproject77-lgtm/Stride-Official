//! Small C ABI for passing DEX inventory summaries to Swift.
//!
//! This module reports structural metadata only. It does not verify or execute DEX code.

use std::panic::{catch_unwind, AssertUnwindSafe};
use std::slice;

/// Summary returned after a successful DEX metadata scan.
#[repr(C)]
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct StrideDexSummary {
    pub class_count: u32,
    pub method_count: u32,
}

/// Inspect one DEX buffer and return the total class and encoded method counts.
///
/// Return codes:
/// - 0: success
/// - 1: null input or output pointer
/// - 2: input length exceeds Rust's slice limit
/// - 3: invalid or unsupported DEX
/// - 4: parser panicked
/// - 5: total method count overflowed
///
/// # Safety
/// When `len` is nonzero, `data` must point to `len` readable bytes. `out`
/// must point to an aligned, writable `StrideDexSummary`. Both pointers must
/// remain valid for the duration of the call.
#[no_mangle]
pub unsafe extern "C" fn stride_dex_inventory_counts(
    data: *const u8,
    len: usize,
    out: *mut StrideDexSummary,
) -> i32 {
    if data.is_null() || out.is_null() {
        return 1;
    }
    if len > isize::MAX as usize {
        return 2;
    }

    let result = catch_unwind(AssertUnwindSafe(|| {
        // The caller guarantees that data references len readable bytes.
        let bytes = unsafe { slice::from_raw_parts(data, len) };
        let inventory = stride_dex_inspector::classes::inspect_classes(bytes)
            .map_err(|_| 3i32)?;
        let class_count = u32::try_from(inventory.classes.len()).map_err(|_| 5i32)?;
        let method_count = inventory.classes.iter().try_fold(0u32, |total, class| {
            total.checked_add(u32::try_from(class.methods.len()).ok()?)
        }).ok_or(5i32)?;
        Ok::<_, i32>(StrideDexSummary { class_count, method_count })
    }));

    match result {
        Ok(Ok(summary)) => {
            // The caller guarantees that out points to an aligned writable value.
            unsafe { out.write(summary) };
            0
        }
        Ok(Err(code)) => {
            unsafe { out.write(StrideDexSummary::default()) };
            code
        }
        Err(_) => {
            unsafe { out.write(StrideDexSummary::default()) };
            4
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_null_pointers() {
        let mut summary = StrideDexSummary::default();
        assert_eq!(unsafe {
            stride_dex_inventory_counts(std::ptr::null(), 0, &mut summary)
        }, 1);
        assert_eq!(unsafe {
            stride_dex_inventory_counts([0u8; 1].as_ptr(), 1, std::ptr::null_mut())
        }, 1);
    }

    #[test]
    fn reports_invalid_dex_without_exposing_rust_errors() {
        let bytes = [0u8; 8];
        let mut summary = StrideDexSummary { class_count: 99, method_count: 99 };
        let status = unsafe {
            stride_dex_inventory_counts(bytes.as_ptr(), bytes.len(), &mut summary)
        };
        assert_eq!(status, 3);
        assert_eq!(summary, StrideDexSummary::default());
    }
}
