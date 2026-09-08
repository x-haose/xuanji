# 璇玑 Xuanji

跨平台东方玄学**动态壁纸**。产品名「璇玑」（不带「壁纸」二字），英文 `Xuanji`，副标题「东方玄学动态壁纸」。取自北斗「璇玑玉衡」——上古演算天象历数的旋转天文仪器。

- 仓库/包名/标识：`xuanji`
- macOS Bundle ID：`dev.xuanji.app`（占位，有域名再换）
- 目标平台：**Windows + macOS 起步**，Linux 后续（Wayland 仅 wlroots 系）
- 支持版本（不求全兼容，够用即止）：**Windows 10 22H2（64 位）+ Windows 11**（依赖 WebView2 Runtime；砍 32 位 / Win10 EOL 前版本 / Win7·8）；**macOS 14.6 Sonoma 及以上**（含 15 / 26，Apple Silicon+Intel）。mac 地板定在 14.6 = CoreAudio tap 音频律动的下限，全功能零降级分支。

## 由来与核心决策

源自一个 Wallpaper Engine 壁纸：`~/Downloads/干支四化流注/`（`干支四化流注.html` 2252 行 + `particles.min.js` + `LunarCalendar.min.js` + `assets/js/lunar.js`）。功能：八字天干地支、紫微斗数四化、五运六气、万年历、个人十神日历，背景支持粒子/渐变/纯色/图片/视频/轮播 + 音频律动。

**架构决策（已定，勿推翻）：保留 Web 核心，用 Rust 造壳。**

那份 HTML 本质是普通静态网页，WE 只提供了 4 件事，脱离 WE 就是自己补上：桌面壁纸层描画、系统音频 FFT、原生文件对话框、设置注入。

- **不重写占卜逻辑，不上 Bevy/wgpu 原生渲染。** 壁纸 80% 画面是复杂 CJK 排版 + 玻璃拟态，是 HTML/CSS 主场；原生渲染要重造文字引擎，方向相反且有「做得比原版差」的风险。保留 Web 核心 = 逐像素等同原版，天然锁死「≥ 原版」地板。
- **要着色器不等于要原生渲染**：特效走 WebView 内的 canvas。**基线 = WebGL2**（WKWebView/WebView2 都无版本门槛全稳，粒子/流体/噪声场全能扛）。**WebGPU 评估后搁置**——覆盖面窄（mac 仅 26+ Apple Silicon，Win WebView2 需注入 `--enable-features` flag，地板 14.6 用户绝大多数没有），壁纸场景吃不到其 compute 性能红利（反而要省电降帧），不值维护双渲染路径；除非日后出现 WebGL2 真扛不动的具体特效（超大规模 compute 粒子 / 3D 体渲染流体）再评估。
- **新特效一律增量叠加，原 canvas 粒子模式永远保留为兜底**——「一定不比原版差」由架构保证。

## 技术栈（只用现役库，杜绝过期）

- 壳框架：`tao`（窗口）+ `wry`（WebView，用系统内置引擎，不打包 Chromium）。壁纸窗口非常规，不套完整 Tauri 以免其窗口管理打架。**`wry` 锁 ≥ 0.55.1**——旧版 `build_as_child` 在 macOS 26 Tahoe 会 ObjC 崩溃且 `catch_unwind` 抓不住，0.55.0 才修。
- 窗口句柄：`raw-window-handle 0.6`
- Windows 壁纸层：`windows` crate（**非** `winapi`），给 `Progman` 发 `0x052C` 生成 `WorkerW`，`SetParent` 挂为其子窗口。**Win11 24H2 起 WorkerW 开机后非立即存在**，须防御：`0x052C` 发两次+延迟、轮询等 WorkerW 出现、监听 DWM 崩溃/壁纸切换后重建、退出时 `SetParent(NULL)`+刷新桌面。
- macOS 壁纸层：`objc2` + `objc2-app-kit`（**非**已进入维护期的 `cocoa`），`NSWindow.level` = desktop+1（夹在真壁纸之上、图标之下）、`collectionBehavior` 全 Space、`ignoresMouseEvents` 点击穿透；styleMask 用 `.borderless`（带 `.titled` 会让全 Space 失效）。
- 音频：**两平台统一 `cpal`（0.18+）+ `rustfft`（6.4+）**。Win = WASAPI loopback；mac = CoreAudio loopback（底层 macOS 14.2+ 的 Core Audio process tap，cpal 已内建）。**不用 `ScreenCaptureKit`**——它要屏幕录制权限，对壁纸程序体验灾难，且 cpal 相关 PR 已废弃。mac 音频律动地板 = **macOS 14.6**，13–14.5 区间标「律动降级不可用」。
- 错误处理：`Result` + `thiserror`，生产路径不 `unwrap`
- Web 特效层新代码：TypeScript strict
- **平台层参考**：`ownself/wewa`（GitHub，Rust 同栈网页壁纸）的 `platform/windows/wallpaper.rs`、`platform/macos/wallpaper.rs` 是已验证的 WorkerW/NSWindow 魔数与调用序列活文档。**只读思路、照公共技术自己重写，绝不 fork**——它无 LICENSE（法律上不可复制）且版本全面落后（wry 0.44 / `cocoa` / windows 0.52）。

