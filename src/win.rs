//! Small Win32 pieces the window needs: tray icon, process and Steam checks,
//! screen size, links. Raw calls, the same way `overlay_win.rs` does it.

use std::ffi::{c_void, OsStr};
use std::os::windows::ffi::OsStrExt;
use std::sync::atomic::{AtomicPtr, AtomicU32, Ordering};
use std::sync::{Mutex, OnceLock};

const WM_APP: u32 = 0x8000;
const TRAY_MSG: u32 = WM_APP + 1;
const WM_LBUTTONUP: u32 = 0x0202;
const WM_LBUTTONDBLCLK: u32 = 0x0203;
const WM_RBUTTONUP: u32 = 0x0205;
const NIM_ADD: u32 = 0;
const NIM_MODIFY: u32 = 1;
const NIM_DELETE: u32 = 2;
const NIF_MESSAGE: u32 = 0x01;
const NIF_ICON: u32 = 0x02;
const NIF_TIP: u32 = 0x04;
const NIF_INFO: u32 = 0x10;
const NIIF_NOSOUND: u32 = 0x10;
const WS_POPUP: u32 = 0x8000_0000;
const WS_EX_TOOLWINDOW: u32 = 0x80;
const MF_STRING: u32 = 0;
const TPM_RIGHTBUTTON: u32 = 0x0002;
const TPM_RIGHTALIGN: u32 = 0x0008;
const TPM_BOTTOMALIGN: u32 = 0x0020;
const TPM_RETURNCMD: u32 = 0x0100;
const WM_NULL: u32 = 0;
const APP_MODE_FORCE_DARK: i32 = 2;
const SM_CXSCREEN: i32 = 0;
const SM_CYSCREEN: i32 = 1;
const SM_CXSMICON: i32 = 49;
const SPI_GETWORKAREA: u32 = 0x0030;
const SW_SHOWNORMAL: i32 = 1;
const TH32CS_SNAPPROCESS: u32 = 0x2;
const DWMWA_WINDOW_CORNER_PREFERENCE: u32 = 33;
const DWMWCP_ROUND: u32 = 2;
const MB_ICONWARNING: u32 = 0x30;
const GA_ROOT: u32 = 2;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TrayClick {
    Left,
    Right,
    Double,
}

static TRAY_HWND: AtomicPtr<c_void> = AtomicPtr::new(std::ptr::null_mut());
static TRAY_ICON: AtomicPtr<c_void> = AtomicPtr::new(std::ptr::null_mut());
static TASKBAR_CREATED: AtomicU32 = AtomicU32::new(0);

type TrayHandler = Box<dyn Fn(TrayClick) + Send>;

fn tray_handler() -> &'static Mutex<Option<TrayHandler>> {
    static HANDLER: OnceLock<Mutex<Option<TrayHandler>>> = OnceLock::new();
    HANDLER.get_or_init(|| Mutex::new(None))
}

fn tray_tip() -> &'static Mutex<String> {
    static TIP: OnceLock<Mutex<String>> = OnceLock::new();
    TIP.get_or_init(|| Mutex::new("StashSync".into()))
}

pub fn wide(text: &str) -> Vec<u16> {
    OsStr::new(text).encode_wide().chain(std::iter::once(0)).collect()
}

fn fill(dest: &mut [u16], text: &str) {
    let chars: Vec<u16> = text.encode_utf16().take(dest.len() - 1).collect();
    dest[..chars.len()].copy_from_slice(&chars);
    dest[chars.len()] = 0;
}

/// Steam writes the signed-in account here while it runs.
pub fn steam_signed_in() -> bool {
    let hkcu = winreg::RegKey::predef(winreg::enums::HKEY_CURRENT_USER);
    hkcu.open_subkey(r"Software\Valve\Steam\ActiveProcess")
        .and_then(|key| key.get_value::<u32, _>("ActiveUser"))
        .map(|user| user != 0)
        .unwrap_or(false)
}

