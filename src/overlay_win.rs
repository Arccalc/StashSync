//! Goal icons as a layered window. Empty pixels stay empty, the way desktop icons do.
//! The egui window cannot do that on Windows: its empty area becomes a glass edge.

use std::collections::HashMap;
use std::ffi::{c_void, OsStr};
use std::os::windows::ffi::OsStrExt;
use std::sync::atomic::{AtomicBool, AtomicI32, AtomicPtr, Ordering};
use std::sync::Mutex;

use crate::state::OverlayTile;

const TILE: f32 = 58.0;
const GAP: f32 = 8.0;
const PER_LINE: usize = 8;
const SWP_NOSIZE: u32 = 0x0001;
const SWP_NOMOVE: u32 = 0x0002;
const WM_LBUTTONDOWN: u32 = 0x0201;
const WM_MOUSEMOVE: u32 = 0x0200;
const WM_LBUTTONUP: u32 = 0x0202;
const WM_SETCURSOR: u32 = 0x0020;
const IDC_SIZEALL: isize = 32646;
const ROW: f32 = 28.0;
const LIST_W: f32 = 280.0;
const LIST_ICON: f32 = 22.0;

const WS_POPUP: u32 = 0x8000_0000;
const WS_EX_TOPMOST: u32 = 0x0000_0008;
const WS_EX_TRANSPARENT: u32 = 0x0000_0020;
const WS_EX_TOOLWINDOW: u32 = 0x0000_0080;
const WS_EX_LAYERED: u32 = 0x0008_0000;
const WS_EX_NOACTIVATE: u32 = 0x0800_0000;
const SW_HIDE: i32 = 0;
const SWP_NOACTIVATE: u32 = 0x0010;
const SWP_SHOWWINDOW: u32 = 0x0040;
const ULW_ALPHA: u32 = 0x0000_0002;
const AC_SRC_ALPHA: u8 = 1;
const FW_NORMAL: i32 = 400;
const FW_MEDIUM: i32 = 500;
const ANTIALIASED: u32 = 4;
const TRANSPARENT_BK: i32 = 1;
const DT_VCENTER: u32 = 0x0004;
const DT_SINGLELINE: u32 = 0x0020;
const DT_NOCLIP: u32 = 0x0100;
const DT_NOPREFIX: u32 = 0x0800;
const DT_END_ELLIPSIS: u32 = 0x8000;

static WINDOW: AtomicPtr<c_void> = AtomicPtr::new(std::ptr::null_mut());
static FONTS: AtomicBool = AtomicBool::new(false);
static PLACE_X: AtomicI32 = AtomicI32::new(16);
static PLACE_Y: AtomicI32 = AtomicI32::new(16);
static DRAG_DX: AtomicI32 = AtomicI32::new(0);
static DRAG_DY: AtomicI32 = AtomicI32::new(0);
static DRAGGING: AtomicBool = AtomicBool::new(false);
static RELEASED: AtomicBool = AtomicBool::new(false);
static MOVABLE: AtomicBool = AtomicBool::new(false);

pub struct OverlayFrame {
    pub list: bool,
    pub vertical: bool,
    pub opacity: u8,
    pub scale_pct: u8,
    pub x: i32,
    pub y: i32,
    pub movable: bool,
}

struct IconPx {
    fingerprint: u64,
    side: u32,
    rgba: Vec<u8>,
    width: u32,
    height: u32,
}

fn icon_cache() -> &'static Mutex<HashMap<String, IconPx>> {
    static CACHE: std::sync::OnceLock<Mutex<HashMap<String, IconPx>>> = std::sync::OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

pub fn place_at(x: i32, y: i32) {
    DRAGGING.store(false, Ordering::Relaxed);
    RELEASED.store(false, Ordering::Relaxed);
    PLACE_X.store(x, Ordering::Relaxed);
    PLACE_Y.store(y, Ordering::Relaxed);
}

pub fn take_dragged() -> Option<(i32, i32)> {
    if RELEASED.swap(false, Ordering::Relaxed) {
        Some((
            PLACE_X.load(Ordering::Relaxed),
            PLACE_Y.load(Ordering::Relaxed),
        ))
    } else {
        None
    }
}

