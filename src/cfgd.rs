// SPDX-License-Identifier: GPL-3.0-only

use std::ffi::{CStr, CString, c_char, c_int, c_void};
use std::os::fd::{AsFd, BorrowedFd, RawFd};
use std::sync::atomic::{AtomicBool, Ordering};

use libloading::{Library, Symbol};

const LIBRARY_NAMES: [&str; 2] = ["libcfgd.so.0", "libcfgd.so"];

#[repr(C)]
struct RawCfgd {
    _private: [u8; 0],
}

type ChangedFn = unsafe extern "C" fn(key: *const c_char, user: *mut c_void);

struct Functions {
    open: unsafe extern "C" fn() -> *mut RawCfgd,
    close: unsafe extern "C" fn(*mut RawCfgd),
    get_text: unsafe extern "C" fn(*mut RawCfgd, *const c_char) -> *mut c_char,
    get_strv: unsafe extern "C" fn(*mut RawCfgd, *const c_char, *mut usize) -> *mut *mut c_char,
    strv_free: unsafe extern "C" fn(*mut *mut c_char, usize),
    subscribe: unsafe extern "C" fn(*mut RawCfgd, *const c_char, ChangedFn, *mut c_void) -> c_int,
    notify_fd: unsafe extern "C" fn(*mut RawCfgd) -> c_int,
    dispatch: unsafe extern "C" fn(*mut RawCfgd),
}

unsafe extern "C" {
    fn free(pointer: *mut c_void);
}

pub struct Cfgd {
    _library: Library,
    functions: Functions,
    raw: *mut RawCfgd,
    changed: Box<AtomicBool>,
}

pub struct NotifyFd(RawFd);

impl AsFd for NotifyFd {
    fn as_fd(&self) -> BorrowedFd<'_> {
        unsafe { BorrowedFd::borrow_raw(self.0) }
    }
}

unsafe extern "C" fn mark_changed(_key: *const c_char, user: *mut c_void) {
    let flag = unsafe { &*(user as *const AtomicBool) };
    flag.store(true, Ordering::Relaxed);
}

fn load_library() -> Option<Library> {
    LIBRARY_NAMES
        .iter()
        .find_map(|name| unsafe { Library::new(name) }.ok())
}

fn symbol<T: Copy>(library: &Library, name: &[u8]) -> Option<T> {
    let symbol: Symbol<T> = unsafe { library.get(name) }.ok()?;
    Some(*symbol)
}

fn load_functions(library: &Library) -> Option<Functions> {
    Some(Functions {
        open: symbol(library, b"cfgd_open\0")?,
        close: symbol(library, b"cfgd_close\0")?,
        get_text: symbol(library, b"cfgd_get_text\0")?,
        get_strv: symbol(library, b"cfgd_get_strv\0")?,
        strv_free: symbol(library, b"cfgd_strv_free\0")?,
        subscribe: symbol(library, b"cfgd_subscribe\0")?,
        notify_fd: symbol(library, b"cfgd_notify_fd\0")?,
        dispatch: symbol(library, b"cfgd_dispatch\0")?,
    })
}

impl Cfgd {
    pub fn open() -> Option<Cfgd> {
        let library = load_library()?;
        let functions = load_functions(&library)?;
        let raw = unsafe { (functions.open)() };
        if raw.is_null() {
            return None;
        }
        Some(Cfgd {
            _library: library,
            functions,
            raw,
            changed: Box::new(AtomicBool::new(false)),
        })
    }

    pub fn subscribe(&mut self, prefix: &str) -> bool {
        let prefix = CString::new(prefix).expect("prefix has no NUL byte");
        let user = &*self.changed as *const AtomicBool as *mut c_void;
        unsafe { (self.functions.subscribe)(self.raw, prefix.as_ptr(), mark_changed, user) >= 0 }
    }

    pub fn notify_fd(&self) -> Option<NotifyFd> {
        let fd = unsafe { (self.functions.notify_fd)(self.raw) };
        (fd >= 0).then_some(NotifyFd(fd))
    }

    pub fn dispatch(&mut self) -> bool {
        unsafe { (self.functions.dispatch)(self.raw) };
        self.changed.swap(false, Ordering::Relaxed)
    }

    pub fn get_text(&mut self, key: &str) -> Option<String> {
        let key = CString::new(key).expect("key has no NUL byte");
        let pointer = unsafe { (self.functions.get_text)(self.raw, key.as_ptr()) };
        if pointer.is_null() {
            return None;
        }
        let value = unsafe { CStr::from_ptr(pointer) }.to_string_lossy().into_owned();
        unsafe { free(pointer.cast()) };
        Some(value)
    }

    pub fn get_strv(&mut self, key: &str) -> Option<Vec<String>> {
        let key = CString::new(key).expect("key has no NUL byte");
        let mut count = 0;
        let list = unsafe { (self.functions.get_strv)(self.raw, key.as_ptr(), &mut count) };
        if list.is_null() {
            return None;
        }
        let items = (0..count)
            .map(|index| unsafe { CStr::from_ptr(*list.add(index)) }.to_string_lossy().into_owned())
            .collect();
        unsafe { (self.functions.strv_free)(list, count) };
        Some(items)
    }
}

impl Drop for Cfgd {
    fn drop(&mut self) {
        unsafe { (self.functions.close)(self.raw) };
    }
}
