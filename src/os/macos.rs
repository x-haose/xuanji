//! macOS 壁纸层：把 tao 的 `NSWindow` 沉到桌面级别。
//! 层级取自 `CGWindowLevelForKey`（运行时取值，不硬编码魔数），
//! 设 `desktop+1` 正好夹在真壁纸之上、桌面图标之下。

use std::ptr::NonNull;

use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2::{MainThreadMarker, MainThreadOnly, msg_send};
use objc2_app_kit::{
    NSApplication, NSApplicationActivationPolicy, NSAutoresizingMaskOptions, NSColor, NSEvent,
    NSEventMask, NSScreen, NSVisualEffectBlendingMode, NSVisualEffectMaterial, NSVisualEffectState,
    NSVisualEffectView, NSWindow, NSWindowCollectionBehavior, NSWindowOrderingMode,
};
use objc2_foundation::NSString;
use tao::platform::macos::WindowExtMacOS;
use tao::window::Window;

/// `kCGDesktopWindowLevelKey` —— CoreGraphics 桌面窗口层级的 key。
const CG_DESKTOP_WINDOW_LEVEL_KEY: i32 = 2;

/// 壁纸底色（sRGB 0.18 灰），与 project.json 默认 `bgcolor` 一致，消除启动闪白/闪蓝。
const BG_GRAY: f64 = 0.18;

#[link(name = "CoreGraphics", kind = "framework")]
unsafe extern "C" {
    /// 返回给定 key 对应的窗口层级数值。
    fn CGWindowLevelForKey(key: i32) -> i32;
}

pub fn attach_to_desktop(window: &Window, screen_index: usize) {
    let ptr = window.ns_window() as *const NSWindow;
    // SAFETY: 在 macOS 上 tao 的 `ns_window()` 返回有效、已初始化的 NSWindow 指针，
    // 其所有权与生命周期由 tao 的 Window 持有；此处仅借用它在主线程调用 AppKit
    // 方法，不转移所有权、不越过 window 的生命周期。
    let ns_window: &NSWindow = unsafe { &*ptr };

    // SAFETY: CGWindowLevelForKey 是纯函数，对任意 i32 key 都安全返回一个层级值。
    let desktop = unsafe { CGWindowLevelForKey(CG_DESKTOP_WINDOW_LEVEL_KEY) };
    ns_window.setLevel(desktop as isize + 1);

    ns_window.setCollectionBehavior(
        NSWindowCollectionBehavior::CanJoinAllSpaces
            | NSWindowCollectionBehavior::Stationary
            | NSWindowCollectionBehavior::IgnoresCycle,
    );

    ns_window.setIgnoresMouseEvents(true);
    ns_window.setOpaque(true);
    ns_window.setHasShadow(false);

    // webview 首帧绘制前的窗口底色，避免露出系统默认蓝/白。
    let dark = NSColor::colorWithSRGBRed_green_blue_alpha(BG_GRAY, BG_GRAY, BG_GRAY, 1.0);
    ns_window.setBackgroundColor(Some(&dark));

    // 铺满对应显示器：直接取原生 NSScreen frame，绕开 tao 多屏坐标换算的偏差。
    if let Some(mtm) = MainThreadMarker::new() {
        let screens = NSScreen::screens(mtm);
        if screen_index < screens.count() {
            ns_window.setFrame_display(screens.objectAtIndex(screen_index).frame(), true);
        }
    }
}

/// 给（透明的）窗口加一层 `NSVisualEffectView` 毛玻璃背衬，垫在 webview 之下。
/// 设置窗因此呈现原生磨砂玻璃，而非直接透出杂乱桌面。
pub fn add_vibrancy(window: &Window) {
    let ptr = window.ns_window() as *const NSWindow;
    // SAFETY: 同 attach_to_desktop —— tao 保证有效 NSWindow 指针，仅主线程借用。
    let ns_window: &NSWindow = unsafe { &*ptr };
    let Some(mtm) = MainThreadMarker::new() else {
        return;
    };
    // 关键：窗口须非不透明 + 清空底色，否则 BehindWindow 混合的毛玻璃无法透出后面内容，
    // 会渲染成一块实心暗色（表现为设置窗「完全不透明」）。
    ns_window.setOpaque(false);
    ns_window.setBackgroundColor(Some(&NSColor::clearColor()));
    let Some(content) = ns_window.contentView() else {
        return;
    };
    let bounds = content.bounds();
    let effect = NSVisualEffectView::initWithFrame(NSVisualEffectView::alloc(mtm), bounds);
    effect.setMaterial(NSVisualEffectMaterial::HUDWindow);
    effect.setBlendingMode(NSVisualEffectBlendingMode::BehindWindow);
    effect.setState(NSVisualEffectState::Active);
    effect.setAutoresizingMask(
        NSAutoresizingMaskOptions::ViewWidthSizable | NSAutoresizingMaskOptions::ViewHeightSizable,
    );
    // 插到最底层（webview 之下），毛玻璃只作背衬不挡内容。
    content.addSubview_positioned_relativeTo(&effect, NSWindowOrderingMode::Below, None);
}

/// 把设置窗带到前台并取得焦点。`.accessory` 应用无法抢前台，故临时切到 `.regular`
/// 激活策略（会短暂出现 Dock 图标）+ 激活 App + `makeKeyAndOrderFront`。
/// 关闭设置窗时调 `hide_dock` 切回 `.accessory`。
pub fn focus_window(window: &Window) {
    let Some(mtm) = MainThreadMarker::new() else {
        return;
    };
    let app = NSApplication::sharedApplication(mtm);
    app.setActivationPolicy(NSApplicationActivationPolicy::Regular);
    app.activate();
    let ptr = window.ns_window() as *const NSWindow;
    // SAFETY: 同 attach_to_desktop —— tao 保证有效 NSWindow 指针，仅主线程借用。
    let ns_window: &NSWindow = unsafe { &*ptr };
    ns_window.makeKeyAndOrderFront(None);
}

