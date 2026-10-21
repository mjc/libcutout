use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;

#[derive(Clone, Copy, Default)]
struct Counts {
    allocations: usize,
    reallocations: usize,
}

thread_local! {
    // Const initialization and a value without Drop keep allocator hooks allocation-free.
    static COUNTS: Cell<Option<Counts>> = const { Cell::new(None) };
}

struct CountingAllocator;

#[global_allocator]
static GLOBAL_ALLOCATOR: CountingAllocator = CountingAllocator;

fn record_allocation(reallocation: bool) {
    // TLS may be unavailable during thread teardown; allocator hooks must not unwind.
    let _ = COUNTS.try_with(|slot| {
        if let Some(mut counts) = slot.get() {
            if reallocation {
                counts.reallocations = counts.reallocations.saturating_add(1);
            } else {
                counts.allocations = counts.allocations.saturating_add(1);
            }
            slot.set(Some(counts));
        }
    });
}

// SAFETY: counters use nonallocating, nonpanicking thread-local Cell operations.
// Every allocation operation delegates the original pointers and layouts to System.
unsafe impl GlobalAlloc for CountingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        record_allocation(false);
        // SAFETY: delegate the caller's valid layout to the system allocator.
        unsafe { System.alloc(layout) }
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        record_allocation(false);
        // SAFETY: delegate the caller's valid layout to the system allocator.
        unsafe { System.alloc_zeroed(layout) }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        // SAFETY: delegate the caller's valid pointer and layout to the system allocator.
        unsafe { System.dealloc(ptr, layout) }
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        record_allocation(true);
        // SAFETY: delegate the caller's valid allocation and size to the system allocator.
        unsafe { System.realloc(ptr, layout, new_size) }
    }
}

struct Measurement;

impl Drop for Measurement {
    fn drop(&mut self) {
        let _ = COUNTS.try_with(|slot| slot.set(None));
    }
}

/// Checks the allocation budget for synchronous work on the calling thread.
pub(crate) fn assert_no_allocations(label: &str, action: impl FnOnce()) {
    COUNTS.with(|slot| {
        assert!(slot.get().is_none(), "allocation measurements cannot nest");
        slot.set(Some(Counts::default()));
    });
    let guard = Measurement;
    action();
    let counts = COUNTS.with(|slot| slot.get().expect("allocation measurement is active"));
    drop(guard);

    assert_eq!(counts.allocations, 0, "{label} allocated");
    assert_eq!(counts.reallocations, 0, "{label} reallocated");
}

#[cfg(test)]
mod tests {
    use std::hint::black_box;
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    };

    use super::assert_no_allocations;

    #[test]
    fn allocation_measurement_excludes_other_threads() {
        let phase = Arc::new(AtomicUsize::new(0));
        let worker_phase = Arc::clone(&phase);
        let worker = std::thread::spawn(move || {
            worker_phase.store(1, Ordering::Release);
            while worker_phase.load(Ordering::Acquire) != 2 {
                std::hint::spin_loop();
            }
            let bytes = vec![black_box(0_u8); black_box(64)];
            black_box(bytes);
            worker_phase.store(3, Ordering::Release);
        });
        while phase.load(Ordering::Acquire) != 1 {
            std::hint::spin_loop();
        }
        let result = std::panic::catch_unwind(|| {
            assert_no_allocations("measured thread while another thread allocates", || {
                phase.store(2, Ordering::Release);
                while phase.load(Ordering::Acquire) != 3 {
                    std::hint::spin_loop();
                }
            });
        });
        worker.join().expect("allocation worker finishes");
        if let Err(panic) = result {
            std::panic::resume_unwind(panic);
        }
    }

    #[test]
    #[should_panic(expected = "local allocation allocated")]
    fn rejects_local_allocation() {
        assert_no_allocations("local allocation", || {
            black_box(Box::new(black_box(42_u64)));
        });
    }

    #[test]
    #[should_panic(expected = "local zeroed allocation allocated")]
    fn rejects_local_zeroed_allocation() {
        assert_no_allocations("local zeroed allocation", || {
            black_box(vec![0_u8; black_box(64)]);
        });
    }

    #[test]
    #[should_panic(expected = "local reallocation reallocated")]
    fn rejects_local_reallocation() {
        let mut bytes = Vec::<u8>::with_capacity(1);
        assert_no_allocations("local reallocation", || {
            black_box(&mut bytes).reserve_exact(black_box(64));
            black_box(&bytes);
        });
    }

    #[test]
    fn panicking_action_clears_measurement() {
        let result = std::panic::catch_unwind(|| {
            assert_no_allocations("panicking action", || panic!("intentional action panic"));
        });
        assert!(result.is_err());
        assert_no_allocations("after panicking action", || {});
    }
}