/// Process list without starting `tasklist`. The window asks every few seconds.
pub fn process_running(exe: &str) -> bool {
    unsafe {
        let snap = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0);
        if snap.is_null() || snap as isize == -1 {
            return false;
        }
        let mut entry: ProcessEntry = std::mem::zeroed();
        entry.size = std::mem::size_of::<ProcessEntry>() as u32;
        let mut found = false;
        if Process32FirstW(snap, &mut entry) != 0 {
            loop {
                let len = entry.exe.iter().position(|ch| *ch == 0).unwrap_or(entry.exe.len());
                if String::from_utf16_lossy(&entry.exe[..len]).eq_ignore_ascii_case(exe) {
                    found = true;
                    break;
                }
                if Process32NextW(snap, &mut entry) == 0 {
                    break;
                }
            }
        }
        CloseHandle(snap);
        found
    }
}

/// Primary screen in physical pixels. The game runs there.
pub fn screen_size() -> (i32, i32) {
    unsafe { (GetSystemMetrics(SM_CXSCREEN).max(1), GetSystemMetrics(SM_CYSCREEN).max(1)) }
}

/// Desktop area without the taskbar, physical pixels: left, top, right, bottom.
pub fn work_area() -> (i32, i32, i32, i32) {
    let mut rect = WinRect { left: 0, top: 0, right: 0, bottom: 0 };
    let ok = unsafe { SystemParametersInfoW(SPI_GETWORKAREA, 0, &mut rect as *mut _ as *mut c_void, 0) };
    if ok == 0 {
        let (w, h) = screen_size();
        return (0, 0, w, h);
    }
    (rect.left, rect.top, rect.right, rect.bottom)
}

/// Only the two pages the window links to.
pub fn open_url(url: &str) {
    let allowed = url == "https://arccalc.github.io" || url == "https://github.com/Arccalc/StashSync";
    if !allowed {
        return;
    }
    let verb = wide("open");
    let target = wide(url);
    unsafe {
        ShellExecuteW(
            std::ptr::null_mut(),
            verb.as_ptr(),
            target.as_ptr(),
            std::ptr::null(),
            std::ptr::null(),
            SW_SHOWNORMAL,
        );
    }
}

/// Windows 11 rounds the borderless window like its own windows. Older Windows ignores it.
pub fn round_corners(hwnd: isize) {
    let pref = DWMWCP_ROUND;
    unsafe {
        DwmSetWindowAttribute(
            hwnd as *mut c_void,
            DWMWA_WINDOW_CORNER_PREFERENCE,
            &pref as *const u32 as *const c_void,
            4,
        );
    }
}

/// True while `hwnd` (or one of its child windows, such as WebView2's) is the foreground window.
pub fn is_foreground(hwnd: isize) -> bool {
    unsafe {
        let fg = GetForegroundWindow();
        !fg.is_null() && GetAncestor(fg, GA_ROOT) as isize == hwnd
    }
}

pub fn message_box(title: &str, text: &str) {
    let title = wide(title);
    let text = wide(text);
    unsafe {
        MessageBoxW(std::ptr::null_mut(), text.as_ptr(), title.as_ptr(), MB_ICONWARNING);
    }
}

pub fn small_icon_size() -> u32 {
    (unsafe { GetSystemMetrics(SM_CXSMICON) }).clamp(16, 64) as u32
}

/// RGBA pixels → HICON. Windows keeps the alpha channel of a 32-bit icon.
pub fn make_icon(rgba: &[u8], side: u32) -> *mut c_void {
    let mut bgra = rgba.to_vec();
    for px in bgra.chunks_exact_mut(4) {
        px.swap(0, 2);
    }
    let mask = vec![0u8; (side * side / 8) as usize];
    unsafe {
        CreateIcon(
            GetModuleHandleW(std::ptr::null()),
            side as i32,
            side as i32,
            1,
            32,
            mask.as_ptr(),
            bgra.as_ptr(),
        )
    }
}