pub fn sync(open: bool, frame: &OverlayFrame, tiles: &[OverlayTile]) {
    if !open || tiles.is_empty() || frame.opacity == 0 {
        hide();
        return;
    }
    let user = f32::from(frame.scale_pct.clamp(50, 160)) / 100.0;
    let scale = dpi_scale() * user;
    if !DRAGGING.load(Ordering::Relaxed) {
        PLACE_X.store(frame.x, Ordering::Relaxed);
        PLACE_Y.store(frame.y, Ordering::Relaxed);
    }
    MOVABLE.store(frame.movable, Ordering::Relaxed);
    let (width, height) = if frame.list {
        (px(LIST_W, scale), px(ROW, scale) * tiles.len() as i32)
    } else {
        let (cols, rows) = grid_counts(tiles.len(), frame.vertical);
        (
            cols * px(TILE, scale) + (cols - 1).max(0) * px(GAP, scale),
            rows * px(TILE, scale) + (rows - 1).max(0) * px(GAP, scale),
        )
    };
    if width <= 0 || height <= 0 {
        hide();
        return;
    }
    let Some(hwnd) = window() else {
        return;
    };
    apply_mouse(hwnd, frame.movable);
    paint(
        hwnd,
        frame.list,
        frame.vertical,
        frame.opacity,
        tiles,
        scale,
        width,
        height,
    );
}

fn grid_counts(len: usize, vertical: bool) -> (i32, i32) {
    let n = len.max(1);
    if vertical {
        (n.div_ceil(PER_LINE) as i32, n.min(PER_LINE) as i32)
    } else {
        (n.min(PER_LINE) as i32, n.div_ceil(PER_LINE) as i32)
    }
}

fn slot(index: usize, vertical: bool) -> (i32, i32) {
    if vertical {
        ((index / PER_LINE) as i32, (index % PER_LINE) as i32)
    } else {
        ((index % PER_LINE) as i32, (index / PER_LINE) as i32)
    }
}

fn hide() {
    let hwnd = WINDOW.load(Ordering::Relaxed);
    if !hwnd.is_null() {
        unsafe { ShowWindow(hwnd, SW_HIDE) };
    }
}

fn dpi_scale() -> f32 {
    let dpi = unsafe { GetDpiForSystem() };
    if dpi < 96 { 1.0 } else { dpi as f32 / 96.0 }
}

fn px(points: f32, scale: f32) -> i32 {
    (points * scale).round().max(1.0) as i32
}

fn ensure_fonts() {
    if FONTS.swap(true, Ordering::Relaxed) {
        return;
    }
    unsafe {
        let class = wide("StashSyncGoalIcons");
        let instance = GetModuleHandleW(std::ptr::null());
        let info = WndClass {
            size: std::mem::size_of::<WndClass>() as u32,
            style: 0,
            wnd_proc: Some(wnd_proc),
            cls_extra: 0,
            wnd_extra: 0,
            instance,
            icon: std::ptr::null_mut(),
            cursor: std::ptr::null_mut(),
            brush: std::ptr::null_mut(),
            menu: std::ptr::null(),
            class_name: class.as_ptr(),
            icon_sm: std::ptr::null_mut(),
        };
        RegisterClassExW(&info);
        let mut count = 0u32;
        let inter = include_bytes!("../assets/fonts/Inter-Regular.ttf");
        let mono = include_bytes!("../assets/fonts/JetBrainsMono-Medium.ttf");
        AddFontMemResourceEx(
            inter.as_ptr() as *mut c_void,
            inter.len() as u32,
            std::ptr::null_mut(),
            &mut count,
        );
        AddFontMemResourceEx(
            mono.as_ptr() as *mut c_void,
            mono.len() as u32,
            std::ptr::null_mut(),
            &mut count,
        );
    }
}

fn window() -> Option<*mut c_void> {
    let existing = WINDOW.load(Ordering::Relaxed);
    if !existing.is_null() {
        return Some(existing);
    }
    ensure_fonts();
    unsafe {
        let class = wide("StashSyncGoalIcons");
        let title = wide("StashSync Goal Icons");
        let hwnd = CreateWindowExW(
            WS_EX_LAYERED | WS_EX_TRANSPARENT | WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE,
            class.as_ptr(),
            title.as_ptr(),
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
            return None;
        }
        WINDOW.store(hwnd, Ordering::Relaxed);
        Some(hwnd)
    }
}

