//! Keeps the main window off the screen while eframe builds it, on Windows.
//!
//! eframe creates the window hidden and shows it once the first frame is painted. But when the
//! saved window state is maximized, winit maximizes the new window with `ShowWindow(SW_MAXIMIZE)`,
//! which also shows it, and hides it again straight after; egui-winit then sets its size
//! (`SW_RESTORE`) and maximizes it again, each time showing and hiding it. So before the app has
//! drawn anything, a blank white window flashes up and disappears.
//!
//! DWM cloaking keeps a window off the screen without changing its Win32 visibility, so winit's
//! idea of the window stays right. [`install`] cloaks the main window as soon as it exists (when
//! its `WM_CREATE` returns, from a hook on this thread) and [`reveal`] uncloaks it once eframe has
//! built it. eframe still keeps it hidden until the first frame is painted, so the first thing on
//! screen is the app.

use std::cell::RefCell;
use std::ffi::c_void;
use std::ptr;

use windows_sys::Win32::Foundation::{LPARAM, LRESULT, WPARAM};
use windows_sys::Win32::Graphics::Dwm::{DWMWA_CLOAK, DwmSetWindowAttribute};
use windows_sys::Win32::System::Threading::GetCurrentThreadId;
use windows_sys::Win32::UI::WindowsAndMessaging::{
    CREATESTRUCTW, CWPRETSTRUCT, CallNextHookEx, HC_ACTION, HHOOK, SetWindowsHookExW,
    UnhookWindowsHookEx, WH_CALLWNDPROCRET, WM_CREATE,
};

struct State {
    hook: HHOOK,
    /// The window title as UTF-16, without the terminating zero.
    title: Vec<u16>,
    /// The main window, once created and cloaked.
    window: *mut c_void,
}

thread_local! {
    static STATE: RefCell<Option<State>> = const { RefCell::new(None) };
}

/// Cloaks the next top-level window this thread creates with the given title. Call it on the
/// thread that runs the event loop, before `eframe::run_native`.
pub fn install(title: &str) {
    // SAFETY: a hook for this thread only (so no module handle), with a procedure that lives for
    // the whole program.
    let hook = unsafe {
        SetWindowsHookExW(
            WH_CALLWNDPROCRET,
            Some(on_message_handled),
            ptr::null_mut(),
            GetCurrentThreadId(),
        )
    };
    if hook.is_null() {
        return;
    }
    STATE.with_borrow_mut(|state| {
        *state = Some(State {
            hook,
            title: title.encode_utf16().collect(),
            window: ptr::null_mut(),
        });
    });
}

/// Removes the hook and uncloaks the main window. Call it from the app creator: eframe has built
/// the window by then and keeps it hidden until the first frame is painted.
pub fn reveal() {
    let Some(state) = STATE.with_borrow_mut(Option::take) else {
        return;
    };
    // SAFETY: the hook came from `SetWindowsHookExW` and `take` above makes this the only removal.
    unsafe { UnhookWindowsHookEx(state.hook) };
    if !state.window.is_null() {
        set_cloak(state.window, false);
    }
}

fn set_cloak(window: *mut c_void, cloaked: bool) {
    let value = i32::from(cloaked);
    // SAFETY: `value` outlives the call and its size is passed along. A wrong handle only makes
    // DWM return an error, and a window that stays uncloaked is no worse than before.
    unsafe {
        DwmSetWindowAttribute(
            window,
            DWMWA_CLOAK as u32,
            (&raw const value).cast(),
            size_of::<i32>() as u32,
        );
    }
}

unsafe extern "system" fn on_message_handled(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code == HC_ACTION as i32 {
        // SAFETY: for HC_ACTION, `lparam` points to a valid CWPRETSTRUCT for this call.
        let message = unsafe { &*(lparam as *const CWPRETSTRUCT) };
        // WM_CREATE returning 0 means the window was created.
        if message.message == WM_CREATE && message.lResult == 0 {
            // SAFETY: for WM_CREATE, `lParam` points to the window's CREATESTRUCTW.
            let create = unsafe { &*(message.lParam as *const CREATESTRUCTW) };
            STATE.with(|state| {
                // Skip rather than panic across the FFI boundary should the state be borrowed.
                let Ok(mut state) = state.try_borrow_mut() else {
                    return;
                };
                let Some(state) = state.as_mut() else {
                    return;
                };
                let is_main = state.window.is_null()
                    && create.hwndParent.is_null()
                    && !create.lpszName.is_null()
                    // SAFETY: a non-null `lpszName` is a zero-terminated string.
                    && unsafe { wide_eq(create.lpszName, &state.title) };
                if is_main {
                    set_cloak(message.hwnd, true);
                    state.window = message.hwnd;
                }
            });
        }
    }
    // SAFETY: passing the call on, as every hook procedure must.
    unsafe { CallNextHookEx(ptr::null_mut(), code, wparam, lparam) }
}

/// Whether the zero-terminated UTF-16 string at `text` equals `expected`, which has no zeros.
///
/// # Safety
/// `text` must point to a zero-terminated UTF-16 string.
unsafe fn wide_eq(text: *const u16, expected: &[u16]) -> bool {
    for (i, &want) in expected.iter().enumerate() {
        // SAFETY: the units before `i` were non-zero (they matched `expected`), so `i` is at most
        // the terminator's index.
        if unsafe { *text.add(i) } != want {
            return false;
        }
    }
    // SAFETY: as above, `expected.len()` is at most the terminator's index.
    unsafe { *text.add(expected.len()) == 0 }
}