/// Adds the tray icon. `on_click` runs on the UI thread from the tray window.
pub fn tray_add(icon: *mut c_void, tip: &str, on_click: impl Fn(TrayClick) + Send + 'static) {
    if let Ok(mut guard) = tray_handler().lock() {
        *guard = Some(Box::new(on_click));
    }
    if let Ok(mut guard) = tray_tip().lock() {
        *guard = tip.to_string();
    }
    TRAY_ICON.store(icon, Ordering::Relaxed);
    dark_menus();
    unsafe {
        let class = wide("StashSyncTray");
        let info = WndClass {
            size: std::mem::size_of::<WndClass>() as u32,
            style: 0,
            wnd_proc: Some(tray_proc),
            cls_extra: 0,
            wnd_extra: 0,
            instance: GetModuleHandleW(std::ptr::null()),
            icon: std::ptr::null_mut(),
            cursor: std::ptr::null_mut(),
            brush: std::ptr::null_mut(),
            menu: std::ptr::null(),
            class_name: class.as_ptr(),
            icon_sm: std::ptr::null_mut(),
        };
        RegisterClassExW(&info);
        // A hidden top-level window, not a message-only one: those miss the
        // TaskbarCreated broadcast and cannot own the tray menu.
        let hwnd = CreateWindowExW(
            WS_EX_TOOLWINDOW,
            class.as_ptr(),
            class.as_ptr(),
            WS_POPUP,
            0,
            0,
            0,
            0,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            GetModuleHandleW(std::ptr::null()),
            std::ptr::null_mut(),
        );
        if hwnd.is_null() {
            return;
        }
        TRAY_HWND.store(hwnd, Ordering::Relaxed);
        TASKBAR_CREATED.store(RegisterWindowMessageW(wide("TaskbarCreated").as_ptr()), Ordering::Relaxed);
    }
    notify(NIM_ADD, NIF_MESSAGE | NIF_ICON | NIF_TIP, None);
}

/// Right-click menu at the cursor. Returns the picked item's id, 0 when dismissed.
pub fn tray_menu(items: &[(u32, &str)]) -> u32 {
    let hwnd = TRAY_HWND.load(Ordering::Relaxed);
    if hwnd.is_null() {
        return 0;
    }
    unsafe {
        let menu = CreatePopupMenu();
        if menu.is_null() {
            return 0;
        }
        for (id, label) in items {
            let text = wide(label);
            AppendMenuW(menu, MF_STRING, *id as usize, text.as_ptr());
        }
        let mut cursor = WinPoint { x: 0, y: 0 };
        GetCursorPos(&mut cursor);
        // Without this the menu stays open after a click elsewhere.
        SetForegroundWindow(hwnd);
        let picked = TrackPopupMenu(
            menu,
            TPM_RETURNCMD | TPM_RIGHTBUTTON | TPM_BOTTOMALIGN | TPM_RIGHTALIGN,
            cursor.x,
            cursor.y,
            0,
            hwnd,
            std::ptr::null(),
        );
        PostMessageW(hwnd, WM_NULL, 0, 0);
        DestroyMenu(menu);
        picked.max(0) as u32
    }
}

/// Dark context menus to match the window. uxtheme's SetPreferredAppMode (ordinal 135)
/// is undocumented but stable since Windows 10 1903; older systems keep light menus.
fn dark_menus() {
    unsafe {
        let lib = LoadLibraryW(wide("uxtheme.dll").as_ptr());
        if lib.is_null() {
            return;
        }
        let set_mode = GetProcAddress(lib, 135 as *const u8);
        if !set_mode.is_null() {
            let set_mode: unsafe extern "system" fn(i32) -> i32 = std::mem::transmute(set_mode);
            set_mode(APP_MODE_FORCE_DARK);
        }
        let flush = GetProcAddress(lib, 136 as *const u8);
        if !flush.is_null() {
            let flush: unsafe extern "system" fn() = std::mem::transmute(flush);
            flush();
        }
    }
}