## WE API shim 契约

页面依赖两个全局，壳侧复刻，页面「以为还在 WE 里」：

- `window.wallpaperPropertyListener.applyUserProperties(props)` — 设置注入。属性键见 `~/Downloads/干支四化流注/project.json`：`bgtype/bgcolor/bgimage/videofile/bgdir/slideshowinterval`、`particlestyle/particlecolor(mode)/particlecount/particlespeed/particlesize/particleopacity/lineenable/linedistance/lineopacity/movedirection`、`gradientcolor*/gradientangle/gradienttype/gradientspeed`、`audioreactive/audiosensitivity/audiobass/audiotreble`、`darktheme`、`glassopacity/glassblur/glasssaturation/glassborder`、`showtitle`（雷祖圣号显隐）、`userRiGan`（日干设置）、`fps`。全部保留，一个不删。
- `window.wallpaperRegisterAudioListener(cb)` — 128 段 FFT 数组回调。

## 质量契约（硬性）

- 分层目录按功能组织：`os/`（壁纸层）`audio/` `ipc/` `settings/` `effects/`。单文件 200–400 行，上限 800。
- 提交门槛：`cargo clippy -- -D warnings` + `rustfmt` 零告警；TS 侧 ESLint 零告警。不在源码写 `#![deny(warnings)]`（随工具链会脆断）。
- 不可变优先、边界校验输入、错误显式处理、无一次性抽象（不为单实现造 trait / 不为不变值造 config）。
- **注释**：只用 `///` 文档注释说明「是什么 / 为什么（非显而易见处）」；禁止过程叙述、对话式、解释显而易见代码的冗余注释。走捷径处用一行 `// ponytail:` 标注天花板与升级路径。
- **验证**：干支/四化换算等纯函数留最小 `#[test]`/断言自检；OS 副作用留可手动跑的冒烟检查。不套框架。

## 里程碑（按「视觉优先」重排）

先把画面做到明显超过原版，再做设置功能，再上 Windows，最后打包。

- **骨架**（✅ 完成）：`tao+wry` 起窗 + WE shim + 原版壁纸逐像素跑通（原 M0）。

