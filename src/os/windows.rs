//! Windows 壁纸层：WorkerW 技术把 WebView 窗口挂为桌面壁纸的子窗。
//! 序列参考 `ownself/wewa`（公共技术，按思路自行重写）：给 `Progman` 发 `0x052C`
//! 令其在桌面图标背后生成 `WorkerW`，`SetParent` 把壁纸窗挂上去；退出时
//! `SetParent(NULL)` + 刷新桌面清残影。
//!
//! 24H2 起 `WorkerW` 开机后并非立即存在，故 spawn 消息发多次 + 轮询等待其出现。
//! ponytail: DWM 崩溃/壁纸切换后的自动重建尚未做——待 Windows 实测确认触发条件后补
//! （监听相应广播消息重跑 `attach_to_desktop`）。

use std::cell::Cell;
use std::ffi::c_void;
use std::time::Duration;

use tao::platform::windows::WindowExtWindows;
use tao::window::Window;
use windows::Win32::Foundation::{COLORREF, HWND, LPARAM, WPARAM};
use windows::Win32::UI::WindowsAndMessaging::{
    EnumWindows, FindWindowExW, FindWindowW, GWL_EXSTYLE, GWL_STYLE, HWND_BOTTOM, LWA_ALPHA,
    SMTO_NORMAL, SPI_SETDESKWALLPAPER, SPIF_UPDATEINIFILE, SW_HIDE, SWP_NOACTIVATE, SWP_SHOWWINDOW,
    SendMessageTimeoutW, SetLayeredWindowAttributes, SetParent, SetWindowLongPtrW, SetWindowPos,
    ShowWindow, SystemParametersInfoW, WS_CHILD, WS_EX_LAYERED, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW,
    WS_EX_TRANSPARENT,
};
use windows::core::{BOOL, PCWSTR, w};

/// Progman 私有消息：令其在桌面图标背后生成 `WorkerW`。
const SPAWN_WORKER_W: u32 = 0x052C;

/// 轮询等待 `WorkerW` 出现的次数与间隔（24H2 开机后可能延迟数百 ms 才就绪）。
const WORKER_W_TRIES: u32 = 20;
const WORKER_W_INTERVAL: Duration = Duration::from_millis(120);

thread_local! {
    /// 已找到的 `WorkerW` 句柄缓存（raw isize；0=未找到）。所有壁纸窗共挂它。
    static WORKER_W: Cell<isize> = const { Cell::new(0) };
}

fn hwnd_of(window: &Window) -> HWND {
    HWND(window.hwnd() as *mut c_void)
}

/// 枚举顶层窗，找到「有 `SHELLDLL_DefView` 子窗」的那个，取其下一个兄弟 `WorkerW`。
/// 桌面图标由 `SHELLDLL_DefView` 承载，其后紧邻的 `WorkerW` 正是壁纸绘制层。
unsafe extern "system" fn enum_cb(top: HWND, lparam: LPARAM) -> BOOL {
    let shell = unsafe { FindWindowExW(Some(top), None, w!("SHELLDLL_DefView"), PCWSTR::null()) };
    if let Ok(sh) = shell
        && !sh.is_invalid()
    {
        let worker = unsafe { FindWindowExW(None, Some(top), w!("WorkerW"), PCWSTR::null()) };
        if let Ok(wk) = worker
            && !wk.is_invalid()
        {
            unsafe { *(lparam.0 as *mut isize) = wk.0 as isize };
            return BOOL(0); // 找到，停止枚举
        }
    }
    BOOL(1) // 继续
}

fn find_worker_w() -> Option<HWND> {
    let mut found: isize = 0;
    let _ = unsafe { EnumWindows(Some(enum_cb), LPARAM(&mut found as *mut isize as isize)) };
    (found != 0).then_some(HWND(found as *mut c_void))
}