/// New status dot or tooltip.
pub fn tray_update(icon: *mut c_void, tip: &str) {
    let old = TRAY_ICON.swap(icon, Ordering::Relaxed);
    if let Ok(mut guard) = tray_tip().lock() {
        *guard = tip.to_string();
    }
    notify(NIM_MODIFY, NIF_ICON | NIF_TIP, None);
    if !old.is_null() && old != icon {
        unsafe { DestroyIcon(old) };
    }
}

/// Windows notification from the tray icon, without a sound.
pub fn tray_balloon(title: &str, text: &str) {
    notify(NIM_MODIFY, NIF_INFO, Some((title, text)));
}

pub fn tray_remove() {
    notify(NIM_DELETE, 0, None);
}

fn notify(op: u32, flags: u32, info: Option<(&str, &str)>) {
    let hwnd = TRAY_HWND.load(Ordering::Relaxed);
    if hwnd.is_null() {
        return;
    }
    unsafe {
        let mut data: NotifyIconData = std::mem::zeroed();
        data.size = std::mem::size_of::<NotifyIconData>() as u32;
        data.hwnd = hwnd;
        data.id = 1;
        data.flags = flags;
        data.callback_message = TRAY_MSG;
        data.icon = TRAY_ICON.load(Ordering::Relaxed);
        if let Ok(tip) = tray_tip().lock() {
            fill(&mut data.tip, &tip);
        }
        if let Some((title, text)) = info {
            fill(&mut data.info_title, title);
            fill(&mut data.info, text);
            data.info_flags = NIIF_NOSOUND;
        }
        Shell_NotifyIconW(op, &mut data);
    }
}

unsafe extern "system" fn tray_proc(hwnd: *mut c_void, msg: u32, wparam: usize, lparam: isize) -> isize {
    if msg == TRAY_MSG {
        let click = match lparam as u32 & 0xFFFF {
            WM_LBUTTONUP => Some(TrayClick::Left),
            WM_RBUTTONUP => Some(TrayClick::Right),
            WM_LBUTTONDBLCLK => Some(TrayClick::Double),
            _ => None,
        };
        if let Some(click) = click {
            if let Ok(guard) = tray_handler().lock() {
                if let Some(handler) = guard.as_ref() {
                    handler(click);
                }
            }
        }
        return 0;
    }
    let created = TASKBAR_CREATED.load(Ordering::Relaxed);
    if created != 0 && msg == created {
        // Explorer restarted and lost every tray icon.
        notify(NIM_ADD, NIF_MESSAGE | NIF_ICON | NIF_TIP, None);
        return 0;
    }
    DefWindowProcW(hwnd, msg, wparam, lparam)
}

#[repr(C)]
struct WndClass {
    size: u32,
    style: u32,
    wnd_proc: Option<unsafe extern "system" fn(*mut c_void, u32, usize, isize) -> isize>,
    cls_extra: i32,
    wnd_extra: i32,
    instance: *mut c_void,
    icon: *mut c_void,
    cursor: *mut c_void,
    brush: *mut c_void,
    menu: *const u16,
    class_name: *const u16,
    icon_sm: *mut c_void,
}

#[repr(C)]
struct NotifyIconData {
    size: u32,
    hwnd: *mut c_void,
    id: u32,
    flags: u32,
    callback_message: u32,
    icon: *mut c_void,
    tip: [u16; 128],
    state: u32,
    state_mask: u32,
    info: [u16; 256],
    timeout_or_version: u32,
    info_title: [u16; 64],
    info_flags: u32,
    guid: [u8; 16],
    balloon_icon: *mut c_void,
}

