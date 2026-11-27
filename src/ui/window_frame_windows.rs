use std::cell::Cell;

use eframe::egui::{self, Rect, pos2, vec2};
use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, POINT, WPARAM};
use windows_sys::Win32::Graphics::Gdi::ScreenToClient;
use windows_sys::Win32::UI::Shell::{DefSubclassProc, RemoveWindowSubclass, SetWindowSubclass};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    GWL_STYLE, GetForegroundWindow, GetWindowLongW, HTCAPTION, HTCLIENT, HWND_BOTTOM, IsIconic,
    NCCALCSIZE_PARAMS, PostMessageW, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOOWNERZORDER, SWP_NOSIZE,
    SetWindowPos, WM_ACTIVATEAPP, WM_APP, WM_NCCALCSIZE, WM_NCDESTROY, WM_NCHITTEST, WS_CAPTION,
    WS_THICKFRAME,
};

const CAPTION_ID: &str = "redox_native_caption";
const YIELD_FULLSCREEN: u32 = WM_APP + 0x36;

thread_local! {
    static WINDOW: Cell<HWND> = const { Cell::new(std::ptr::null_mut()) };
    static CAPTION: Cell<Option<Rect>> = const { Cell::new(None) };
    static TERMINAL_FULLSCREEN: Cell<bool> = const { Cell::new(false) };
}

pub(super) fn set_terminal_fullscreen(fullscreen: bool) {
    if !WINDOW.get().is_null() {
        TERMINAL_FULLSCREEN.set(fullscreen);
    }
}

fn is_fullscreen_style(requested: bool, style: u32) -> bool {
    requested && style & (WS_CAPTION | WS_THICKFRAME) == 0
}

fn is_terminal_fullscreen(window: HWND) -> bool {
    WINDOW.get() == window
        && TERMINAL_FULLSCREEN.get()
        && is_fullscreen_style(true, unsafe { GetWindowLongW(window, GWL_STYLE) } as u32)
}

fn should_yield_fullscreen(fullscreen: bool, minimized: bool, is_foreground: bool) -> bool {
    fullscreen && !minimized && !is_foreground
}

fn window_handle(owner: &impl HasWindowHandle) -> Option<HWND> {
    match owner.window_handle().ok()?.as_raw() {
        RawWindowHandle::Win32(handle) => Some(handle.hwnd.get() as HWND),
        _ => None,
    }
}

pub(super) fn begin_frame(context: &egui::Context, frame: &eframe::Frame) {
    let active = window_handle(frame).is_some_and(|window| {
        if WINDOW.get() == window {
            return true;
        }
        if unsafe { SetWindowSubclass(window, Some(caption_hit_test), 0, 0) } == 0 {
            return false;
        }
        WINDOW.set(window);
        true
    });
    context.data_mut(|data| {
        data.insert_temp(egui::Id::new(CAPTION_ID), (active, None::<Rect>));
    });
}

pub(super) fn register_caption(context: &egui::Context, response: &egui::Response) -> bool {
    if context.viewport_id() != egui::ViewportId::ROOT {
        return false;
    }
    let viewport = context.viewport_rect();
    let maximized = context.input(|input| input.viewport().maximized.unwrap_or(false));
    let bounds = caption_bounds(viewport, response.interact_rect, maximized);
    context.data_mut(|data| {
        let entry = data.get_temp_mut_or(egui::Id::new(CAPTION_ID), (false, None::<Rect>));
        if entry.0 && response.enabled() && bounds.is_positive() {
            entry.1 = Some(bounds.translate(-viewport.min.to_vec2()));
        }
        entry.0
    })
}

fn caption_bounds(viewport: Rect, caption: Rect, maximized: bool) -> Rect {
    if maximized {
        return caption.intersect(viewport);
    }
    caption.intersect(Rect::from_min_max(
        viewport.min + vec2(6.0, 3.0),
        viewport.max - vec2(6.0, 6.0),
    ))
}

