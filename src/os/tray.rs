//! 跨平台系统托盘（tray-icon + muda）。菜单：背景子菜单（勾选当前）/ 设置… / 退出。
//! 须在事件循环就绪后创建；返回句柄须存活，否则托盘消失。

use std::collections::HashMap;

use tray_icon::menu::{
    CheckMenuItem, Menu, MenuEvent, MenuId, MenuItem, PredefinedMenuItem, Submenu,
};
use tray_icon::{TrayIcon, TrayIconBuilder};

/// 托盘菜单指令。
#[derive(Clone)]
pub enum TrayCmd {
    /// 设置背景类型为该 bgtype value。
    SetBg(String),
    /// 打开设置窗。
    OpenSettings,
    /// 退出应用。
    Quit,
}

/// 托盘存活句柄；drop 即移除托盘。持有勾选项以便随当前背景更新。
pub struct TrayHandle {
    _tray: TrayIcon,
    /// bgtype value → 勾选菜单项，用于高亮当前背景。
    checks: HashMap<String, CheckMenuItem>,
}

impl TrayHandle {
    /// 把当前背景对应的项打勾，其余取消——托盘随时反映实际选中项。
    pub fn set_active(&self, value: &str) {
        for (v, item) in &self.checks {
            item.set_checked(v == value);
        }
    }
}

/// 建托盘。`items` 是 (bgtype value, 显示名) 列表（与设置下拉同源）；
/// `current` 为当前 bgtype，用于初始勾选。菜单事件经 `on_cmd` 转发。
pub fn install(
    items: &[(String, String)],
    current: &str,
    on_cmd: impl Fn(TrayCmd) + Send + Sync + 'static,
) -> Option<TrayHandle> {
    let menu = Menu::new();
    let mut map: HashMap<MenuId, TrayCmd> = HashMap::new();
    let mut checks: HashMap<String, CheckMenuItem> = HashMap::new();

    let bg = Submenu::new("背景", true);
    for (value, label) in items {
        let item = CheckMenuItem::new(label, true, value == current, None);
        map.insert(item.id().clone(), TrayCmd::SetBg(value.clone()));
        checks.insert(value.clone(), item.clone());
        let _ = bg.append(&item);
    }
    let _ = menu.append(&bg);
    let _ = menu.append(&PredefinedMenuItem::separator());

    let settings = MenuItem::new("设置…", true, None);
    map.insert(settings.id().clone(), TrayCmd::OpenSettings);
    let _ = menu.append(&settings);

    let quit = MenuItem::new("退出", true, None);
    map.insert(quit.id().clone(), TrayCmd::Quit);
    let _ = menu.append(&quit);

    MenuEvent::set_event_handler(Some(move |ev: MenuEvent| {
        if let Some(cmd) = map.get(ev.id()) {
            on_cmd(cmd.clone());
        }
    }));

    let mut builder = TrayIconBuilder::new()
        .with_menu(Box::new(menu))
        .with_tooltip("璇玑 Xuanji");
    // macOS 菜单栏用文字符号（随明暗自适应、清爽）；Windows 需位图图标。
    #[cfg(target_os = "macos")]
    {
        builder = builder.with_title("☯ 璇玑");
    }
    #[cfg(not(target_os = "macos"))]
    {
        builder = builder.with_icon(taiji_icon());
    }
    let tray = builder.build().ok()?;

    Some(TrayHandle {
        _tray: tray,
        checks,
    })
}

/// 32×32 太极图标（Windows 托盘用；macOS 走文字符号 ☯）。
/// ponytail: 程序生成的极简太极；出品前可换设计师版 png（阶段四打包时）。
#[cfg(not(target_os = "macos"))]
fn taiji_icon() -> tray_icon::Icon {
    const S: i32 = 32;
    let c = S as f32 / 2.0;
    let r = c - 1.0;
    let half = r / 2.0;
    let eye = r / 6.0;
    let mut rgba = vec![0u8; (S * S * 4) as usize];
    for y in 0..S {
        for x in 0..S {
            let dx = x as f32 + 0.5 - c;
            let dy = y as f32 + 0.5 - c;
            if dx * dx + dy * dy > r * r {
                continue; // 圆外透明
            }
            let up = dx * dx + (dy + half).powi(2) < half * half;
            let lo = dx * dx + (dy - half).powi(2) < half * half;
            // 上下两个半瓣拼出 S 形分界，再点鱼眼。
            let mut dark = if up {
                false
            } else if lo {
                true
            } else {
                dx >= 0.0
            };
            if dx * dx + (dy + half).powi(2) < eye * eye {
                dark = true;
            } else if dx * dx + (dy - half).powi(2) < eye * eye {
                dark = false;
            }
            let v = if dark { 30 } else { 235 };
            let px = ((y * S + x) * 4) as usize;
            rgba[px] = v;
            rgba[px + 1] = v;
            rgba[px + 2] = v;
            rgba[px + 3] = 255;
        }
    }
    tray_icon::Icon::from_rgba(rgba, S as u32, S as u32).expect("taiji RGBA 合法")
}