- **阶段一 · 显示（做到好看为止）**
  - 1.1 基础显示收尾（✅ mac）：每 `NSScreen` 一窗一 webview，用原生 `NSScreen` frame 铺屏（绕开 tao 多屏坐标偏差）；`.accessory` 去 Dock 图标；各窗 alpha 淡入防闪。⏳ 显示器热插拔重建待补。
  - 1.2 特效升级（✅ 四个 WebGL2 特效 + 后期处理 + 鼠标交互）：
    - 引擎 `web/effects/fx.js`：全屏 canvas 垫底、rAF、注册表、包裹 `applyCustomBackground` 接线、`?fx=` 预览、`?fxdebug` HUD、卡片深色背衬保可读、**后期处理管线**（离屏 FBO → bloom 泛光 + 颗粒 + 暗角 + 边缘色散）。
    - 四特效：`starfield`(星空北斗)/`ink`(水墨,单色墨调)/`thunder`(雷法,锯齿+分叉)/`flowfield`(流场星尘,transform-feedback GPU 粒子 + 拖尾流线)。作为新 `bgtype`，原生模式不受影响。
    - **鼠标交互**：壳侧 `NSEvent` 全局监视器(不破穿透/无需授权) → proxy → 逐屏归一化 → `evaluate_script` 喂 `XuanjiFx.pointer` → `mouse{x,y,influence}`（平滑 + 空闲衰减,底层动画永不停）。响应：flowfield 漩涡气眼 / starfield 视差 / ink 搅墨 / thunder 光标劈雷。`XUANJI_FX="fx&fxmouse=auto"` 自测画圈。
    - **五行驱动配色**：读日干(`#code-gan-day`)→五行→`XuanjiFx.accent` 主色（甲乙木青/丙丁火赤/戊己土黄/庚辛金白/壬癸水玄），四特效配色 + 星盘时钟光晕都据此染，每天自动换色（`--fx-accent` CSS 变量）。
    - **特效深化**：ink 浓淡对比拉大；thunder 电光核心收细 + 三层云纵深；starfield 银河带 + 偶发流星；星盘时钟同心环光晕(加性 CSS，fx-active 门控)。
    - 调试基建：`we-shim.js` 把 `console.*`/未捕获错误经 wry IPC 转发到 stderr（`[web] ...`）。
    - **特效切换**（阶段二已正式化）：`XuanjiFx.select(name)`=`forced` 覆盖，现仅 `?fx=` 预览用；正式切换已并入 `bgtype`（托盘背景子菜单 / 设置下拉，走 `set_and_broadcast`→持久化）。⌃⌥→ 循环快捷键 + mac 全局键监听已移除（都是背景了，单给特效循环别扭）。`XUANJI_FX=<name>` 仍可 URL 强制预览。
    - **星盘时钟**：`enhanceClock` 给原表盘加性注入浑天仪叠层（同心环 + 二十八宿刻度 + 黄道/赤道斜环 + 十二地支）。地支按 **12 小时表盘上下午分组**（上午子—巳、下午午—亥，6 位置每 60°）落在时针对应钟点，`updateShichen` 按真实时间填充并高亮当前时辰；原阿拉伯数字/紫色读数经 JS 内联样式改掉；时钟块 `--clock-size:230`。
    - **UI 改造**（全部 fx-active 门控、JS 注入、stop 还原；开发用 `python3 -m http.server` + headless Chrome 截图迭代，非盲调）：`enhanceBazi` 八字命盘大四柱（年月日时，干上支下，五行色）；`enhanceProgress` 本日进度发光圆环；`enhanceSihua` 四化彩色药丸（禄绿/权紫/科蓝/忌红 + 小标）；`enhanceBento` 宜忌拉成通栏页脚、内容排成两列网格；玻璃卡片分层阴影 + 顶部内高光边；卡片随光标 3D 微倾斜(`tiltCards`)。
    - ⏳ 待深化：flowfield 速度上色（`RENDER_VS` 重算 curl 得速率上色，浏览器 OK 但 **WKWebView 触发 GL_INVALID_OPERATION(1282)** 已回退，需换不触发该错误的实现——如把速率存进 transform feedback 而非 render 时重算）。（真流体墨、bento 已完成；WebGPU 评估后搁置——见「由来与核心决策」。）
    - **特效架构要点（踩坑记）**：四特效渲到后期处理的离屏 scene FBO（bloom/颗粒/暗角/色散再合成）。浏览器对默认帧缓冲每帧自动清、但**离屏 FBO 不自动清**——不完全铺满的 starfield（nebula 未满 + 星点加色叠加）必须自己每帧 `gl.clear` scene，否则累积成放射拖尾；thunder（全屏 shader 覆盖）/flowfield（trail FBO blit）/ink（ping-pong blit）各自全屏覆盖 scene，无需清、也不可在 post 层一刀切清（会误伤）。thunder 暗态（暗云+偶发闪电）是设计非 bug。
  - 1.3 音频律动（✅ mac）：`audio.rs` 用 `cpal` 对默认输出设备 `build_input_stream`（自动 Core Audio process tap，无需授权、不弹框）→ `rustfft` 128 段对数分桶 → 事件循环每 33ms `evaluate_script` 下发。`we-shim.js` 的 `__xuanjiPushAudio` 同时喂 WE 回调与 `XuanjiFx.setAudio`（分 bass/mid/treble，attack 0.6/decay 0.2 平滑）。四特效各自律动：starfield 鼓点胀星+星云明灭、ink 音量涌墨、flowfield 鼓点加速冲刺+提亮、thunder 重拍云海轻闪+额外闪电（阈值门控防泛白）。⏳ Win WASAPI 待阶段三验证。
  - 1.4 视觉打磨：整体观感、过渡、默认配色，锁定「明显超过原版」。

