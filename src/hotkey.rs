//! Global F8 toggle. The game keeps focus, so the window itself never sees the key.

use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU8, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

pub const VK_F8: u32 = 0x77;
const WM_REBIND: u32 = 0x8001;

const MOD_NOREPEAT: u32 = 0x4000;
const WM_HOTKEY: u32 = 0x0312;
const WM_QUIT: u32 = 0x0012;
const HOTKEY_ID: i32 = 1;

#[repr(C)]
struct Msg {
    hwnd: *mut std::ffi::c_void,
    message: u32,
    _pad: u32,
    wparam: usize,
    lparam: isize,
    time: u32,
    pt_x: i32,
    pt_y: i32,
}

const _: () = assert!(std::mem::size_of::<Msg>() == 48);

#[link(name = "user32")]
extern "system" {
    fn RegisterHotKey(hwnd: *mut std::ffi::c_void, id: i32, modifiers: u32, vk: u32) -> i32;
    fn UnregisterHotKey(hwnd: *mut std::ffi::c_void, id: i32) -> i32;
    fn GetMessageW(msg: *mut Msg, hwnd: *mut std::ffi::c_void, min: u32, max: u32) -> i32;
    fn PostThreadMessageW(thread: u32, msg: u32, wparam: usize, lparam: isize) -> i32;
}

#[link(name = "kernel32")]
extern "system" {
    fn GetCurrentThreadId() -> u32;
}

pub struct Hotkey {
    thread: u32,
    pressed: Arc<AtomicBool>,
    applied: Arc<AtomicU32>,
}

pub fn key_label(vk: u32) -> String {
    match vk {
        0x70..=0x7B => format!("F{}", vk - 0x6F),
        0x30..=0x39 => ((vk as u8) as char).to_string(),
        0x41..=0x5A => ((vk as u8) as char).to_string(),
        _ => format!("0x{vk:02X}"),
    }
}

/// `false` when another program already owns the key. The on-screen button still works.
pub fn spawn(vk: u32) -> (Hotkey, bool) {
    let pressed = Arc::new(AtomicBool::new(false));
    let flag = pressed.clone();
    let applied = Arc::new(AtomicU32::new(vk));
    let applied_thread = applied.clone();
    let thread_id = Arc::new(AtomicU32::new(0));
    let ready = Arc::new(AtomicU8::new(0));
    let tid = thread_id.clone();
    let ready_flag = ready.clone();
    thread::spawn(move || unsafe {
        tid.store(GetCurrentThreadId(), Ordering::Release);
        let hwnd = std::ptr::null_mut();
        let mut current = vk;
        let ok = register(current);
        if ok {
            applied_thread.store(current, Ordering::Release);
        }
        ready_flag.store(if ok { 1 } else { 2 }, Ordering::Release);
        let mut msg = std::mem::zeroed::<Msg>();
        while GetMessageW(&mut msg, std::ptr::null_mut(), 0, 0) > 0 {
            if msg.message == WM_HOTKEY {
                flag.store(true, Ordering::Relaxed);
            } else if msg.message == WM_REBIND {
                let next = msg.wparam as u32;
                if next == current {
                    continue;
                }
                UnregisterHotKey(hwnd, HOTKEY_ID);
                if register(next) {
                    current = next;
                    applied_thread.store(current, Ordering::Release);
                } else if !register(current) {
                    applied_thread.store(0, Ordering::Release);
                }
            }
        }
        UnregisterHotKey(hwnd, HOTKEY_ID);
    });

    let mut id = 0u32;
    let mut state = 0u8;
    for _ in 0..100 {
        id = thread_id.load(Ordering::Acquire);
        state = ready.load(Ordering::Acquire);
        if id != 0 && state != 0 {
            break;
        }
        thread::sleep(Duration::from_millis(2));
    }
    (
        Hotkey {
            thread: id,
            pressed,
            applied,
        },
        state == 1,
    )
}

fn register(vk: u32) -> bool {
    unsafe {
        RegisterHotKey(std::ptr::null_mut(), HOTKEY_ID, MOD_NOREPEAT, vk) != 0
            || RegisterHotKey(std::ptr::null_mut(), HOTKEY_ID, 0, vk) != 0
    }
}

impl Hotkey {
    pub fn take_press(&self) -> bool {
        self.pressed.swap(false, Ordering::Relaxed)
    }

    pub fn rebind(&self, vk: u32) {
        self.applied.store(vk, Ordering::Release);
        if self.thread != 0 {
            unsafe {
                PostThreadMessageW(self.thread, WM_REBIND, vk as usize, 0);
            }
        }
    }

    pub fn applied(&self) -> u32 {
        self.applied.load(Ordering::Acquire)
    }
}

impl Drop for Hotkey {
    fn drop(&mut self) {
        if self.thread != 0 {
            unsafe {
                PostThreadMessageW(self.thread, WM_QUIT, 0, 0);
            }
        }
    }
}