fn paint(
    hwnd: *mut c_void,
    list: bool,
    vertical: bool,
    opacity: u8,
    tiles: &[OverlayTile],
    scale: f32,
    width: i32,
    height: i32,
) {
    unsafe {
        let screen = GetDC(std::ptr::null_mut());
        let info = BitmapInfo {
            header: BitmapHeader {
                size: 40,
                width,
                height: -height,
                planes: 1,
                bit_count: 32,
                compression: 0,
                size_image: 0,
                x_pels: 0,
                y_pels: 0,
                clr_used: 0,
                clr_important: 0,
            },
        };
        let mut bits: *mut u8 = std::ptr::null_mut();
        let dib = CreateDIBSection(screen, &info, 0, &mut bits, std::ptr::null_mut(), 0);
        if dib.is_null() || bits.is_null() {
            ReleaseDC(std::ptr::null_mut(), screen);
            return;
        }
        std::ptr::write_bytes(bits, 0, (width as usize) * (height as usize) * 4);
        let stride = width * 4;
        if list {
            paint_list(bits, stride, width, tiles, scale, opacity);
            paint_list_text(bits, stride, width, height, screen, tiles, scale, opacity);
        } else {
            paint_tiles(bits, stride, width, tiles, scale, opacity, vertical);
            paint_tile_text(
                bits, stride, width, height, screen, tiles, scale, opacity, vertical,
            );
        }
        // A layered window ignores fully clear pixels. The list has no plate, so
        // while Move is on the empty area gets alpha 1 and can be dragged.
        if MOVABLE.load(Ordering::Relaxed) {
            fill_grab_alpha(bits, width, height);
        }
        let mem = CreateCompatibleDC(screen);
        let old = SelectObject(mem, dib);
        let origin_x = PLACE_X.load(Ordering::Relaxed);
        let origin_y = PLACE_Y.load(Ordering::Relaxed);
        let mut place = WinPoint {
            x: origin_x,
            y: origin_y,
        };
        let mut size = WinSize { cx: width, cy: height };
        let mut src = WinPoint { x: 0, y: 0 };
        let blend = Blend {
            op: 0,
            flags: 0,
            alpha: 255,
            format: AC_SRC_ALPHA,
        };
        UpdateLayeredWindow(
            hwnd,
            screen,
            &mut place,
            &mut size,
            mem,
            &mut src,
            0,
            &blend,
            ULW_ALPHA,
        );
        // ShowWindow leaves this layered window hidden. SetWindowPos is what puts it on screen.
        if !DRAGGING.load(Ordering::Relaxed) {
            SetWindowPos(
                hwnd,
                -1isize as *mut c_void,
                origin_x,
                origin_y,
                width,
                height,
                SWP_NOACTIVATE | SWP_SHOWWINDOW,
            );
        }
        SelectObject(mem, old);
        DeleteDC(mem);
        DeleteObject(dib);
        ReleaseDC(std::ptr::null_mut(), screen);
    }
}

fn paint_tiles(
    bits: *mut u8,
    stride: i32,
    width: i32,
    tiles: &[OverlayTile],
    scale: f32,
    opacity: u8,
    vertical: bool,
) {
    let tile = px(TILE, scale);
    let gap = px(GAP, scale);
    let step = tile + gap;
    for (index, item) in tiles.iter().enumerate() {
        let (col, row) = slot(index, vertical);
        let x = col * step;
        let y = row * step;
        blit_icon(bits, stride, width, item, x, y, tile, opacity);
    }
}

fn paint_tile_text(
    bits: *mut u8,
    stride: i32,
    width: i32,
    height: i32,
    screen: *mut c_void,
    tiles: &[OverlayTile],
    scale: f32,
    opacity: u8,
    vertical: bool,
) {
    let tile = px(TILE, scale);
    let gap = px(GAP, scale);
    let step = tile + gap;
    let size = (13.0 * scale).round().max(8.0) as i32;
    let margin = px(3.0, scale);
    for (index, item) in tiles.iter().enumerate() {
        let (col, row) = slot(index, vertical);
        let text = item.left.to_string();
        let (tw, th) = text_size(screen, "JetBrains Mono", FW_MEDIUM, size, &text);
        let x = col * step + tile - tw - margin;
        let y = row * step + tile - th - px(2.0, scale);
        stamp_text(
            bits, stride, width, height, screen, &text, "JetBrains Mono", FW_MEDIUM, size, x, y, tw,
            th, (0xe8, 0xa3, 0x3d), opacity, DT_SINGLELINE | DT_NOCLIP | DT_NOPREFIX,
        );
    }
}

