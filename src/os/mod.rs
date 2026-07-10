//! 壁纸层：把 WebView 窗口沉到桌面图标背后。

#[cfg(target_os = "macos")]
mod macos;

/// 将窗口挂到第 `screen_index` 块显示器的桌面壁纸层（图标背后、跨 Space、点击穿透，
/// 并铺满该屏）。非 macOS 平台暂为 no-op —— Windows WorkerW 见 CLAUDE.md 阶段三。
pub fn attach_to_desktop(window: &tao::window::Window, screen_index: usize) {
    #[cfg(target_os = "macos")]
    macos::attach_to_desktop(window, screen_index);
    #[cfg(not(target_os = "macos"))]
    let _ = (window, screen_index);
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

/// 在菜单栏放特效切换菜单；点击某项 → `on_select(索引)`。返回句柄需存活。非 macOS no-op。
pub fn install_effect_menu(
    items: &[&str],
    on_select: impl Fn(isize) + 'static,
) -> Option<Box<dyn std::any::Any>> {
    #[cfg(target_os = "macos")]
    {
        macos::install_effect_menu(items, on_select)
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = (items, on_select);
        None
    }
}

/// 安装全局快捷键（`⌃⌥→`）监视器；触发时回调。返回句柄需存活。非 macOS no-op。
pub fn install_key_monitor(on_hotkey: impl Fn() + 'static) -> Option<Box<dyn std::any::Any>> {
    #[cfg(target_os = "macos")]
    {
        macos::install_key_monitor(on_hotkey).map(|t| Box::new(t) as Box<dyn std::any::Any>)
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = on_hotkey;
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
