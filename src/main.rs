//! 璇玑 M0 骨架：`tao` 起窗 + `wry` 挂载系统 WebView，经 `xuanji://` 协议加载
//! 干支四化 Web 核心，并注入 WE shim 让页面「以为还在 Wallpaper Engine 里」。

mod audio;
mod ipc;
mod os;
mod protocol;
mod settings;

use std::error::Error;
use std::time::{Duration, Instant};

use tao::dpi::LogicalSize;
use tao::event::{Event, StartCause, WindowEvent};
use tao::event_loop::{ControlFlow, EventLoopBuilder, EventLoopProxy, EventLoopWindowTarget};
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
    /// 托盘菜单指令。
    Tray(os::tray::TrayCmd),
    /// 设置窗控制消息。
    Ipc(ipc::Msg),
}

/// 把 128 段频谱下发到所有屏（喂 WE 音频回调 + 特效律动）。
fn push_audio(screens: &[Screen], bands: &[f32], sample_rate: f32) {
    let mut js = String::with_capacity(bands.len() * 6 + 72);
    // 先带上采样率（供 JS 按 Hz 分频反解 band），再推 128 段频谱。
    js.push_str(&format!("window.__xuanjiSR={sample_rate:.0};"));
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

/// 把一批属性下发单块壁纸窗（走 WE 契约 applyUserProperties）。
/// WE 契约：页面读 `props[key].value`，故每个标量值包成 `{value: ...}`。
fn apply_to(screen: &Screen, props: &serde_json::Map<String, serde_json::Value>) {
    let wrapped: serde_json::Map<String, serde_json::Value> = props
        .iter()
        .map(|(k, v)| (k.clone(), serde_json::json!({ "value": v })))
        .collect();
    let json = serde_json::Value::Object(wrapped);
    let js =
        format!("window.__xuanjiApplyUserProperties&&window.__xuanjiApplyUserProperties({json})");
    let _ = screen.webview.evaluate_script(&js);
}

/// 某屏解析配置时的有效作用域：**per-screen 覆盖仅在多屏时生效**。
/// 单屏无「每屏差异」可言，且单屏 UI 无「作用范围」入口——若仍生效，历史遗留的
/// 某屏覆盖会静默劫持画面且无从解除（多屏设过、拔屏变单屏即卡死）。故单屏一律走全局。
fn screen_scope<'a>(screens: &[Screen], s: &'a Screen) -> Option<&'a str> {
    (screens.len() > 1).then_some(s.id.as_str())
}

/// 给每块屏下发「它自己的」最终配置（多屏按 id 解析 per-screen ⊕ 全局 ⊕ 默认；单屏走全局）。
/// 用于全局性变更（预设/重置/bgtype）——多屏时已有专属覆盖的屏保留自身，不被全局值冲掉。
fn apply_all(screens: &[Screen], settings: &settings::Settings) {
    for s in screens {
        apply_to(s, &settings.resolved_for(screen_scope(screens, s)));
    }
}

/// 改一个参数：校验更新 → 广播壁纸窗 → 落盘 →（设置窗开着则回填该字段）。
/// `target = None` 改全局（下发所有屏，但各屏若有专属覆盖则保留自身）；
/// `Some(id)` 只改该屏、只下发该屏。托盘切背景与设置窗改参共用此路径，保证单一数据源一致。
fn set_and_broadcast(
    settings: &mut settings::Settings,
    screens: &[Screen],
    setwv: Option<&WebView>,
    key: &str,
    value: serde_json::Value,
    target: Option<&str>,
) {
    match settings.set(key, value.clone(), target) {
        Ok(()) => {
            // autostart 是壳侧 OS 副作用（Win 注册表 / mac LaunchAgent），非壁纸属性，单独落地。
            if key == "autostart" {
                os::set_autostart(value.as_bool().unwrap_or(false));
            }
            // 切背景类型须整体重应用（带上粒子等参数触发页面 needReinit 重建）；普通改参只下发单键。
            // 目标屏过滤：target=Some 时只碰该屏；None 时下发每屏「它自己的」解析值。
            let full = key == "bgtype";
            for s in screens {
                if target.is_some_and(|t| t != s.id) {
                    continue;
                }
                let resolved = settings.resolved_for(screen_scope(screens, s));
                if full {
                    apply_to(s, &resolved);
                } else {
                    let mut one = serde_json::Map::new();
                    if let Some(v) = resolved.get(key) {
                        one.insert(key.to_string(), v.clone());
                    }
                    apply_to(s, &one);
                }
            }
            if let Err(e) = settings.save() {
                eprintln!("[shell] 落盘失败: {e}");
            }
            if let Some(w) = setwv {
                let _ = w.evaluate_script(&format!(
                    "window.__xuanjiSetField&&window.__xuanjiSetField({},{},{})",
                    serde_json::Value::String(key.to_string()),
                    value,
                    match target {
                        Some(id) => serde_json::Value::String(id.to_string()),
                        None => serde_json::Value::Null,
                    }
                ));
            }
        }
        Err(e) => eprintln!("[shell] set 拒绝: {e}"),
    }
}