fn paint_list(bits: *mut u8, stride: i32, width: i32, tiles: &[OverlayTile], scale: f32, opacity: u8) {
    let row_h = px(ROW, scale);
    let icon = px(LIST_ICON, scale);
    for (index, item) in tiles.iter().enumerate() {
        let y = index as i32 * row_h;
        let icon_y = y + (row_h - icon) / 2;
        blit_icon(bits, stride, width, item, 0, icon_y, icon, opacity);
    }
}

fn paint_list_text(
    bits: *mut u8,
    stride: i32,
    width: i32,
    height: i32,
    screen: *mut c_void,
    tiles: &[OverlayTile],
    scale: f32,
    opacity: u8,
) {
    let row_h = px(ROW, scale);
    let icon = px(LIST_ICON, scale);
    let size = (14.0 * scale).round().max(8.0) as i32;
    let gap = px(8.0, scale);
    for (index, item) in tiles.iter().enumerate() {
        let y = index as i32 * row_h;
        let count = item.left.to_string();
        let (cw, ch) = text_size(screen, "JetBrains Mono", FW_MEDIUM, size, &count);
        let count_x = width - cw - px(2.0, scale);
        let count_y = y + (row_h - ch) / 2;
        stamp_text(
            bits, stride, width, height, screen, &count, "JetBrains Mono", FW_MEDIUM, size, count_x,
            count_y, cw, ch, (0xe8, 0xa3, 0x3d), opacity, DT_SINGLELINE | DT_NOCLIP | DT_NOPREFIX,
        );
        let text_x = icon + gap;
        let text_w = (count_x - gap - text_x).max(px(24.0, scale));
        stamp_text(
            bits,
            stride,
            width,
            height,
            screen,
            &item.label,
            "Inter",
            FW_NORMAL,
            size,
            text_x,
            y,
            text_w,
            row_h,
            (0xed, 0xed, 0xf0),
            opacity,
            DT_SINGLELINE | DT_VCENTER | DT_END_ELLIPSIS | DT_NOPREFIX,
        );
    }
}

fn blit_icon(
    bits: *mut u8,
    stride: i32,
    width: i32,
    tile: &OverlayTile,
    x: i32,
    y: i32,
    side: i32,
    opacity: u8,
) {
    let Some(icon) = icon_pixels(tile, side as u32) else {
        return;
    };
    let fade = u32::from(opacity.clamp(0, 100));
    for row in 0..icon.height as i32 {
        for col in 0..icon.width as i32 {
            let si = ((row as u32 * icon.width + col as u32) * 4) as usize;
            let sa = icon.rgba[si + 3] as u32 * fade / 100;
            if sa == 0 {
                continue;
            }
            let dx = x + col;
            let dy = y + row;
            if dx < 0 || dy < 0 || dx >= width {
                continue;
            }
            let di = (dy * stride + dx * 4) as usize;
            let src_r = icon.rgba[si] as u32 * sa / 255;
            let src_g = icon.rgba[si + 1] as u32 * sa / 255;
            let src_b = icon.rgba[si + 2] as u32 * sa / 255;
            unsafe {
                let pixel = bits.add(di);
                let inv = 255 - sa;
                let db = u32::from(*pixel);
                let dg = u32::from(*pixel.add(1));
                let dr = u32::from(*pixel.add(2));
                let da = u32::from(*pixel.add(3));
                *pixel = (src_b + db * inv / 255) as u8;
                *pixel.add(1) = (src_g + dg * inv / 255) as u8;
                *pixel.add(2) = (src_r + dr * inv / 255) as u8;
                *pixel.add(3) = (sa + da * inv / 255) as u8;
            }
        }
    }
}

