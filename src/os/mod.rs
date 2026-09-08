//! 壁纸层：把 WebView 窗口沉到桌面图标背后。

#[cfg(target_os = "macos")]
mod macos;

#[cfg(target_os = "windows")]
mod windows;

/// 跨平台系统托盘（tray-icon）。见 `tray` 模块。
pub mod tray;

/// 将窗口挂到第 `screen_index` 块显示器的桌面壁纸层（图标背后、跨 Space、点击穿透，
/// 并铺满该屏）。mac=沉 NSWindow 层，Windows=WorkerW 子窗；其它平台 no-op。
pub fn attach_to_desktop(window: &tao::window::Window, screen_index: usize) {
    #[cfg(target_os = "macos")]
    macos::attach_to_desktop(window, screen_index);
    #[cfg(target_os = "windows")]
    windows::attach_to_desktop(window, screen_index);
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    let _ = (window, screen_index);
}

/// 退出前把壁纸窗从桌面层摘下（Windows: `SetParent(NULL)`+隐藏，防残影）。其它平台 no-op。
pub fn detach_from_desktop(window: &tao::window::Window) {
    #[cfg(target_os = "windows")]
    windows::detach_from_desktop(window);
    #[cfg(not(target_os = "windows"))]
    let _ = window;
}

/// 摘下所有壁纸窗后刷新桌面壁纸，清 WorkerW 残影（Windows）。其它平台 no-op。
pub fn refresh_desktop() {
    #[cfg(target_os = "windows")]
    windows::refresh_desktop();
}

/// 是否有全屏应用占据前台（壁纸此时被完全遮挡，应暂停渲染省电）。
/// Windows：前台窗矩形铺满其所在显示器且非桌面/任务栏 → 真（含无边框全屏游戏/视频）。
/// mac：全屏应用走独立 Space、壁纸所在 Space 被遮，WKWebView 自动节流 rAF，无需显式暂停 → 恒假。
pub fn foreground_fullscreen() -> bool {
    #[cfg(target_os = "windows")]
    {
        windows::foreground_fullscreen()
    }
    #[cfg(not(target_os = "windows"))]
    {
        false
    }
}

/// 设置开机自启（mac=写/删 `~/Library/LaunchAgents` plist，Windows=写/删注册表 HKCU Run 键）。
/// 幂等：`true` 注册当前 exe 路径、`false` 移除。其它平台 no-op。
pub fn set_autostart(enabled: bool) {
    #[cfg(target_os = "macos")]
    macos::set_autostart(enabled);
    #[cfg(target_os = "windows")]
    windows::set_autostart(enabled);
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    let _ = enabled;
}

/// 给透明窗口加毛玻璃背衬（mac=`NSVisualEffectView`，Windows=DWM Acrylic）。其它平台 no-op。
pub fn add_vibrancy(window: &tao::window::Window) {
    #[cfg(target_os = "macos")]
    macos::add_vibrancy(window);
    #[cfg(target_os = "windows")]
    windows::add_vibrancy(window);
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    let _ = window;
}

/// 把窗口带到前台并取得焦点（`.accessory` 策略下从托盘开设置窗需显式激活）。
pub fn focus_window(window: &tao::window::Window) {
    #[cfg(target_os = "macos")]
    macos::focus_window(window);
    #[cfg(not(target_os = "macos"))]
    window.set_focus();
}

/// 设置窗关闭后隐藏 Dock 图标，回到纯菜单栏形态。非 macOS 为 no-op。
pub fn hide_dock() {
    #[cfg(target_os = "macos")]
    macos::hide_dock();
}

/// 设置窗口整体不透明度（0.0=全透明，1.0=不透明），驱动淡入。其它平台 no-op。
pub fn set_alpha(window: &tao::window::Window, alpha: f64) {
    #[cfg(target_os = "macos")]
    macos::set_alpha(window, alpha);
    #[cfg(target_os = "windows")]
    windows::set_alpha(window, alpha);
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    let _ = (window, alpha);
}

/// 安装全局鼠标监视器；返回的句柄需存活以保持监听（mac=NSEvent 监视器，Windows=轮询线程
/// guard）。其它平台 no-op（返回 None）。`Send` 界限为 Windows 轮询线程所需。
pub fn install_mouse_monitor(
    on_move: impl Fn(f64, f64) + Send + 'static,
) -> Option<Box<dyn std::any::Any>> {
    #[cfg(target_os = "macos")]
    {
        macos::install_mouse_monitor(on_move).map(|t| Box::new(t) as Box<dyn std::any::Any>)
    }
    #[cfg(target_os = "windows")]
    {
        windows::install_mouse_monitor(on_move).map(|g| Box::new(g) as Box<dyn std::any::Any>)
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        let _ = on_move;
        None
    }
}

/// 第 `index` 块显示器的稳定硬件标识（mac 取 `CGDisplayID`，可区分同名同型号屏）。
/// 非 macOS 或取不到时返回 None，调用方回落到显示器名/序号。见 CLAUDE.md 阶段三：
/// Windows 侧改用 `EnumDisplayDevices` 的 DeviceID。
pub fn screen_id(index: usize) -> Option<String> {
    #[cfg(target_os = "macos")]
    {
        macos::screen_id(index)
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = index;
        None
    }
}

/// 全局坐标 → 第 `index` 屏归一化坐标（[0,1]，光标不在该屏则分量越界）。非 macOS 返回 None。
pub fn screen_norm(x: f64, y: f64, index: usize) -> Option<(f64, f64)> {
    #[cfg(target_os = "macos")]
    {
        macos::screen_norm(x, y, index)
    }
    #[cfg(target_os = "windows")]
    {
        windows::screen_norm(x, y, index)
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        let _ = (x, y, index);
        None
    }
}