/// 屏描述列表（去重 id）：喂设置窗的屏幕选择器。
/// label 用序号「屏幕 N」——id 是 CGDisplayID 数字不宜示人，且同型号屏名字也相同，序号才是唯一可辨的。
fn screen_descriptors(screens: &[Screen]) -> Vec<serde_json::Value> {
    let mut seen = std::collections::BTreeSet::new();
    let mut out = Vec::new();
    for (i, s) in screens.iter().enumerate() {
        if !seen.insert(s.id.clone()) {
            continue;
        }
        out.push(serde_json::json!({ "id": s.id, "label": format!("屏幕 {}", i + 1) }));
    }
    out
}

/// 把当前配置 + 预设 + 屏列表 + 每屏解析值注入设置窗（其加载完/切预设后调用）。
fn init_settings_window(wv: &WebView, settings: &settings::Settings, screens: &[Screen]) {
    let descriptors = screen_descriptors(screens);
    let screen_values: serde_json::Map<String, serde_json::Value> = descriptors
        .iter()
        .filter_map(|d| d.get("id").and_then(|v| v.as_str()))
        .map(|id| {
            (
                id.to_string(),
                serde_json::Value::Object(settings.resolved_for(Some(id))),
            )
        })
        .collect();
    let payload = serde_json::json!({
        "values": settings.resolved(),
        "presets": settings.presets(),
        "screens": descriptors,
        "screenValues": screen_values,
    });
    let _ = wv.evaluate_script(&format!(
        "window.__xuanjiInitSettings&&window.__xuanjiInitSettings({payload})"
    ));
}

/// 建一个普通（可见/有边框/正常层级/响应鼠标）设置窗，加载 Svelte 产物。
/// 其 ipc_handler 解析控制消息经 proxy 送回事件循环。
/// 程序化生成 64×64 北斗七星窗口图标（璇玑玉衡=北斗，最贴主题；免图标资源与图片解码依赖）。
/// 深蓝底 + 七枚青色高斯亮点排成斗形。取不到（尺寸非法）返回 None，退回系统默认。
fn app_icon() -> Option<tao::window::Icon> {
    const S: usize = 64;
    let mut rgba = vec![0u8; S * S * 4];
    for px in rgba.chunks_exact_mut(4) {
        px.copy_from_slice(&[24, 26, 40, 255]);
    }
    // 北斗七星归一化坐标（斗形），亮点染青 accent。
    let stars = [
        (0.20, 0.28),
        (0.20, 0.50),
        (0.37, 0.56),
        (0.44, 0.42),
        (0.60, 0.44),
        (0.76, 0.40),
        (0.90, 0.30),
    ];
    for &(sx, sy) in &stars {
        let (cx, cy) = (sx * S as f64, sy * S as f64);
        for y in 0..S {
            for x in 0..S {
                let (dx, dy) = (x as f64 - cx, y as f64 - cy);
                let g = (-(dx * dx + dy * dy) / 6.0).exp(); // 高斯亮点
                if g <= 0.01 {
                    continue;
                }
                let i = (y * S + x) * 4;
                let mix = |c: u8, t: u8| (c as f64 + (t as f64 - c as f64) * g).min(255.0) as u8;
                rgba[i] = mix(rgba[i], 120);
                rgba[i + 1] = mix(rgba[i + 1], 230);
                rgba[i + 2] = mix(rgba[i + 2], 220);
            }
        }
    }
    tao::window::Icon::from_rgba(rgba, S as u32, S as u32).ok()
}