- **阶段二 · 功能（设置/配置/参数）**
  - 2.1 设置系统（✅ mac）：`tray-icon` 跨平台托盘（背景子菜单 `CheckMenuItem` 勾选当前项，与设置下拉同源）+ 透明毛玻璃设置窗（另开 wry 窗）+ 迁移 `project.json` 全部参数 + `rfd` 文件/文件夹对话框。
  - 2.2 配置持久化 + IPC 热重载（✅ 一并完成）：`serde_json`→`directories` config dir（overrides-only + 命名预设），改动实时 `applyUserProperties` 下发不重启。
  - 2.3 每屏独立配置（✅ mac）：`settings.rs` 加 per-screen 覆盖层，`resolved_for(screen)` = 默认 ⊕ 全局 ⊕ 该屏（该屏优先，即便等于 schema 默认也显式存不回落）。壳侧屏键取 **CGDisplayID**（`os::screen_id`，回落显示器名/序号——同型号双屏也各异不撞车）；设置窗注入屏列表 + 每屏解析值，Svelte 头部「作用范围」选择器（>1 屏才显示），`ipc.Set`/`Pick` 带 `screen` 目标。两条关键语义（皆为踩坑后定）：① **`screen_scope`：per-screen 仅多屏生效**，单屏一律走全局——否则多屏设过、拔屏变单屏的遗留覆盖会静默劫持画面且单屏 UI 无入口解除；② **`set(…, None)` = 所有屏统一**（托盘天生全局唯一，「所有屏」操作清掉各屏对该键的专属覆盖 + 设全局），只有显式选某屏才写专属——杜绝「所有屏」切不动有覆盖的屏。

- **阶段三 · 跨平台（Windows）**
  - 3.1 Windows 壁纸层（✅ Parallels ARM64 Win11 24H2 实测通过）：`os/windows.rs` —— 给 Progman 发 `0x052C`（发多次+轮询等 WorkerW 出现）→ **多策略找 WorkerW**：先查 Progman 直接子窗（**24H2 实测就是这种**，经典的顶层 `SHELLDLL_DefView` 兄弟已失效），再退回经典，都失败打印窗口结构诊断 → 壁纸窗设 `WS_CHILD` + `WS_EX_TOOLWINDOW|NOACTIVATE|TRANSPARENT|LAYERED`（隐任务栏/不抢焦/点击穿透/可淡入）→ `SetParent` 挂上 → 按原屏坐标 `SetWindowPos` 铺满（-1/+2 补 WebView2 描边）；`set_alpha` 走 `SetLayeredWindowAttributes` 淡入；退出 `LoopDestroyed` 逐窗 `SetParent(NULL)`+隐藏 + `SPI_SETDESKWALLPAPER` 刷新清残影。**两处 WebView2 兼容坑（实测踩出）**：① wry 在 Win 把自定义协议映射为 `http://xuanji.localhost/`（非 mac 的 `xuanji://localhost/`），页面内 `fetch` 不能硬编码 scheme——设置窗 `schema.ts` 改相对 origin 的 `/project.json`；② WebView2 在 `document_start` 注入脚本时 DOM 尚空（WKWebView 那时已就绪），`we-shim` 防闪底色 `appendChild` 抛错会中断整个 shim（连带 `__xuanjiApplyUserProperties` 不定义→托盘切背景失效）——改为 DOM 未就绪时延迟到 `DOMContentLoaded`。**mac 交叉编译 check + clippy 零告警。** **鼠标交互（✅ 代码）**：`install_mouse_monitor` 起后台线程按 ~60Hz 轮询 `GetCursorPos`（虚拟屏坐标，无钩子/无授权/不破穿透，对应 mac NSEvent 监视器），guard 落时停线程；`screen_norm` 用 `EnumDisplayMonitors` 逐屏 rcMonitor 换算，**Y 翻成向上（0=底）与 mac 约定一致**保鼠标特效不上下颠倒。**设置窗毛玻璃（✅ 代码）**：`add_vibrancy` 走 DWM `DWMWA_SYSTEMBACKDROP_TYPE=DWMSBT_TRANSIENTWINDOW`（Win11 Acrylic，Win10 忽略退化普通透明窗）。⏳ 未做（多需真机）：多屏坐标实测（单屏已验；`screen_norm`/`EnumDisplayMonitors` 顺序与 tao `available_monitors` 是否一致待多屏核）、退出残影待确认、DWM 崩溃/壁纸切换后自动重建、`screen_id`（win 仍 None，per-screen 键回落显示器名`\\.\DISPLAYn`——要更稳需读 EDID 且真机验，暂缓）。
  - 3.2 Windows 音频（✅ 代码，⏳ 真机验）：`audio.rs` 用输出设备开 `build_input_stream` 走 loopback（mac process tap / Win WASAPI 同一路径）。按 `sample_format()` 分派 F32/I16/U16、回调统一 `f32::from_sample_` 下混——不写死 f32，非 F32 设备（Win WASAPI 混音可能 I16）不再 BuildStreamError 后静默无律动。mac 交叉 check + clippy 零告警；Win 真机律动待验。
  - 3.3 Windows 上跑通设置/特效/音频全链路。

