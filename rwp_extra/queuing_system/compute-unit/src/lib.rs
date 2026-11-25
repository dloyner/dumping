use std::alloc::{Layout, alloc};
use std::ptr;
use std::thread::sleep;
use std::time::Duration;

unsafe extern "C" {
    fn redis_get(key_ptr: i32, key_len: i32) -> i32;
}

#[unsafe(no_mangle)]
pub extern "C" fn ns_malloc(size: u32, alignment: u32) -> *mut u8 {
    if size == 0 {
        return ptr::null_mut();
    }
    match Layout::from_size_align(size as usize, alignment as usize) {
        Ok(layout) => unsafe { alloc(layout) },
        Err(_) => ptr::null_mut(),
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn run_compute(mem_ptr: i32, len: i32) -> i32 {
    let mem_ptr = ptr::with_exposed_provenance_mut::<u8>(mem_ptr as u32 as usize);
    let memory = unsafe { std::slice::from_raw_parts_mut(mem_ptr, len as usize) };

    let Some((head, _)) = memory.split_first_chunk_mut::<8>() else {
        return -1;
    };
    let (highest_bytes, current_bytes) = head.split_at_mut(4);

    let highest = i32::from_le_bytes((&*highest_bytes).try_into().expect("exactly 4 bytes"));
    let current = i32::from_le_bytes((&*current_bytes).try_into().expect("exactly 4 bytes"))
        .saturating_add(1);

    // Simulate work
    sleep(Duration::from_secs(2));

    current_bytes.copy_from_slice(&current.to_le_bytes());
    println!("{memory:?}");

    i32::from(current >= highest)
}
