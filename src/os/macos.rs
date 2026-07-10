//! macOS 壁纸层：把 tao 的 `NSWindow` 沉到桌面级别。
//! 层级取自 `CGWindowLevelForKey`（运行时取值，不硬编码魔数），
//! 设 `desktop+1` 正好夹在真壁纸之上、桌面图标之下。

use std::ptr::NonNull;

use objc2::rc::Retained;
use objc2::runtime::{AnyObject, NSObject};
use objc2::{AnyThread, DefinedClass, MainThreadMarker, define_class, msg_send, sel};
use objc2_app_kit::{
    NSColor, NSEvent, NSEventMask, NSEventModifierFlags, NSMenu, NSMenuItem, NSScreen, NSStatusBar,
    NSWindow, NSWindowCollectionBehavior,
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

struct MenuIvars {
    on_select: Box<dyn Fn(isize)>,
}

define_class!(
    #[unsafe(super(NSObject))]
    #[name = "XuanjiMenuTarget"]
    #[ivars = MenuIvars]
    struct MenuTarget;

    impl MenuTarget {
        /// 菜单项点击回调：按 tag(=特效索引)转发。
        #[unsafe(method(onSelect:))]
        fn on_select(&self, sender: &NSMenuItem) {
            (self.ivars().on_select)(sender.tag());
        }
    }
);

impl MenuTarget {
    fn new(cb: Box<dyn Fn(isize)>) -> Retained<Self> {
        let this = Self::alloc().set_ivars(MenuIvars { on_select: cb });
        unsafe { msg_send![super(this), init] }
    }
}

/// 在菜单栏放一个「璇玑」图标，菜单列出各特效；点击 → `on_select(索引)`。
/// 返回的句柄(含状态项/菜单/target)须存活，否则菜单消失或点击无响应。
pub fn install_effect_menu(
    items: &[&str],
    on_select: impl Fn(isize) + 'static,
) -> Option<Box<dyn std::any::Any>> {
    let mtm = MainThreadMarker::new()?;
    let target = MenuTarget::new(Box::new(on_select));
    let status = NSStatusBar::systemStatusBar().statusItemWithLength(-1.0); // 可变宽度
    if let Some(button) = status.button(mtm) {
        button.setTitle(&NSString::from_str("☯ 璇玑"));
    }
    let menu = NSMenu::new(mtm);
    for (i, name) in items.iter().enumerate() {
        let mi = NSMenuItem::new(mtm);
        mi.setTitle(&NSString::from_str(name));
        mi.setTag(i as isize);
        // SAFETY: target 存活于返回句柄中；action 选择子由 MenuTarget 实现。
        unsafe {
            mi.setTarget(Some(&target));
            mi.setAction(Some(sel!(onSelect:)));
        }
        menu.addItem(&mi);
    }
    status.setMenu(Some(&menu));
    Some(Box::new((status, menu, target)))
}

/// 安装全局快捷键监视器：按下 `⌃⌥→`(Control+Option+右箭头) 时回调。
/// 需系统「输入监控」授权（键盘全局监听）。返回 token 须存活。
pub fn install_key_monitor<F: Fn() + 'static>(on_hotkey: F) -> Option<Retained<AnyObject>> {
    let block = block2::RcBlock::new(move |ev: NonNull<NSEvent>| {
        // SAFETY: 全局监视器在回调期间提供有效 NSEvent。
        let ev = unsafe { ev.as_ref() };
        let flags = ev.modifierFlags();
        let need = NSEventModifierFlags::Control | NSEventModifierFlags::Option;
        // 右箭头 keyCode=124；要求恰按 ⌃⌥ 且未按 ⌘。
        if ev.keyCode() == 124
            && flags.contains(need)
            && !flags.contains(NSEventModifierFlags::Command)
        {
            on_hotkey();
        }
    });
    NSEvent::addGlobalMonitorForEventsMatchingMask_handler(NSEventMask::KeyDown, &block)
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
