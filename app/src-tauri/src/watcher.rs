//! Feeds the switcher: foreground-window changes (WinEvent hook) and, in `running` mode, a
//! 2 s process scan. Also lists top-level windows for "add running app".

use crate::state::{Shared, WindowInfo};
use crate::switcher::{basename, normalize};
use std::collections::HashSet;
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::sync::OnceLock;
use std::time::Duration;
use tauri::{AppHandle, Manager};
use windows_sys::core::BOOL;
use windows_sys::Win32::Foundation::{CloseHandle, HWND, INVALID_HANDLE_VALUE, LPARAM, TRUE};
use windows_sys::Win32::System::Diagnostics::ToolHelp::{
    CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W, TH32CS_SNAPPROCESS,
};
use windows_sys::Win32::System::Threading::{
    GetCurrentProcessId, OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32,
    PROCESS_QUERY_LIMITED_INFORMATION,
};
use windows_sys::Win32::UI::Accessibility::{SetWinEventHook, HWINEVENTHOOK};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    EnumWindows, GetForegroundWindow, GetWindow, GetWindowLongW, GetWindowTextLengthW,
    GetWindowTextW, GetWindowThreadProcessId, IsWindowVisible, EVENT_SYSTEM_FOREGROUND,
    GWL_EXSTYLE, GW_OWNER, WINEVENT_OUTOFCONTEXT, WINEVENT_SKIPOWNPROCESS, WS_EX_TOOLWINDOW,
};
use x3d_core::config::MatchMode;

const DEBOUNCE: Duration = Duration::from_millis(250);
const SCAN_INTERVAL: Duration = Duration::from_secs(2);

/// The WinEvent callback has no user data pointer, so it reaches the worker through this.
static FOREGROUND_PIDS: OnceLock<Sender<u32>> = OnceLock::new();

pub fn start(app: &AppHandle) {
    let (tx, rx) = mpsc::channel();
    if FOREGROUND_PIDS.set(tx).is_err() {
        return;
    }
    // Out-of-context WinEvents are delivered through the installing thread's message loop,
    // which on the main thread is Tauri's event loop.
    let hooked = app.run_on_main_thread(|| {
        // SAFETY: plain Win32 call; the callback is a 'static fn.
        let hook = unsafe {
            SetWinEventHook(
                EVENT_SYSTEM_FOREGROUND,
                EVENT_SYSTEM_FOREGROUND,
                std::ptr::null_mut(),
                Some(on_foreground),
                0,
                0,
                WINEVENT_OUTOFCONTEXT | WINEVENT_SKIPOWNPROCESS,
            )
        };
        if hook.is_null() {
            log::error!("SetWinEventHook failed; foreground switching disabled");
        }
    });
    if let Err(e) = hooked {
        log::error!("cannot install foreground hook: {e}");
    }
    // SAFETY: no preconditions.
    send_foreground_pid(unsafe { GetForegroundWindow() });

    let fg_app = app.clone();
    let scan_app = app.clone();
    let spawned = std::thread::Builder::new()
        .name("foreground".into())
        .spawn(move || foreground_worker(&fg_app, rx))
        .and_then(|_| {
            std::thread::Builder::new()
                .name("process-scan".into())
                .spawn(move || scan_worker(&scan_app))
        });
    if let Err(e) = spawned {
        log::error!("cannot start watcher threads: {e}");
    }
}

unsafe extern "system" fn on_foreground(
    _hook: HWINEVENTHOOK,
    _event: u32,
    hwnd: HWND,
    _id_object: i32,
    _id_child: i32,
    _thread: u32,
    _time: u32,
) {
    send_foreground_pid(hwnd);
}

fn send_foreground_pid(hwnd: HWND) {
    let mut pid = 0;
    // SAFETY: `pid` outlives the call; a stale hwnd just yields 0.
    unsafe { GetWindowThreadProcessId(hwnd, &mut pid) };
    if pid != 0 {
        if let Some(tx) = FOREGROUND_PIDS.get() {
            let _ = tx.send(pid);
        }
    }
}

fn foreground_worker(app: &AppHandle, rx: Receiver<u32>) {
    while let Ok(mut pid) = rx.recv() {
        // Alt-tab and launchers flash several windows; act on the one that stays.
        loop {
            match rx.recv_timeout(DEBOUNCE) {
                Ok(p) => pid = p,
                Err(RecvTimeoutError::Timeout) => break,
                Err(RecvTimeoutError::Disconnected) => return,
            }
        }
        let exe = process_path(pid).or_else(|| {
            // Protected processes refuse even limited queries; their basename still matches.
            snapshot()
                .into_iter()
                .find(|&(p, _)| p == pid)
                .map(|(_, name)| name)
        });
        let switched = {
            let shared = app.state::<Shared>();
            let mut s = shared.lock();
            s.switcher.set_foreground(exe);
            s.reevaluate()
        };
        if switched.is_some() {
            crate::notify(app, switched);
        }
    }
}

