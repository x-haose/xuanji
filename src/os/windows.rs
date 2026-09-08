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
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use tao::platform::windows::WindowExtWindows;
use tao::window::Window;
use windows::Win32::Foundation::{COLORREF, HWND, LPARAM, POINT, RECT, WPARAM};
use windows::Win32::Graphics::Dwm::{
    DWM_SYSTEMBACKDROP_TYPE, DWMSBT_TRANSIENTWINDOW, DWMWA_SYSTEMBACKDROP_TYPE,
    DwmExtendFrameIntoClientArea, DwmSetWindowAttribute,
};
use windows::Win32::Graphics::Gdi::{
    EnumDisplayMonitors, GetMonitorInfoW, HDC, HMONITOR, MONITOR_DEFAULTTONEAREST, MONITORINFO,
    MonitorFromWindow,
};
use windows::Win32::System::Registry::{
    HKEY, HKEY_CURRENT_USER, KEY_SET_VALUE, REG_OPTION_NON_VOLATILE, REG_SZ, RegCloseKey,
    RegCreateKeyExW, RegDeleteValueW, RegSetValueExW,
};
use windows::Win32::UI::Controls::MARGINS;
use windows::Win32::UI::WindowsAndMessaging::{
    EnumWindows, FindWindowExW, FindWindowW, GWL_EXSTYLE, GWL_STYLE, GetClassNameW, GetCursorPos,
    GetForegroundWindow, GetWindowRect, HWND_BOTTOM, LWA_ALPHA, SMTO_NORMAL, SPI_SETDESKWALLPAPER,
    SPIF_UPDATEINIFILE, SW_HIDE, SWP_NOACTIVATE, SWP_SHOWWINDOW, SendMessageTimeoutW,
    SetLayeredWindowAttributes, SetParent, SetWindowLongPtrW, SetWindowPos, ShowWindow,
    SystemParametersInfoW, WS_CHILD, WS_EX_LAYERED, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW,
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

/// 经典结构（≤23H2）：顶层 WorkerW —— `SHELLDLL_DefView` 宿主的下一个兄弟。
fn find_worker_classic() -> Option<HWND> {
    let mut found: isize = 0;
    let _ = unsafe { EnumWindows(Some(enum_cb), LPARAM(&mut found as *mut isize as isize)) };
    (found != 0).then_some(HWND(found as *mut c_void))
}

/// 多策略找 WorkerW：先看 Progman 的直接子窗（24H2 常把 WorkerW 挂在 Progman 下），
/// 再退回经典的顶层兄弟结构。
fn find_worker_w(progman: HWND) -> Option<HWND> {
    let child = unsafe { FindWindowExW(Some(progman), None, w!("WorkerW"), PCWSTR::null()) };
    if let Ok(worker) = child
        && !worker.is_invalid()
    {
        eprintln!("[win] WorkerW 在 Progman 下（24H2 式）");
        return Some(worker);
    }
    if let Some(worker) = find_worker_classic() {
        eprintln!("[win] WorkerW 为顶层兄弟（经典式）");
        return Some(worker);
    }
    None
}

/// 取窗口类名（诊断用）。
fn class_of(hwnd: HWND) -> String {
    let mut buf = [0u16; 256];
    let n = unsafe { GetClassNameW(hwnd, &mut buf) };
    String::from_utf16_lossy(&buf[..n.max(0) as usize])
}

/// 找不到 WorkerW 时打印实际窗口结构，供远程诊断 Windows 版本差异。
fn diagnose(progman: HWND) {
    eprintln!("[win][诊断] Progman 直接子窗：");
    let mut prev: Option<HWND> = None;
    loop {
        let next = unsafe { FindWindowExW(Some(progman), prev, PCWSTR::null(), PCWSTR::null()) };
        match next {
            Ok(c) if !c.is_invalid() => {
                eprintln!("[win][诊断]   {} ({})", class_of(c), c.0 as isize);
                prev = Some(c);
            }
            _ => break,
        }
    }
    eprintln!("[win][诊断] 顶层 WorkerW（及是否含 SHELLDLL_DefView）：");
    let _ = unsafe { EnumWindows(Some(diag_cb), LPARAM(0)) };
}

unsafe extern "system" fn diag_cb(top: HWND, _: LPARAM) -> BOOL {
    if class_of(top) == "WorkerW" {
        let shell =
            unsafe { FindWindowExW(Some(top), None, w!("SHELLDLL_DefView"), PCWSTR::null()) };
        let has_shell = shell.map(|h| !h.is_invalid()).unwrap_or(false);
        eprintln!(
            "[win][诊断]   WorkerW {} 含 SHELLDLL_DefView={}",
            top.0 as isize, has_shell
        );
    }
    BOOL(1)
}

/// 确保 `WorkerW` 存在并返回其句柄：缓存命中直接返回；否则给 Progman 发 spawn 消息
/// 并轮询等待（应对 24H2 延迟就绪 / 结构差异）。
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
        if let Some(worker) = find_worker_w(progman) {
            WORKER_W.set(worker.0 as isize);
            eprintln!("[win] WorkerW 就绪（第 {} 次尝试）", attempt + 1);
            return Some(worker);
        }
        std::thread::sleep(WORKER_W_INTERVAL);
    }
    eprintln!("[win] {WORKER_W_TRIES} 次重试后仍未找到 WorkerW；打印窗口结构诊断：");
    diagnose(progman);
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