fn build_settings_window(
    target: &EventLoopWindowTarget<UserEvent>,
    proxy: EventLoopProxy<UserEvent>,
) -> Result<(Window, WebView), Box<dyn Error>> {
    let size = LogicalSize::new(980.0, 720.0);
    let mut builder = WindowBuilder::new()
        .with_title("璇玑 · 设置")
        .with_inner_size(size)
        .with_window_icon(app_icon())
        .with_transparent(true);
    // 居中到主屏（否则多屏时可能落在别的屏上「点了没看见」）。
    if let Some(mon) = target.primary_monitor() {
        let ms = mon.size();
        let mp = mon.position();
        let scale = mon.scale_factor();
        let w = 980.0 * scale;
        let h = 720.0 * scale;
        let x = mp.x as f64 + (ms.width as f64 - w) / 2.0;
        let y = mp.y as f64 + (ms.height as f64 - h) / 2.0;
        builder = builder.with_position(tao::dpi::PhysicalPosition::new(x, y));
    }
    let window = builder.build(target)?;
    os::add_vibrancy(&window);
    let webview = WebViewBuilder::new()
        .with_transparent(true)
        .with_custom_protocol("xuanji".into(), move |_id, req| protocol::serve(&req))
        .with_ipc_handler(move |req| {
            if let Some(msg) = ipc::parse(req.body()) {
                let _ = proxy.send_event(UserEvent::Ipc(msg));
            }
        })
        .with_url("xuanji://localhost/settings/dist/index.html")
        .build(&window)?;
    Ok((window, webview))
}

/// 把托盘背景子菜单的勾选同步到当前 bgtype。
fn sync_tray_bg(tray: &Option<os::tray::TrayHandle>, settings: &settings::Settings) {
    if let Some(t) = tray
        && let Some(bg) = settings.resolved().get("bgtype").and_then(|v| v.as_str())
    {
        t.set_active(bg);
    }
}

/// 处理设置窗来的一条 IPC 消息：更新配置 → 广播壁纸窗 → 落盘 → 回写设置窗/托盘。
fn handle_ipc(
    msg: ipc::Msg,
    settings: &mut settings::Settings,
    screens: &[Screen],
    settings_win: &Option<(Window, WebView)>,
    tray: &Option<os::tray::TrayHandle>,
) {
    use ipc::Msg;
    let setwv = settings_win.as_ref().map(|(_, w)| w);
    let persist = |s: &settings::Settings| {
        if let Err(e) = s.save() {
            eprintln!("[shell] 落盘失败: {e}");
        }
    };
    match msg {
        Msg::Ready => {
            if let Some(w) = setwv {
                init_settings_window(w, settings, screens);
            }
        }
        Msg::Set { key, value, screen } => {
            set_and_broadcast(settings, screens, setwv, &key, value, screen.as_deref());
            // 托盘只反映全局 bgtype；某屏专属改动不动托盘勾选。
            if key == "bgtype" && screen.is_none() {
                sync_tray_bg(tray, settings);
            }
        }
        Msg::Pick { key, kind, screen } => {
            let picked = match kind {
                ipc::PickKind::File => rfd::FileDialog::new().pick_file(),
                ipc::PickKind::Directory => rfd::FileDialog::new().pick_folder(),
            };
            if let Some(path) = picked {
                let val = serde_json::Value::String(path.to_string_lossy().into_owned());
                set_and_broadcast(settings, screens, setwv, &key, val, screen.as_deref());
            }
        }
        Msg::SavePreset { name } => {
            settings.save_preset(&name);
            persist(settings);
            if let Some(w) = setwv {
                init_settings_window(w, settings, screens);
            }
        }
        Msg::DeletePreset { name } => {
            settings.delete_preset(&name);
            persist(settings);
            if let Some(w) = setwv {
                init_settings_window(w, settings, screens);
            }
        }
        Msg::ApplyPreset { name } => {
            if settings.apply_preset(&name) {
                apply_all(screens, settings);
                persist(settings);
                sync_tray_bg(tray, settings);
                if let Some(w) = setwv {
                    init_settings_window(w, settings, screens);
                }
            }
        }
        Msg::Reset => {
            settings.reset();
            apply_all(screens, settings);
            persist(settings);
            sync_tray_bg(tray, settings);
            if let Some(w) = setwv {
                init_settings_window(w, settings, screens);
            }
        }
    }
}

