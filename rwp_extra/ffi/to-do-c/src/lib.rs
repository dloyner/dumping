use std::ffi::{CStr, CString, c_char};
use std::ptr;

use to_do::Todo;

const STATUS_PENDING: &str = "PENDING";
const STATUS_DONE: &str = "DONE";

unsafe fn borrow_str<'a>(ptr: *const c_char) -> Option<&'a str> {
    if ptr.is_null() {
        return None;
    }
    unsafe { CStr::from_ptr(ptr) }.to_str().ok()
}

fn into_c_string(s: String) -> *mut c_char {
    CString::new(s).map_or(ptr::null_mut(), CString::into_raw)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn create_todo_json(title: *const c_char) -> *mut c_char {
    let Some(title) = (unsafe { borrow_str(title) }) else {
        return ptr::null_mut();
    };

    let todo = Todo {
        title: title.to_owned(),
        status: STATUS_PENDING.to_owned(),
    };

    serde_json::to_string(&todo).map_or(ptr::null_mut(), into_c_string)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn mark_done_json(todo_json: *const c_char) -> *mut c_char {
    let Some(json) = (unsafe { borrow_str(todo_json) }) else {
        return ptr::null_mut();
    };

    let Ok(mut todo) = serde_json::from_str::<Todo>(json) else {
        return ptr::null_mut();
    };

    todo.status = STATUS_DONE.to_owned();
    serde_json::to_string(&todo).map_or(ptr::null_mut(), into_c_string)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn free_string(ptr: *mut c_char) {
    if !ptr.is_null() {
        drop(unsafe { CString::from_raw(ptr) });
    }
}