/// 鼠标轮询线程的存活标志；此 guard 落时置 false 让线程退出。
pub struct MouseGuard(Arc<AtomicBool>);
impl Drop for MouseGuard {
    fn drop(&mut self) {
        self.0.store(false, Ordering::Relaxed);
    }
}

/// 全局鼠标监视：壁纸窗点击穿透收不到鼠标事件，故起后台线程按 ~60Hz 轮询 `GetCursorPos`
/// （虚拟屏坐标，多屏可为负）。无钩子、无授权、不破穿透——对应 mac 的 NSEvent 全局监视器。
pub fn install_mouse_monitor<F: Fn(f64, f64) + Send + 'static>(on_move: F) -> Option<MouseGuard> {
    let alive = Arc::new(AtomicBool::new(true));
    let flag = alive.clone();
    std::thread::spawn(move || {
        let mut last = (i32::MIN, i32::MIN);
        while flag.load(Ordering::Relaxed) {
            let mut p = POINT::default();
            // SAFETY: p 是有效可写 POINT；GetCursorPos 仅写入它。
            if unsafe { GetCursorPos(&mut p) }.is_ok() && (p.x, p.y) != last {
                last = (p.x, p.y);
                on_move(p.x as f64, p.y as f64);
            }
            std::thread::sleep(Duration::from_millis(16));
        }
    });
    Some(MouseGuard(alive))
}

unsafe extern "system" fn collect_monitor(
    _h: HMONITOR,
    _dc: HDC,
    rc: *mut RECT,
    lp: LPARAM,
) -> BOOL {
    // SAFETY: lp 由 enum_monitors 传入，指向该栈上的 Vec<RECT>；rc 指向本块显示器 rcMonitor。
    let out = unsafe { &mut *(lp.0 as *mut Vec<RECT>) };
    out.push(unsafe { *rc });
    true.into()
}

/// 枚举所有显示器的虚拟屏矩形（顺序对应 tao `available_monitors`——单屏必对；
/// ponytail: 多屏顺序待真机核，若与 tao 不一致改按 position 匹配）。
fn enum_monitors() -> Vec<RECT> {
    let mut out: Vec<RECT> = Vec::new();
    // SAFETY: 回调把每块 rcMonitor 收进 out，经 lparam 传其可变引用；调用期间 out 存活。
    unsafe {
        let _ = EnumDisplayMonitors(
            None,
            None,
            Some(collect_monitor),
            LPARAM(&mut out as *mut _ as isize),
        );
    }
    out
}

/// 全局虚拟屏坐标 → 第 `index` 屏归一化坐标（x 左→右、y 下→上，与 mac 约定一致，
/// 故鼠标特效在两平台上下不颠倒）。光标不在该屏时分量越界，调用方据此判断。
pub fn screen_norm(x: f64, y: f64, index: usize) -> Option<(f64, f64)> {
    let r = *enum_monitors().get(index)?;
    let w = (r.right - r.left) as f64;
    let h = (r.bottom - r.top) as f64;
    if w <= 0.0 || h <= 0.0 {
        return None;
    }
    Some(((x - r.left as f64) / w, (r.bottom as f64 - y) / h))
}