/// 音频推送节拍（约 30fps）。
const AUDIO_TICK: Duration = Duration::from_millis(16);

/// 显示器热插拔轮询间隔——每隔一会儿比对显示器排布，变化则重建壁纸窗。
const SCREEN_CHECK: Duration = Duration::from_secs(2);

/// 一块显示器对应的壁纸窗口及其淡入状态。
struct Screen {
    /// 稳定屏标识：CGDisplayID（`display-N`）优先，回落显示器名、再回落 `screen-{i}`。
    /// 作每屏配置的键——CGDisplayID 唯一，两块同型号外接屏也能分别配置。
    id: String,
    window: Window,
    webview: WebView,
    /// 加载完成后待淡入的时刻；`None` 表示尚未收到加载完成。
    reveal_at: Option<Instant>,
    /// 迟迟收不到加载完成时的兜底淡入时刻（各窗独立，热插拔新窗也从容淡入）。
    fallback_at: Instant,
    revealed: bool,
}

/// 为第 `i` 块显示器建一个壁纸窗（沉桌面层、铺满、先透明待页面画好再淡入）。
fn build_screen(
    target: &EventLoopWindowTarget<UserEvent>,
    url: &str,
    proxy: &EventLoopProxy<UserEvent>,
    i: usize,
    monitor: Option<&tao::monitor::MonitorHandle>,
    debug_top: bool,
) -> Result<Screen, Box<dyn Error>> {
    let mut builder = WindowBuilder::new()
        .with_title("璇玑 Xuanji")
        .with_decorations(false)
        .with_always_on_top(debug_top)
        .with_visible(false);
    if let Some(m) = monitor {
        builder = builder
            .with_position(m.position())
            .with_inner_size(m.size());
    }
    // 稳定屏键优先取 CGDisplayID（同名同型号屏也各异）；取不到回落显示器名、再回落序号。
    let id = os::screen_id(i)
        .or_else(|| monitor.and_then(|m| m.name()).filter(|n| !n.is_empty()))
        .unwrap_or_else(|| format!("screen-{i}"));
    let window = builder.build(target)?;

    let load_proxy = proxy.clone();
    let webview = WebViewBuilder::new()
        .with_background_color((46, 46, 46, 255))
        .with_ipc_handler(|req| eprintln!("[web] {}", req.body()))
        .with_custom_protocol("xuanji".into(), move |_id, request| {
            protocol::serve(&request)
        })
        .with_initialization_script(WE_SHIM)
        .with_on_page_load_handler(move |event, _url| {
            if matches!(event, PageLoadEvent::Finished) {
                let _ = load_proxy.send_event(UserEvent::PageLoaded(i));
            }
        })
        .with_url(url)
        .build(&window)?;

    if !debug_top {
        os::attach_to_desktop(&window, i);
    }
    os::set_alpha(&window, 0.0);
    window.set_visible(true);

    Ok(Screen {
        id,
        window,
        webview,
        reveal_at: None,
        fallback_at: Instant::now() + REVEAL_FALLBACK,
        revealed: false,
    })
}