/// 确保 `WorkerW` 存在并返回其句柄：缓存命中直接返回；否则给 Progman 发 spawn 消息
/// 并轮询等待（应对 24H2 延迟就绪）。
fn ensure_worker_w() -> Option<HWND> {
    let cached = WORKER_W.get();
    if cached != 0 {
        return Some(HWND(cached as *mut c_void));
    }
    let progman = unsafe { FindWindowW(w!("Progman"), PCWSTR::null()) }.ok()?;
    if progman.is_invalid() {
        eprintln!("[win] 未找到 Progman，无法挂载壁纸");
        return None;
    }
    for attempt in 0..WORKER_W_TRIES {
        unsafe {
            let mut res: usize = 0;
            let _ = SendMessageTimeoutW(
                progman,
                SPAWN_WORKER_W,
                WPARAM(0xD),
                LPARAM(0x1),
                SMTO_NORMAL,
                1000,
                Some(&mut res),
            );
        }
        if let Some(worker) = find_worker_w() {
            WORKER_W.set(worker.0 as isize);
            eprintln!("[win] WorkerW 就绪（第 {} 次尝试）", attempt + 1);
            return Some(worker);
        }
        std::thread::sleep(WORKER_W_INTERVAL);
    }
    eprintln!("[win] {WORKER_W_TRIES} 次重试后仍未找到 WorkerW，壁纸可能不显示");
    None
}

/// 把窗口挂到 `WorkerW` 桌面壁纸层（子窗 + 隐于任务栏 + 不抢焦点 + 点击穿透），
/// 并按其原屏坐标铺满对应显示器。`screen_index` 仅用于日志。
pub fn attach_to_desktop(window: &Window, screen_index: usize) {
    let hwnd = hwnd_of(window);
    let pos = window.outer_position().unwrap_or_default();
    let size = window.outer_size();
    let Some(worker) = ensure_worker_w() else {
        return;
    };
    unsafe {
        SetWindowLongPtrW(hwnd, GWL_STYLE, WS_CHILD.0 as isize);
        let ex = (WS_EX_TOOLWINDOW.0 | WS_EX_NOACTIVATE.0 | WS_EX_TRANSPARENT.0 | WS_EX_LAYERED.0)
            as isize;
        SetWindowLongPtrW(hwnd, GWL_EXSTYLE, ex);
        // 需 WS_EX_LAYERED 先就位；初值全不透明，随后 set_alpha 驱动淡入。
        let _ = SetLayeredWindowAttributes(hwnd, COLORREF(0), 255, LWA_ALPHA);
        if SetParent(hwnd, Some(worker)).is_err() {
            eprintln!("[win] 显示器 {screen_index} SetParent 到 WorkerW 失败");
            return;
        }
        // SetParent 后坐标系变为 WorkerW 客户区（≈虚拟桌面），仍用原屏坐标定位。
        // -1/+2 补掉 WebView2 在上/左边缘的 1px 描边。
        let _ = SetWindowPos(
            hwnd,
            Some(HWND_BOTTOM),
            pos.x - 1,
            pos.y - 1,
            size.width as i32 + 2,
            size.height as i32 + 2,
            SWP_NOACTIVATE | SWP_SHOWWINDOW,
        );
    }
    eprintln!(
        "[win] 显示器 {screen_index} 已挂 WorkerW @ ({},{}) {}x{}",
        pos.x, pos.y, size.width, size.height
    );
}

/// 设窗口整体不透明度（0.0=全透明→1.0=不透明），驱动淡入。需窗口已有 WS_EX_LAYERED。
pub fn set_alpha(window: &Window, alpha: f64) {
    let a = (alpha.clamp(0.0, 1.0) * 255.0) as u8;
    let _ = unsafe { SetLayeredWindowAttributes(hwnd_of(window), COLORREF(0), a, LWA_ALPHA) };
}

/// 退出前把壁纸窗从 `WorkerW` 摘下并隐藏，防止残留子窗框在桌面上留影。
pub fn detach_from_desktop(window: &Window) {
    let hwnd = hwnd_of(window);
    unsafe {
        let _ = ShowWindow(hwnd, SW_HIDE);
        let _ = SetParent(hwnd, None);
    }
}

/// 刷新桌面壁纸，清掉 WorkerW 残影（退出清理最后一步）。
pub fn refresh_desktop() {
    let _ = unsafe { SystemParametersInfoW(SPI_SETDESKWALLPAPER, 0, None, SPIF_UPDATEINIFILE) };
}
