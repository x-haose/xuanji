//! 璇玑 M0 骨架：`tao` 起窗 + `wry` 挂载系统 WebView，经 `xuanji://` 协议加载
//! 干支四化 Web 核心，并注入 WE shim 让页面「以为还在 Wallpaper Engine 里」。

mod audio;
mod os;
mod protocol;

use std::error::Error;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use tao::event::{Event, StartCause, WindowEvent};
use tao::event_loop::{ControlFlow, EventLoopBuilder};
use tao::window::{Window, WindowBuilder};
use wry::{PageLoadEvent, WebView, WebViewBuilder};

/// WE API shim，随二进制编译进来，在页面脚本前注入。
const WE_SHIM: &str = include_str!("../web/we-shim.js");

/// 页面迟迟未报加载完成时的兜底淡入时限，避免黑屏。
const REVEAL_FALLBACK: Duration = Duration::from_millis(1500);

/// 收到「加载完成」后再等一小段，确保 WebView 首帧已合成，才淡入。
const REVEAL_AFTER_LOAD: Duration = Duration::from_millis(150);

/// 事件循环自定义事件。
enum UserEvent {
    /// 第 i 块屏的页面加载完成。
    PageLoaded(usize),
    /// 全局鼠标移动到屏幕坐标 (x, y)（点，左下原点）。
    MouseMoved(f64, f64),
    /// 菜单栏选中第 i 个特效（0 = 原生背景）。
    SwitchEffect(isize),
    /// 快捷键循环到下一个特效。
    CycleEffect,
}

/// 菜单项对应的特效 id（索引 0 = 空 = 回原生背景）。
const EFFECT_IDS: [&str; 5] = ["", "starfield", "ink", "thunder", "flowfield"];

/// 把指定特效 id 下发到所有屏（空字符串 = 回原生背景）。
fn apply_effect(screens: &[Screen], id: &str) {
    let js = format!("window.XuanjiFx&&XuanjiFx.select('{id}')");
    for s in screens {
        let _ = s.webview.evaluate_script(&js);
    }
}

/// 把 128 段频谱下发到所有屏（喂 WE 音频回调 + 特效律动）。
fn push_audio(screens: &[Screen], bands: &[f32]) {
    let mut js = String::with_capacity(bands.len() * 6 + 48);
    js.push_str("window.__xuanjiPushAudio&&window.__xuanjiPushAudio([");
    for (i, v) in bands.iter().enumerate() {
        if i > 0 {
            js.push(',');
        }
        js.push_str(&format!("{v:.3}"));
    }
    js.push_str("])");
    for s in screens {
        let _ = s.webview.evaluate_script(&js);
    }
}

/// 音频推送节拍（约 30fps）。
const AUDIO_TICK: Duration = Duration::from_millis(33);

/// 一块显示器对应的壁纸窗口及其淡入状态。
struct Screen {
    window: Window,
    webview: WebView,
    /// 加载完成后待淡入的时刻；`None` 表示尚未收到加载完成。
    reveal_at: Option<Instant>,
    revealed: bool,
}