fn icon_pixels(tile: &OverlayTile, side: u32) -> Option<IconPx> {
    if tile.png.is_empty() || side == 0 {
        return None;
    }
    let fingerprint = fingerprint(&tile.png);
    let mut cache = icon_cache().lock().unwrap_or_else(|err| err.into_inner());
    if let Some(hit) = cache.get(&tile.name) {
        if hit.fingerprint == fingerprint && hit.side == side {
            return Some(IconPx {
                fingerprint,
                side,
                rgba: hit.rgba.clone(),
                width: hit.width,
                height: hit.height,
            });
        }
    }
    let image = image::load_from_memory(&tile.png).ok()?;
    let image = image.resize_exact(side, side, image::imageops::FilterType::Triangle);
    let rgba = image.to_rgba8();
    let stored = IconPx {
        fingerprint,
        side,
        rgba: rgba.into_raw(),
        width: side,
        height: side,
    };
    cache.insert(tile.name.clone(), IconPx {
        fingerprint: stored.fingerprint,
        side: stored.side,
        rgba: stored.rgba.clone(),
        width: stored.width,
        height: stored.height,
    });
    Some(stored)
}

fn fingerprint(bytes: &[u8]) -> u64 {
    let mut hash = 0xcbf29ce484222325u64;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    hash
}

fn text_size(screen: *mut c_void, face: &str, weight: i32, size: i32, text: &str) -> (i32, i32) {
    if text.is_empty() || size <= 0 {
        return (1, 1);
    }
    unsafe {
        let dc = CreateCompatibleDC(screen);
        let font = make_font(face, weight, size);
        let old = SelectObject(dc, font);
        let mut measured = WinSize { cx: size, cy: size };
        let chars = wide(text);
        let len = text.encode_utf16().count() as i32;
        GetTextExtentPoint32W(dc, chars.as_ptr(), len, &mut measured);
        SelectObject(dc, old);
        DeleteObject(font);
        DeleteDC(dc);
        (measured.cx.max(1), measured.cy.max(1))
    }
}

/// Draw white letters on a black scratch, then color only those pixels.
/// A 32-bit text draw leaves the count invisible, so the scratch has no alpha channel.
fn stamp_text(
    dest: *mut u8,
    dest_stride: i32,
    dest_w: i32,
    dest_h: i32,
    screen: *mut c_void,
    text: &str,
    face: &str,
    weight: i32,
    size: i32,
    x: i32,
    y: i32,
    box_w: i32,
    box_h: i32,
    color: (u8, u8, u8),
    opacity: u8,
    flags: u32,
) {
    if text.is_empty() || box_w <= 0 || box_h <= 0 || opacity == 0 || size <= 0 {
        return;
    }
    unsafe {
        let stride = box_w * 4;
        let info = BitmapInfo {
            header: BitmapHeader {
                size: 40,
                width: box_w,
                height: -box_h,
                planes: 1,
                bit_count: 32,
                compression: 0,
                size_image: 0,
                x_pels: 0,
                y_pels: 0,
                clr_used: 0,
                clr_important: 0,
            },
        };
        let mut bits: *mut u8 = std::ptr::null_mut();
        let dib = CreateDIBSection(screen, &info, 0, &mut bits, std::ptr::null_mut(), 0);
        if dib.is_null() || bits.is_null() {
            return;
        }
        std::ptr::write_bytes(bits, 0, (stride * box_h) as usize);
        let dc = CreateCompatibleDC(screen);
        let old_dib = SelectObject(dc, dib);
        let font = make_font(face, weight, size);
        let old_font = SelectObject(dc, font);
        SetBkMode(dc, TRANSPARENT_BK);
        SetTextColor(dc, 0x00FF_FFFF);
        let mut rect = WinRect {
            left: 0,
            top: 0,
            right: box_w,
            bottom: box_h,
        };
        let chars = wide(text);
        let len = text.encode_utf16().count() as i32;
        DrawTextW(dc, chars.as_ptr(), len, &mut rect, flags);
        let fade = u32::from(opacity.clamp(0, 100));
        for row in 0..box_h {
            for col in 0..box_w {
                let sample = bits.add((row * stride + col * 4) as usize);
                let coverage = u32::from((*sample).max(*sample.add(1)).max(*sample.add(2)).max(*sample.add(3)));
                if coverage == 0 {
                    continue;
                }
                let dx = x + col;
                let dy = y + row;
                if dx < 0 || dy < 0 || dx >= dest_w || dy >= dest_h {
                    continue;
                }
                let sa = coverage * fade / 100;
                if sa == 0 {
                    continue;
                }
                let src_b = u32::from(color.2) * sa / 255;
                let src_g = u32::from(color.1) * sa / 255;
                let src_r = u32::from(color.0) * sa / 255;
                let pixel = dest.add((dy * dest_stride + dx * 4) as usize);
                let inv = 255 - sa;
                *pixel = (src_b + u32::from(*pixel) * inv / 255) as u8;
                *pixel.add(1) = (src_g + u32::from(*pixel.add(1)) * inv / 255) as u8;
                *pixel.add(2) = (src_r + u32::from(*pixel.add(2)) * inv / 255) as u8;
                *pixel.add(3) = (sa + u32::from(*pixel.add(3)) * inv / 255) as u8;
            }
        }
        SelectObject(dc, old_font);
        SelectObject(dc, old_dib);
        DeleteObject(font);
        DeleteDC(dc);
        DeleteObject(dib);
    }
}