/// 设置窗关闭后切回 `.accessory`（无 Dock 图标，回到纯壁纸/菜单栏形态）。
pub fn hide_dock() {
    let Some(mtm) = MainThreadMarker::new() else {
        return;
    };
    NSApplication::sharedApplication(mtm)
        .setActivationPolicy(NSApplicationActivationPolicy::Accessory);
}

/// 设置窗口整体不透明度。用于「先以 alpha=0 映射离屏渲染、页面画好再淡入」，
/// 从根上消除启动时露出未绘制图层的闪屏（无论其底色来自窗口还是 WebView）。
pub fn set_alpha(window: &Window, alpha: f64) {
    let ptr = window.ns_window() as *const NSWindow;
    // SAFETY: 同 attach_to_desktop —— tao 保证有效 NSWindow 指针，仅主线程借用调用。
    let ns_window: &NSWindow = unsafe { &*ptr };
    ns_window.setAlphaValue(alpha);
}

/// 安装全局鼠标移动监视器（被动观察，不消费事件，故不破坏点击穿透，也无需辅助功能授权）。
/// 每次移动回调全局屏幕坐标（点，左下原点）。返回的 token 必须存活，否则监听停止。
pub fn install_mouse_monitor<F: Fn(f64, f64) + 'static>(on_move: F) -> Option<Retained<AnyObject>> {
    let block = block2::RcBlock::new(move |_ev: NonNull<NSEvent>| {
        let p = NSEvent::mouseLocation();
        on_move(p.x, p.y);
    });
    NSEvent::addGlobalMonitorForEventsMatchingMask_handler(
        NSEventMask::MouseMoved | NSEventMask::LeftMouseDragged,
        &block,
    )
}

/// 第 `index` 块显示器的 `CGDisplayID`（唯一硬件标识），作每屏配置的稳定键。
/// 优于显示器名——两块同型号外接屏名字相同、id 各异，才能分别配置。
/// 取不到（越界/非主线程）返回 None，调用方回落到名字/序号。
pub fn screen_id(index: usize) -> Option<String> {
    let mtm = MainThreadMarker::new()?;
    let screens = NSScreen::screens(mtm);
    if index >= screens.count() {
        return None;
    }
    let screen = screens.objectAtIndex(index);
    let desc = screen.deviceDescription();
    let num = desc.objectForKey(&NSString::from_str("NSScreenNumber"))?;
    // `NSScreenNumber` 的值是包着 CGDirectDisplayID(u32) 的 NSNumber。
    // SAFETY: 该键的值在 AppKit 契约中恒为 NSNumber，`unsignedIntValue` 返回其 u32。
    let display_id: u32 = unsafe { msg_send![&*num, unsignedIntValue] };
    Some(format!("display-{display_id}"))
}

/// 把全局屏幕坐标换算成第 `index` 块显示器内的归一化坐标（x 左→右、y 下→上，[0,1]）。
/// 光标不在该屏时返回的分量会越界，调用方据此判断。
pub fn screen_norm(x: f64, y: f64, index: usize) -> Option<(f64, f64)> {
    let mtm = MainThreadMarker::new()?;
    let screens = NSScreen::screens(mtm);
    if index >= screens.count() {
        return None;
    }
    let f = screens.objectAtIndex(index).frame();
    Some((
        (x - f.origin.x) / f.size.width,
        (y - f.origin.y) / f.size.height,
    ))
}

/// 开机自启：写/删 `~/Library/LaunchAgents/dev.xuanji.app.plist`（用户级 LaunchAgent，
/// `RunAtLoad` 登录即拉起当前 exe）。幂等：`true` 写入、`false` 删除。
pub fn set_autostart(enabled: bool) {
    let Some(home) = std::env::var_os("HOME") else {
        eprintln!("[mac] 无 HOME 环境变量，开机自启未变更");
        return;
    };
    let plist = std::path::Path::new(&home).join("Library/LaunchAgents/dev.xuanji.app.plist");
    if !enabled {
        let _ = std::fs::remove_file(&plist); // 不存在也无妨
        return;
    }
    let exe = match std::env::current_exe() {
        Ok(p) => p,
        Err(e) => {
            eprintln!("[mac] 取 exe 路径失败，开机自启未变更: {e}");
            return;
        }
    };
    let content = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
         <!DOCTYPE plist PUBLIC \"-//Apple//DTD PLIST 1.0//EN\" \"http://www.apple.com/DTD/PropertyList-1.0.dtd\">\n\
         <plist version=\"1.0\"><dict>\n\
         <key>Label</key><string>dev.xuanji.app</string>\n\
         <key>ProgramArguments</key><array><string>{}</string></array>\n\
         <key>RunAtLoad</key><true/>\n\
         </dict></plist>\n",
        exe.display()
    );
    if let Some(dir) = plist.parent()
        && let Err(e) = std::fs::create_dir_all(dir)
    {
        eprintln!("[mac] 建 LaunchAgents 目录失败: {e}");
        return;
    }
    if let Err(e) = std::fs::write(&plist, content) {
        eprintln!("[mac] 写 LaunchAgent plist 失败: {e}");
    }
}