fn main() -> Result<(), Box<dyn Error>> {
    // ponytail: dev 阶段从源码树读 web/；打包时(阶段四)改 rust-embed 嵌入二进制。
    let web_dir: PathBuf = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("web");

    let event_loop = EventLoopBuilder::<UserEvent>::with_user_event().build();
    // 壁纸不占 Dock：设为 Accessory（无 Dock 图标、无菜单栏）。
    #[cfg(target_os = "macos")]
    {
        use tao::platform::macos::{ActivationPolicy, EventLoopWindowTargetExtMacOS};
        event_loop.set_activation_policy_at_runtime(ActivationPolicy::Accessory);
    }

    // 调试：普通置顶而非下沉桌面，便于连拍验证启动闪色。
    let debug_top = std::env::var_os("XUANJI_DEBUG_TOP").is_some();

    // 测试期：XUANJI_FX=<特效名> 经 URL ?fx= 强制预览某特效（设置系统就绪前的临时开关）。
    let url = match std::env::var("XUANJI_FX") {
        Ok(fx) if !fx.is_empty() => format!("xuanji://localhost/index.html?fx={fx}"),
        _ => "xuanji://localhost/index.html".to_string(),
    };

    // 每块显示器各建一窗；无枚举结果时退回单个默认窗口。
    let monitors: Vec<Option<_>> = {
        let all: Vec<_> = event_loop.available_monitors().map(Some).collect();
        if all.is_empty() { vec![None] } else { all }
    };

    let proxy = event_loop.create_proxy();
    let mut screens: Vec<Screen> = Vec::with_capacity(monitors.len());
    for (i, monitor) in monitors.into_iter().enumerate() {
        let mut builder = WindowBuilder::new()
            .with_title("璇玑 Xuanji")
            .with_decorations(false)
            .with_always_on_top(debug_top)
            .with_visible(false);
        if let Some(m) = &monitor {
            builder = builder
                .with_position(m.position())
                .with_inner_size(m.size());
        }
        let window = builder.build(&event_loop)?;

        let web_dir = web_dir.clone();
        let proxy = proxy.clone();
        let webview = WebViewBuilder::new()
            .with_background_color((46, 46, 46, 255))
            .with_ipc_handler(|req| eprintln!("[web] {}", req.body()))
            .with_custom_protocol("xuanji".into(), move |_id, request| {
                protocol::serve(&web_dir, &request)
            })
            .with_initialization_script(WE_SHIM)
            .with_on_page_load_handler(move |event, _url| {
                if matches!(event, PageLoadEvent::Finished) {
                    let _ = proxy.send_event(UserEvent::PageLoaded(i));
                }
            })
            .with_url(&url)
            .build(&window)?;

        // 沉到第 i 块显示器的桌面壁纸层并铺满（调试置顶模式跳过）。
        if !debug_top {
            os::attach_to_desktop(&window, i);
        }
        // 先透明映射离屏渲染，页面画好再淡入，杜绝未绘制图层的白/蓝闪。
        os::set_alpha(&window, 0.0);
        window.set_visible(true);

        screens.push(Screen {
            window,
            webview,
            reveal_at: None,
            revealed: false,
        });
    }

    // 全局鼠标监视器：每次移动经 proxy 把屏幕坐标送回事件循环。token 需存活。
    let mouse_proxy = proxy.clone();
    let _mouse_monitor = os::install_mouse_monitor(move |x, y| {
        let _ = mouse_proxy.send_event(UserEvent::MouseMoved(x, y));
    });

    // 菜单栏特效切换：须在 NSApp 启动完成后建，故推迟到事件循环首次 Init。句柄存活于闭包。
    let menu_proxy = proxy.clone();
    let mut menu: Option<Box<dyn std::any::Any>> = None;

    // 全局快捷键 ⌃⌥→ 循环下一个特效（需「输入监控」授权）。句柄需存活。
    let key_proxy = proxy.clone();
    let _key_monitor = os::install_key_monitor(move || {
        let _ = key_proxy.send_event(UserEvent::CycleEffect);
    });
    let mut current_effect: usize = 0;

    // 系统声音律动：loopback 捕获 + FFT，按节拍下发频谱。失败则静默降级。
    let audio = audio::start();
    let mut next_audio = Instant::now() + AUDIO_TICK;

    let fallback = Instant::now() + REVEAL_FALLBACK;
    event_loop.run(move |event, _, control_flow| {
        match event {
            Event::NewEvents(StartCause::Init) => {
                let mp = menu_proxy.clone();
                menu = os::install_effect_menu(
                    &[
                        "原生背景",
                        "星空 starfield",
                        "水墨 ink",
                        "雷法 thunder",
                        "流场 flowfield",
                    ],
                    move |i| {
                        let _ = mp.send_event(UserEvent::SwitchEffect(i));
                    },
                );
            }
            Event::UserEvent(UserEvent::PageLoaded(i)) => {
                if let Some(s) = screens.get_mut(i)
                    && !s.revealed
                    && s.reveal_at.is_none()
                {
                    s.reveal_at = Some(Instant::now() + REVEAL_AFTER_LOAD);
                }
            }
            Event::UserEvent(UserEvent::SwitchEffect(i)) => {
                if i >= 0 && (i as usize) < EFFECT_IDS.len() {
                    current_effect = i as usize;
                    apply_effect(&screens, EFFECT_IDS[current_effect]);
                }
            }
            Event::UserEvent(UserEvent::CycleEffect) => {
                current_effect = (current_effect + 1) % EFFECT_IDS.len();
                apply_effect(&screens, EFFECT_IDS[current_effect]);
            }
            Event::UserEvent(UserEvent::MouseMoved(x, y)) => {
                // 逐屏换算归一化坐标；仅把光标喂给它所在的屏，其余屏无更新 → 自然淡出。
                for (i, s) in screens.iter().enumerate() {
                    if let Some((nx, ny)) = os::screen_norm(x, y, i)
                        && (0.0..=1.0).contains(&nx)
                        && (0.0..=1.0).contains(&ny)
                    {
                        let js = format!("window.XuanjiFx&&XuanjiFx.pointer({nx:.4},{ny:.4})");
                        let _ = s.webview.evaluate_script(&js);
                    }
                }
            }
            Event::WindowEvent {
                event: WindowEvent::CloseRequested,
                ..
            } => {
                *control_flow = ControlFlow::Exit;
                return;
            }
            _ => {}
        }
        let _ = &menu; // 仅保活菜单栏句柄

        // 到点（或兜底超时）淡入对应窗口。
        let now = Instant::now();
        for s in screens.iter_mut() {
            if !s.revealed && now >= s.reveal_at.unwrap_or(fallback) {
                os::set_alpha(&s.window, 1.0);
                s.revealed = true;
            }
        }

        // 下一次唤醒 = 最早的待淡入时刻；全部淡入后转纯等待。
        *control_flow = match screens
            .iter()
            .filter(|s| !s.revealed)
            .map(|s| s.reveal_at.unwrap_or(fallback))
            .min()
        {
            Some(next) => ControlFlow::WaitUntil(next),
            None => ControlFlow::Wait,
        };

        // 音频律动：按节拍取频谱下发，并让事件循环保持该节拍唤醒。
        if let Some(a) = &audio {
            let now = Instant::now();
            if now >= next_audio {
                push_audio(&screens, &a.bands());
                next_audio = now + AUDIO_TICK;
            }
            *control_flow = ControlFlow::WaitUntil(next_audio);
        }
    });
}