fn make_font(face: &str, weight: i32, size: i32) -> *mut c_void {
    let name = wide(face);
    unsafe {
        CreateFontW(
            -size,
            0,
            0,
            0,
            weight,
            0,
            0,
            0,
            1,
            0,
            0,
            ANTIALIASED,
            0,
            name.as_ptr(),
        )
    }
}

fn wide(text: &str) -> Vec<u16> {
    OsStr::new(text).encode_wide().chain(std::iter::once(0)).collect()
}

fn fill_grab_alpha(bits: *mut u8, width: i32, height: i32) {
    if bits.is_null() || width <= 0 || height <= 0 {
        return;
    }
    let count = (width as usize).saturating_mul(height as usize);
    unsafe {
        for index in 0..count {
            let alpha = bits.add(index * 4 + 3);
            if *alpha == 0 {
                *alpha = 1;
            }
        }
    }
}

fn apply_mouse(hwnd: *mut c_void, movable: bool) {
    unsafe {
        let ex = GetWindowLongPtrW(hwnd, -20);
        let bit = WS_EX_TRANSPARENT as isize;
        let next = if movable { ex & !bit } else { ex | bit };
        if next != ex {
            SetWindowLongPtrW(hwnd, -20, next);
            SetWindowPos(
                hwnd,
                -1isize as *mut c_void,
                0,
                0,
                0,
                0,
                SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE | SWP_SHOWWINDOW,
            );
        }
    }
}