/// 当前显示器排布签名（位置+尺寸，顺序无关）——用于检测热插拔/重排。
fn screens_signature(target: &EventLoopWindowTarget<UserEvent>) -> String {
    let mut parts: Vec<String> = target
        .available_monitors()
        .map(|m| {
            let p = m.position();
            let s = m.size();
            format!("{},{}:{}x{}", p.x, p.y, s.width, s.height)
        })
        .collect();
    parts.sort();
    parts.join("|")
}

/// 按当前显示器重建全部壁纸窗（热插拔/重排后调用）。旧窗随 `clear` 关闭。
fn rebuild_screens(
    target: &EventLoopWindowTarget<UserEvent>,
    url: &str,
    proxy: &EventLoopProxy<UserEvent>,
    debug_top: bool,
    screens: &mut Vec<Screen>,
) {
    screens.clear();
    let monitors: Vec<Option<tao::monitor::MonitorHandle>> = {
        let all: Vec<_> = target.available_monitors().map(Some).collect();
        if all.is_empty() { vec![None] } else { all }
    };
    for (i, monitor) in monitors.iter().enumerate() {
        match build_screen(target, url, proxy, i, monitor.as_ref(), debug_top) {
            Ok(s) => screens.push(s),
            Err(e) => eprintln!("[shell] 显示器 {i} 重建失败: {e}"),
        }
    }
}