pub(super) fn end_frame(context: &egui::Context) {
    let caption = context
        .data(|data| data.get_temp::<(bool, Option<Rect>)>(egui::Id::new(CAPTION_ID)))
        .and_then(|(_, caption)| caption);
    let scale = context.pixels_per_point();
    CAPTION.set(caption.map(|rect| {
        Rect::from_min_max(
            pos2(rect.min.x * scale, rect.min.y * scale),
            pos2(rect.max.x * scale, rect.max.y * scale),
        )
    }));
}

unsafe extern "system" fn caption_hit_test(
    window: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
    subclass_id: usize,
    _data: usize,
) -> LRESULT {
    match message {
        WM_NCCALCSIZE if wparam != 0 && lparam != 0 && is_terminal_fullscreen(window) => {
            // Keep the OpenGL surface composited during fullscreen transitions. A monitor-sized
            // client can leave Windows displaying the previous frame after a resize.
            let params = unsafe { &mut *(lparam as *mut NCCALCSIZE_PARAMS) };
            if params.rgrc[0].bottom > params.rgrc[0].top + 1 {
                params.rgrc[0].bottom -= 1;
            }
            return 0;
        }
        WM_ACTIVATEAPP if wparam == 0 && is_terminal_fullscreen(window) => {
            unsafe { PostMessageW(window, YIELD_FULLSCREEN, 0, 0) };
        }
        YIELD_FULLSCREEN => {
            if should_yield_fullscreen(
                is_terminal_fullscreen(window),
                unsafe { IsIconic(window) } != 0,
                unsafe { GetForegroundWindow() } == window,
            ) {
                unsafe {
                    SetWindowPos(
                        window,
                        HWND_BOTTOM,
                        0,
                        0,
                        0,
                        0,
                        SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE | SWP_NOOWNERZORDER,
                    )
                };
            }
            return 0;
        }
        _ => {}
    }
    if message == WM_NCDESTROY {
        unsafe { RemoveWindowSubclass(window, Some(caption_hit_test), subclass_id) };
        if WINDOW.get() == window {
            WINDOW.set(std::ptr::null_mut());
            CAPTION.set(None);
            TERMINAL_FULLSCREEN.set(false);
        }
    }
    let result = unsafe { DefSubclassProc(window, message, wparam, lparam) };
    if message == WM_NCHITTEST
        && result == HTCLIENT as LRESULT
        && WINDOW.get() == window
        && let Some(caption) = CAPTION.get()
    {
        let mut point = POINT {
            x: i32::from(lparam as i16),
            y: i32::from((lparam >> 16) as i16),
        };
        if unsafe { ScreenToClient(window, &mut point) } != 0
            && caption.contains(pos2(point.x as f32, point.y as f32))
        {
            return HTCAPTION as LRESULT;
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fullscreen_policy_does_not_apply_to_snap_or_maximize() {
        use windows_sys::Win32::UI::WindowsAndMessaging::WS_MAXIMIZE;

        assert!(is_fullscreen_style(true, 0));
        assert!(is_fullscreen_style(true, WS_MAXIMIZE));
        assert!(!is_fullscreen_style(true, WS_CAPTION | WS_THICKFRAME));
        assert!(!is_fullscreen_style(false, 0));
        assert!(should_yield_fullscreen(true, false, false));
        assert!(!should_yield_fullscreen(true, false, true));
        assert!(!should_yield_fullscreen(true, true, false));
        assert!(!should_yield_fullscreen(false, false, false));
    }

    #[test]
    fn native_caption_preserves_resize_edges_and_stops_before_window_buttons() {
        let viewport = Rect::from_min_size(pos2(0.0, 0.0), vec2(540.0, 640.0));
        let title = Rect::from_min_max(pos2(3.0, 1.0), pos2(470.0, 26.0));
        let caption = caption_bounds(viewport, title, false);
        assert!(caption.contains(pos2(100.0, 12.0)));
        assert!(!caption.contains(pos2(100.0, 1.0)));
        assert!(!caption.contains(pos2(4.0, 12.0)));
        assert!(!caption.contains(pos2(500.0, 12.0)));
        assert!(caption_bounds(viewport, title, true).contains(pos2(100.0, 1.0)));
    }
}