#[repr(C)]
struct ProcessEntry {
    size: u32,
    usage: u32,
    process_id: u32,
    default_heap_id: usize,
    module_id: u32,
    threads: u32,
    parent_process_id: u32,
    pri_class_base: i32,
    flags: u32,
    exe: [u16; 260],
}

#[repr(C)]
struct WinPoint {
    x: i32,
    y: i32,
}

#[repr(C)]
struct WinRect {
    left: i32,
    top: i32,
    right: i32,
    bottom: i32,
}

#[link(name = "user32")]
extern "system" {
    fn GetForegroundWindow() -> *mut c_void;
    fn GetAncestor(hwnd: *mut c_void, flags: u32) -> *mut c_void;
    fn SetForegroundWindow(hwnd: *mut c_void) -> i32;
    fn GetCursorPos(point: *mut WinPoint) -> i32;
    fn CreatePopupMenu() -> *mut c_void;
    fn AppendMenuW(menu: *mut c_void, flags: u32, id: usize, text: *const u16) -> i32;
    fn TrackPopupMenu(
        menu: *mut c_void,
        flags: u32,
        x: i32,
        y: i32,
        reserved: i32,
        hwnd: *mut c_void,
        rect: *const WinRect,
    ) -> i32;
    fn DestroyMenu(menu: *mut c_void) -> i32;
    fn PostMessageW(hwnd: *mut c_void, msg: u32, wparam: usize, lparam: isize) -> i32;
    fn RegisterClassExW(class: *const WndClass) -> u16;
    fn CreateWindowExW(
        ex_style: u32,
        class: *const u16,
        title: *const u16,
        style: u32,
        x: i32,
        y: i32,
        width: i32,
        height: i32,
        parent: *mut c_void,
        menu: *mut c_void,
        instance: *mut c_void,
        param: *mut c_void,
    ) -> *mut c_void;
    fn DefWindowProcW(hwnd: *mut c_void, msg: u32, wparam: usize, lparam: isize) -> isize;
    fn RegisterWindowMessageW(name: *const u16) -> u32;
    fn GetSystemMetrics(index: i32) -> i32;
    fn SystemParametersInfoW(action: u32, param: u32, data: *mut c_void, ini: u32) -> i32;
    fn CreateIcon(
        instance: *mut c_void,
        width: i32,
        height: i32,
        planes: u8,
        bits_pixel: u8,
        and_bits: *const u8,
        xor_bits: *const u8,
    ) -> *mut c_void;
    fn DestroyIcon(icon: *mut c_void) -> i32;
    fn MessageBoxW(hwnd: *mut c_void, text: *const u16, caption: *const u16, kind: u32) -> i32;
}

#[link(name = "shell32")]
extern "system" {
    fn Shell_NotifyIconW(message: u32, data: *mut NotifyIconData) -> i32;
    fn ShellExecuteW(
        hwnd: *mut c_void,
        verb: *const u16,
        file: *const u16,
        params: *const u16,
        dir: *const u16,
        show: i32,
    ) -> *mut c_void;
}

#[link(name = "kernel32")]
extern "system" {
    fn GetModuleHandleW(name: *const u16) -> *mut c_void;
    fn LoadLibraryW(name: *const u16) -> *mut c_void;
    fn GetProcAddress(module: *mut c_void, name: *const u8) -> *mut c_void;
    fn CreateToolhelp32Snapshot(flags: u32, process: u32) -> *mut c_void;
    fn Process32FirstW(snapshot: *mut c_void, entry: *mut ProcessEntry) -> i32;
    fn Process32NextW(snapshot: *mut c_void, entry: *mut ProcessEntry) -> i32;
    fn CloseHandle(handle: *mut c_void) -> i32;
}

#[link(name = "dwmapi")]
extern "system" {
    fn DwmSetWindowAttribute(hwnd: *mut c_void, attr: u32, value: *const c_void, size: u32) -> i32;
}