- **阶段四 · 打包发布**
  - 4.1 资源内嵌（✅）：`rust-embed` 把 `web/` 嵌入二进制（排除 `settings/node_modules` 54M）；`protocol.rs` 从内嵌资源 serve（保留 `..` 穿越防护），`main.rs` 去掉 `web_dir` 全链路 threading。**debug 仍运行时读源码树（dev 热更不受影响），release 才真嵌入**——故 release 打包前须先 `npm run build` 生成 `web/settings/dist`（gitignored）。
  - 4.2 开机自启、全屏应用暂停省电、自适应帧率（`fps`）。
  - 4.3 安装包：Win MSI/NSIS，mac dmg + 签名公证。

**当前状态**：**阶段一 · 显示 + 阶段二（2.1/2.2/2.3）+ 阶段四 4.1 资源内嵌（mac 侧）+ 阶段三 3.1 Windows 壁纸层（Parallels ARM64 Win11 24H2 真机通过）完成**。
- M0：`tao 0.35 + wry 0.55.1`（**wry 开 `transparent` feature**，否则透明代码被 gate 掉不编译）起窗，`xuanji://` 协议 serve `web/`（路径穿越防护），注入 `we-shim.js`，原版壁纸逐像素跑起来。
- M1 mac：`os/macos.rs` 把壁纸 `NSWindow` 沉 `desktop+1` + 全 Space + 点击穿透 + 铺满多屏；`alpha=0`→页面就绪淡入防闪。`XUANJI_DEBUG_TOP=1` 普通置顶调试开关（可截图看壁纸）。
- 阶段一：四 WebGL2 特效 + 后期处理 + 鼠标交互 + 五行配色 + 星盘时钟 + UI 改造 + 音频律动，均 mac 跑通。
- **阶段二设置系统**：`settings.rs`（schema 驱动单一数据源，`project.json` 为唯一 schema 声明，加参数零 Rust 改动；overrides-only 持久化 + 命名预设）、`ipc.rs`（设置窗↔壳 JSON 协议）、`os/tray.rs`（tray-icon 托盘）。设置窗 = 透明 wry 窗 + `NSVisualEffectView` 毛玻璃 + `.accessory↔.regular` 激活切换前置 + 居中主屏；Svelte5+TS 数据驱动面板（读 `project.json` 自动渲染 + `condition` 联动显隐 + 玻璃五行）。托盘切背景与设置改参共用 `set_and_broadcast`。
- **四特效并入 bgtype**：星汉灿烂/水墨氤氲/雷霆万钧/流光星尘 作为 `bgtype` 选项，与纯色/粒子互斥、可持久化（引擎本就支持 bgtype-as-effect，原版页面零改动）。
- 关键坑（已解）：① 粒子在 WKWebView 整层不可见——we-shim body 不透明底盖住负 z-index 粒子层，且 **WKWebView 不合成负 z-index 2D canvas**，故底色只给 `html` + 抬粒子/闪电层到非负；② 配置下发须 `{value:}` 包装（WE 契约读 `props[key].value`）；③ 音频参数统一到 5 种律动背景并接进 `fx.js`（原先四特效无视这些参数）。
- 显示器热插拔（✅）：事件循环轮询排布签名，变化则重建全部壁纸窗（每窗独立兜底淡入）。
- **每屏独立配置（✅ mac）**：per-screen 覆盖层 + CGDisplayID 稳定屏键 + 作用范围选择器（详见阶段二 2.3）。
- **资源内嵌（✅）**：rust-embed 嵌入 web/（详见阶段四 4.1）。
- 门槛：clippy 零告警、fmt 干净、17 测过、svelte-check 零错。
- **下一步**：阶段三 3.2 Windows 音频（`cpal` WASAPI loopback 验证）+ 3.3 全链路，或 Windows 观感打磨（设置窗 Mica/Acrylic 玻璃、鼠标交互、多屏坐标实测）。⏳ 遗留：开机自启（阶段四，需签名）、每屏配置在设置窗打开时热插拔的屏列表刷新（当前仅开窗/切预设时刷新）。
