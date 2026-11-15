// SPDX-License-Identifier: GPL-3.0-only

use std::ffi::{CStr, CString, c_char, c_int, c_void};
use std::os::fd::{AsFd, BorrowedFd, RawFd};
use std::sync::atomic::{AtomicBool, Ordering};

#[repr(C)]
struct RawCfgd {
    _private: [u8; 0],
}

type ChangedFn = unsafe extern "C" fn(key: *const c_char, user: *mut c_void);

unsafe extern "C" {
    fn cfgd_open() -> *mut RawCfgd;
    fn cfgd_close(cfgd: *mut RawCfgd);
    fn cfgd_get_bool(cfgd: *mut RawCfgd, key: *const c_char, value: *mut bool) -> bool;
    fn cfgd_get_int(cfgd: *mut RawCfgd, key: *const c_char, value: *mut i64) -> bool;
    fn cfgd_get_string(cfgd: *mut RawCfgd, key: *const c_char) -> *mut c_char;
    fn cfgd_subscribe(cfgd: *mut RawCfgd, prefix: *const c_char, callback: ChangedFn, user: *mut c_void) -> c_int;
    fn cfgd_notify_fd(cfgd: *mut RawCfgd) -> c_int;
    fn cfgd_dispatch(cfgd: *mut RawCfgd);
    fn free(pointer: *mut c_void);
}

pub struct Cfgd {
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

impl Cfgd {
    pub fn open() -> Option<Cfgd> {
        let raw = unsafe { cfgd_open() };
        if raw.is_null() {
            return None;
        }
        Some(Cfgd {
            raw,
            changed: Box::new(AtomicBool::new(false)),
        })
    }

    pub fn subscribe(&mut self, prefix: &str) -> bool {
        let prefix = CString::new(prefix).expect("prefix has no NUL byte");
        let user = &*self.changed as *const AtomicBool as *mut c_void;
        unsafe { cfgd_subscribe(self.raw, prefix.as_ptr(), mark_changed, user) >= 0 }
    }

    pub fn notify_fd(&self) -> Option<NotifyFd> {
        let fd = unsafe { cfgd_notify_fd(self.raw) };
        (fd >= 0).then_some(NotifyFd(fd))
    }

    pub fn dispatch(&mut self) -> bool {
        unsafe { cfgd_dispatch(self.raw) };
        self.changed.swap(false, Ordering::Relaxed)
    }

    pub fn get_bool(&mut self, key: &str) -> Option<bool> {
        let key = CString::new(key).expect("key has no NUL byte");
        let mut value = false;
        unsafe { cfgd_get_bool(self.raw, key.as_ptr(), &mut value) }.then_some(value)
    }

    pub fn get_int(&mut self, key: &str) -> Option<i64> {
        let key = CString::new(key).expect("key has no NUL byte");
        let mut value = 0;
        unsafe { cfgd_get_int(self.raw, key.as_ptr(), &mut value) }.then_some(value)
    }

    pub fn get_string(&mut self, key: &str) -> Option<String> {
        let key = CString::new(key).expect("key has no NUL byte");
        let pointer = unsafe { cfgd_get_string(self.raw, key.as_ptr()) };
        if pointer.is_null() {
            return None;
        }
        let value = unsafe { CStr::from_ptr(pointer) }.to_string_lossy().into_owned();
        unsafe { free(pointer.cast()) };
        Some(value)
    }
}

impl Drop for Cfgd {
    fn drop(&mut self) {
        unsafe { cfgd_close(self.raw) };
    }
}