fn scan_worker(app: &AppHandle) {
    loop {
        std::thread::sleep(SCAN_INTERVAL);
        let wanted: HashSet<String> = {
            let shared = app.state::<Shared>();
            let s = shared.lock();
            if s.settings.match_mode != MatchMode::Running {
                continue;
            }
            s.store
                .list()
                .iter()
                .flat_map(|p| &p.exe_paths)
                .map(|e| basename(&normalize(e)).to_string())
                .collect()
        };
        // Only candidates get the (slower) full-path query.
        let running = snapshot()
            .into_iter()
            .filter(|(_, name)| wanted.contains(&name.to_lowercase()))
            .map(|(pid, name)| process_path(pid).unwrap_or(name))
            .collect();
        let switched = {
            let shared = app.state::<Shared>();
            let mut s = shared.lock();
            s.switcher.set_running(running);
            s.reevaluate()
        };
        if switched.is_some() {
            crate::notify(app, switched);
        }
    }
}

/// (pid, exe basename) of every process.
fn snapshot() -> Vec<(u32, String)> {
    let mut out = Vec::new();
    // SAFETY: the snapshot handle is checked and closed; `entry.dwSize` is set as required.
    unsafe {
        let snap = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0);
        if snap == INVALID_HANDLE_VALUE {
            return out;
        }
        let mut entry = PROCESSENTRY32W {
            dwSize: std::mem::size_of::<PROCESSENTRY32W>() as u32,
            ..Default::default()
        };
        let mut ok = Process32FirstW(snap, &mut entry);
        while ok != 0 {
            let len = entry
                .szExeFile
                .iter()
                .position(|&c| c == 0)
                .unwrap_or(entry.szExeFile.len());
            out.push((
                entry.th32ProcessID,
                String::from_utf16_lossy(&entry.szExeFile[..len]),
            ));
            ok = Process32NextW(snap, &mut entry);
        }
        CloseHandle(snap);
    }
    out
}

fn process_path(pid: u32) -> Option<String> {
    // SAFETY: the process handle is checked and closed; `len` bounds the buffer.
    unsafe {
        let h = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
        if h.is_null() {
            return None;
        }
        let mut buf = [0u16; 1024];
        let mut len = buf.len() as u32;
        let ok = QueryFullProcessImageNameW(h, PROCESS_NAME_WIN32, buf.as_mut_ptr(), &mut len);
        CloseHandle(h);
        (ok != 0).then(|| String::from_utf16_lossy(&buf[..len as usize]))
    }
}

/// Visible, titled, unowned, non-tool top-level windows of other processes.
pub fn list_windows() -> Vec<WindowInfo> {
    unsafe extern "system" fn collect(hwnd: HWND, lparam: LPARAM) -> BOOL {
        (*(lparam as *mut Vec<HWND>)).push(hwnd);
        TRUE
    }
    let mut hwnds: Vec<HWND> = Vec::new();
    // SAFETY: `hwnds` outlives the synchronous enumeration that writes to it.
    unsafe { EnumWindows(Some(collect), &mut hwnds as *mut Vec<HWND> as LPARAM) };
    // SAFETY: no preconditions.
    let own = unsafe { GetCurrentProcessId() };
    hwnds
        .into_iter()
        .filter_map(|hwnd| {
            // SAFETY: window handles from EnumWindows; stale ones make these calls fail harmlessly.
            unsafe {
                if IsWindowVisible(hwnd) == 0
                    || !GetWindow(hwnd, GW_OWNER).is_null()
                    || GetWindowLongW(hwnd, GWL_EXSTYLE) as u32 & WS_EX_TOOLWINDOW != 0
                {
                    return None;
                }
                let len = GetWindowTextLengthW(hwnd);
                if len <= 0 {
                    return None;
                }
                let mut title = vec![0u16; len as usize + 1];
                let n = GetWindowTextW(hwnd, title.as_mut_ptr(), title.len() as i32);
                let mut pid = 0;
                GetWindowThreadProcessId(hwnd, &mut pid);
                if pid == own {
                    return None;
                }
                Some(WindowInfo {
                    pid,
                    exe_path: process_path(pid)?,
                    title: String::from_utf16_lossy(&title[..n.max(0) as usize]),
                })
            }
        })
        .collect()
}