unsafe extern "system" fn wnd_proc(hwnd: *mut c_void, msg: u32, wparam: usize, lparam: isize) -> isize {
    if !MOVABLE.load(Ordering::Relaxed) {
        return DefWindowProcW(hwnd, msg, wparam, lparam);
    }
    match msg {
        WM_LBUTTONDOWN => {
            let mut cursor = WinPoint { x: 0, y: 0 };
            GetCursorPos(&mut cursor);
            DRAG_DX.store(cursor.x - PLACE_X.load(Ordering::Relaxed), Ordering::Relaxed);
            DRAG_DY.store(cursor.y - PLACE_Y.load(Ordering::Relaxed), Ordering::Relaxed);
            DRAGGING.store(true, Ordering::Relaxed);
            SetCapture(hwnd);
            0
        }
        WM_MOUSEMOVE if DRAGGING.load(Ordering::Relaxed) => {
            let mut cursor = WinPoint { x: 0, y: 0 };
            GetCursorPos(&mut cursor);
            let x = cursor.x - DRAG_DX.load(Ordering::Relaxed);
            let y = cursor.y - DRAG_DY.load(Ordering::Relaxed);
            PLACE_X.store(x, Ordering::Relaxed);
            PLACE_Y.store(y, Ordering::Relaxed);
            SetWindowPos(
                hwnd,
                -1isize as *mut c_void,
                x,
                y,
                0,
                0,
                SWP_NOSIZE | SWP_NOACTIVATE | SWP_SHOWWINDOW,
            );
            0
        }
        WM_LBUTTONUP => {
            if DRAGGING.swap(false, Ordering::Relaxed) {
                RELEASED.store(true, Ordering::Relaxed);
            }
            ReleaseCapture();
            0
        }
        WM_SETCURSOR => {
            let cursor = LoadCursorW(std::ptr::null_mut(), IDC_SIZEALL as *const u16);
            if !cursor.is_null() {
                SetCursor(cursor);
            }
            1
        }
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
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
struct BitmapHeader {
    size: u32,
    width: i32,
    height: i32,
    planes: u16,
    bit_count: u16,
    compression: u32,
    size_image: u32,
    x_pels: i32,
    y_pels: i32,
    clr_used: u32,
    clr_important: u32,
}

#[repr(C)]
struct BitmapInfo {
    header: BitmapHeader,
}

#[repr(C)]
struct WinPoint {
    x: i32,
    y: i32,
}

#[repr(C)]
struct WinSize {
    cx: i32,
    cy: i32,
}

#[repr(C)]
struct Blend {
    op: u8,
    flags: u8,
    alpha: u8,
    format: u8,
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
    fn ShowWindow(hwnd: *mut c_void, cmd: i32) -> i32;
    fn SetCapture(hwnd: *mut c_void) -> *mut c_void;
    fn ReleaseCapture() -> i32;
    fn GetCursorPos(point: *mut WinPoint) -> i32;
    fn LoadCursorW(instance: *mut c_void, name: *const u16) -> *mut c_void;
    fn SetCursor(cursor: *mut c_void) -> *mut c_void;
    fn GetWindowLongPtrW(hwnd: *mut c_void, index: i32) -> isize;
    fn SetWindowLongPtrW(hwnd: *mut c_void, index: i32, value: isize) -> isize;
    fn DrawTextW(dc: *mut c_void, text: *const u16, len: i32, rect: *mut WinRect, format: u32) -> i32;
    fn GetDC(hwnd: *mut c_void) -> *mut c_void;
    fn ReleaseDC(hwnd: *mut c_void, dc: *mut c_void) -> i32;
    fn UpdateLayeredWindow(
        hwnd: *mut c_void,
        dst: *mut c_void,
        pos: *mut WinPoint,
        size: *mut WinSize,
        src: *mut c_void,
        src_pos: *mut WinPoint,
        key: u32,
        blend: *const Blend,
        flags: u32,
    ) -> i32;
    fn GetDpiForSystem() -> u32;
    fn SetWindowPos(
        hwnd: *mut c_void,
        insert_after: *mut c_void,
        x: i32,
        y: i32,
        width: i32,
        height: i32,
        flags: u32,
    ) -> i32;
}

#[link(name = "kernel32")]
extern "system" {
    fn GetModuleHandleW(name: *const u16) -> *mut c_void;
}

#[link(name = "gdi32")]
extern "system" {
    fn CreateCompatibleDC(dc: *mut c_void) -> *mut c_void;
    fn DeleteDC(dc: *mut c_void) -> i32;
    fn SelectObject(dc: *mut c_void, object: *mut c_void) -> *mut c_void;
    fn DeleteObject(object: *mut c_void) -> i32;
    fn CreateDIBSection(
        dc: *mut c_void,
        info: *const BitmapInfo,
        usage: u32,
        bits: *mut *mut u8,
        section: *mut c_void,
        offset: u32,
    ) -> *mut c_void;
    fn AddFontMemResourceEx(
        data: *mut c_void,
        len: u32,
        reserved: *mut c_void,
        count: *mut u32,
    ) -> *mut c_void;
    fn CreateFontW(
        height: i32,
        width: i32,
        escapement: i32,
        orientation: i32,
        weight: i32,
        italic: u32,
        underline: u32,
        strike: u32,
        charset: u32,
        out_precision: u32,
        clip_precision: u32,
        quality: u32,
        pitch: u32,
        face: *const u16,
    ) -> *mut c_void;
    fn GetTextExtentPoint32W(dc: *mut c_void, text: *const u16, len: i32, size: *mut WinSize) -> i32;
    fn SetBkMode(dc: *mut c_void, mode: i32) -> i32;
    fn SetTextColor(dc: *mut c_void, color: u32) -> u32;
}
