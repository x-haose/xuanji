//! 壁纸层：把 WebView 窗口沉到桌面图标背后。

#[cfg(target_os = "macos")]
mod macos;

/// 跨平台系统托盘（tray-icon）。见 `tray` 模块。
pub mod tray;

/// 将窗口挂到第 `screen_index` 块显示器的桌面壁纸层（图标背后、跨 Space、点击穿透，
/// 并铺满该屏）。非 macOS 平台暂为 no-op —— Windows WorkerW 见 CLAUDE.md 阶段三。
pub fn attach_to_desktop(window: &tao::window::Window, screen_index: usize) {
    #[cfg(target_os = "macos")]
    macos::attach_to_desktop(window, screen_index);
    #[cfg(not(target_os = "macos"))]
    let _ = (window, screen_index);
}

/// 给透明窗口加 macOS 毛玻璃背衬（`NSVisualEffectView`）。非 macOS 暂为 no-op。
pub fn add_vibrancy(window: &tao::window::Window) {
    #[cfg(target_os = "macos")]
    macos::add_vibrancy(window);
    #[cfg(not(target_os = "macos"))]
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

/// 设置窗口整体不透明度（0.0=全透明，1.0=不透明）。非 macOS 暂为 no-op。
pub fn set_alpha(window: &tao::window::Window, alpha: f64) {
    #[cfg(target_os = "macos")]
    macos::set_alpha(window, alpha);
    #[cfg(not(target_os = "macos"))]
    let _ = (window, alpha);
}

/// 安装全局鼠标监视器；返回的句柄需存活以保持监听。非 macOS 暂为 no-op（返回 None）。
pub fn install_mouse_monitor(
    on_move: impl Fn(f64, f64) + 'static,
) -> Option<Box<dyn std::any::Any>> {
    #[cfg(target_os = "macos")]
    {
        macos::install_mouse_monitor(on_move).map(|t| Box::new(t) as Box<dyn std::any::Any>)
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = on_move;
        None
    }
}

/// 全局坐标 → 第 `index` 屏归一化坐标（[0,1]，光标不在该屏则分量越界）。非 macOS 返回 None。
pub fn screen_norm(x: f64, y: f64, index: usize) -> Option<(f64, f64)> {
    #[cfg(target_os = "macos")]
    {
        macos::screen_norm(x, y, index)
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = (x, y, index);
        None
    }
}