/// 开机自启：写/删注册表 `HKCU\...\Run` 下的 `Xuanji` 值（当前用户级，无需管理员）。
/// 幂等：`true` 写当前 exe 路径（带引号，含空格也安全）、`false` 删除该值。
pub fn set_autostart(enabled: bool) {
    let exe = match std::env::current_exe() {
        Ok(p) => p,
        Err(e) => {
            eprintln!("[win] 取 exe 路径失败，开机自启未变更: {e}");
            return;
        }
    };
    // SAFETY: 句柄由 RegCreateKeyExW 写出后仅本函数内使用并在结束前 RegCloseKey；
    // REG_SZ 数据传含结尾 NUL 的 UTF-16 字节切片，长度按字节数。
    unsafe {
        let mut hkey = HKEY::default();
        let rc = RegCreateKeyExW(
            HKEY_CURRENT_USER,
            w!("Software\\Microsoft\\Windows\\CurrentVersion\\Run"),
            None,
            PCWSTR::null(),
            REG_OPTION_NON_VOLATILE,
            KEY_SET_VALUE,
            None,
            &mut hkey,
            None,
        );
        if rc.is_err() {
            eprintln!("[win] 打开 Run 注册表键失败: {rc:?}");
            return;
        }
        if enabled {
            let mut wide: Vec<u16> = format!("\"{}\"", exe.display()).encode_utf16().collect();
            wide.push(0);
            let bytes = std::slice::from_raw_parts(wide.as_ptr() as *const u8, wide.len() * 2);
            let _ = RegSetValueExW(hkey, w!("Xuanji"), None, REG_SZ, Some(bytes));
        } else {
            let _ = RegDeleteValueW(hkey, w!("Xuanji")); // 不存在返错也无妨
        }
        let _ = RegCloseKey(hkey);
    }
}

/// 前台是否有全屏应用（窗矩形铺满其所在显示器，且非桌面/任务栏）。含无边框全屏游戏/视频。
pub fn foreground_fullscreen() -> bool {
    // SAFETY: 只读前台窗与其显示器信息，句柄即用即弃；MONITORINFO 先置 cbSize。
    unsafe {
        let fg = GetForegroundWindow();
        if fg.0.is_null() {
            return false;
        }
        match class_of(fg).as_str() {
            "WorkerW" | "Progman" | "Shell_TrayWnd" | "" => return false, // 桌面/任务栏不算
            _ => {}
        }
        let mut wr = RECT::default();
        if GetWindowRect(fg, &mut wr).is_err() {
            return false;
        }
        let mon = MonitorFromWindow(fg, MONITOR_DEFAULTTONEAREST);
        let mut mi = MONITORINFO {
            cbSize: std::mem::size_of::<MONITORINFO>() as u32,
            ..Default::default()
        };
        if !GetMonitorInfoW(mon, &mut mi).as_bool() {
            return false;
        }
        let m = mi.rcMonitor;
        wr.left <= m.left && wr.top <= m.top && wr.right >= m.right && wr.bottom >= m.bottom
    }
}

/// 给透明设置窗加 DWM 系统背景（Win11 Acrylic 毛玻璃）。Win10 无此属性，调用被忽略、
/// 退化为普通透明窗——对应 mac 的 `NSVisualEffectView`。
pub fn add_vibrancy(window: &Window) {
    let hwnd = hwnd_of(window);
    let backdrop = DWMSBT_TRANSIENTWINDOW;
    // margins 全 -1 = 把玻璃薄片延展进整个客户区。缺这步则系统背景材质只在非客户区合成，
    // 客户区仍是默认不透明底、材质透不出来（Acrylic 看不见的主因）。
    let margins = MARGINS {
        cxLeftWidth: -1,
        cxRightWidth: -1,
        cyTopHeight: -1,
        cyBottomHeight: -1,
    };
    // SAFETY: hwnd 有效；pvAttribute/pMarInset 指向本栈上有效值，长度按类型大小传。
    unsafe {
        let _ = DwmExtendFrameIntoClientArea(hwnd, &margins);
        let _ = DwmSetWindowAttribute(
            hwnd,
            DWMWA_SYSTEMBACKDROP_TYPE,
            &backdrop as *const _ as *const c_void,
            std::mem::size_of::<DWM_SYSTEMBACKDROP_TYPE>() as u32,
        );
    }
}