fn main() -> Result<(), Box<dyn Error>> {
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
    for (i, monitor) in monitors.iter().enumerate() {
        match build_screen(&event_loop, &url, &proxy, i, monitor.as_ref(), debug_top) {
            Ok(s) => screens.push(s),
            Err(e) => eprintln!("[shell] 显示器 {i} 建窗失败: {e}"),
        }
    }

    // 全局鼠标监视器：每次移动经 proxy 把屏幕坐标送回事件循环。token 需存活。
    let mouse_proxy = proxy.clone();
    let _mouse_monitor = os::install_mouse_monitor(move |x, y| {
        let _ = mouse_proxy.send_event(UserEvent::MouseMoved(x, y));
    });

    // 系统托盘：须在事件循环就绪后建（macOS 依赖 NSApp），故推迟到首次 Init。句柄需存活。
    let tray_proxy = proxy.clone();
    let mut tray: Option<os::tray::TrayHandle> = None;

    // 配置单一数据源 + 设置窗（懒建，单例）。
    let mut settings = settings::Settings::load(settings::Settings::config_path());
    // 启动即把开机自启注册表/plist 对齐到持久化配置（幂等，防手动删注册表后与配置不符）。
    os::set_autostart(
        settings
            .resolved()
            .get("autostart")
            .and_then(|v| v.as_bool())
            .unwrap_or(false),
    );
    let mut settings_win: Option<(Window, WebView)> = None;

    // 系统声音律动：loopback 捕获 + FFT，按节拍下发频谱。失败则静默降级。
    let audio = audio::start();
    let mut next_audio = Instant::now() + AUDIO_TICK;

    // 显示器热插拔：记录初始排布签名，事件循环里定期比对，变化则重建壁纸窗。
    let mut last_screens_sig = screens_signature(&event_loop);
    let mut next_screen_check = Instant::now() + SCREEN_CHECK;

    event_loop.run(move |event, target, control_flow| {
        match event {
            Event::NewEvents(StartCause::Init) => {
                let tp = tray_proxy.clone();
                let opts = settings::Settings::bgtype_options();
                let current = settings
                    .resolved()
                    .get("bgtype")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                tray = os::tray::install(&opts, &current, move |cmd| {
                    let _ = tp.send_event(UserEvent::Tray(cmd));
                });
            }
            Event::UserEvent(UserEvent::PageLoaded(i)) => {
                // 页面就绪即下发该屏配置（壳侧为唯一配置来源，多屏按 id 解析、单屏走全局）。
                if let Some(s) = screens.get(i) {
                    apply_to(s, &settings.resolved_for(screen_scope(&screens, s)));
                }
                if let Some(s) = screens.get_mut(i)
                    && !s.revealed
                    && s.reveal_at.is_none()
                {
                    s.reveal_at = Some(Instant::now() + REVEAL_AFTER_LOAD);
                }
            }
            Event::UserEvent(UserEvent::Tray(cmd)) => {
                use os::tray::TrayCmd;
                match cmd {
                    TrayCmd::SetBg(value) => {
                        let setwv = settings_win.as_ref().map(|(_, w)| w);
                        set_and_broadcast(
                            &mut settings,
                            &screens,
                            setwv,
                            "bgtype",
                            serde_json::Value::String(value.clone()),
                            None,
                        );
                        if let Some(t) = &tray {
                            t.set_active(&value);
                        }
                    }
                    TrayCmd::OpenSettings => {
                        if let Some((w, _)) = &settings_win {
                            os::focus_window(w);
                        } else {
                            match build_settings_window(target, proxy.clone()) {
                                Ok(pair) => {
                                    init_settings_window(&pair.1, &settings, &screens);
                                    os::focus_window(&pair.0);
                                    settings_win = Some(pair);
                                }
                                Err(e) => eprintln!("[shell] 设置窗创建失败: {e}"),
                            }
                        }
                    }
                    TrayCmd::Quit => {
                        *control_flow = ControlFlow::Exit;
                        return;
                    }
                }
            }
            Event::UserEvent(UserEvent::Ipc(msg)) => {
                handle_ipc(msg, &mut settings, &screens, &settings_win, &tray);
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
            // 退出时把壁纸窗从桌面层摘下 + 刷新桌面，清 WorkerW 残影（Windows）；mac no-op。
            Event::LoopDestroyed => {
                for s in &screens {
                    os::detach_from_desktop(&s.window);
                }
                os::refresh_desktop();
            }
            Event::WindowEvent {
                event: WindowEvent::CloseRequested,
                window_id,
                ..
            } => {
                // 关设置窗只销毁它并切回纯菜单栏形态；关壁纸窗才退应用。
                if settings_win
                    .as_ref()
                    .is_some_and(|(w, _)| w.id() == window_id)
                {
                    settings_win = None;
                    os::hide_dock();
                } else {
                    *control_flow = ControlFlow::Exit;
                    return;
                }
            }
            _ => {}
        }
        let _ = &tray; // 仅保活托盘句柄

        // 到点（或兜底超时）淡入对应窗口。
        let now = Instant::now();
        for s in screens.iter_mut() {
            if !s.revealed && now >= s.reveal_at.unwrap_or(s.fallback_at) {
                os::set_alpha(&s.window, 1.0);
                s.revealed = true;
            }
        }

        // 下一次唤醒 = 最早的待淡入时刻；全部淡入后转纯等待。
        *control_flow = match screens
            .iter()
            .filter(|s| !s.revealed)
            .map(|s| s.reveal_at.unwrap_or(s.fallback_at))
            .min()
        {
            Some(next) => ControlFlow::WaitUntil(next),
            None => ControlFlow::Wait,
        };

        // 音频律动：按节拍取频谱下发，并让事件循环保持该节拍唤醒。
        if let Some(a) = &audio {
            let now = Instant::now();
            if now >= next_audio {
                push_audio(&screens, &a.bands(), a.sample_rate);
                next_audio = now + AUDIO_TICK;
            }
            *control_flow = ControlFlow::WaitUntil(next_audio);
        }

        // 显示器热插拔：定期比对排布，变化则重建全部壁纸窗（新屏建/去屏关/重排重贴）。
        let now = Instant::now();
        if now >= next_screen_check {
            next_screen_check = now + SCREEN_CHECK;
            let sig = screens_signature(target);
            if sig != last_screens_sig {
                last_screens_sig = sig;
                rebuild_screens(target, &url, &proxy, debug_top, &mut screens);
            }
        }
        // 保证按屏检查节拍唤醒（与淡入/音频取更早者）。
        *control_flow = match *control_flow {
            ControlFlow::Wait => ControlFlow::WaitUntil(next_screen_check),
            ControlFlow::WaitUntil(t) => ControlFlow::WaitUntil(t.min(next_screen_check)),
            other => other,
        };
    });
}
