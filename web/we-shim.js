// 璇玑 WE API shim —— 在页面脚本运行前注入，让页面「以为还在 Wallpaper Engine 里」。
// 契约见 CLAUDE.md「WE API shim 契约」。M0 仅提供跑通所需的最小存根。
(function () {
  'use strict';

  // 把页面 console / 未捕获错误经 IPC 转发到壳侧 stderr，便于无 devtools 时排查。
  function ipcLog(level, args) {
    try {
      if (!(window.ipc && window.ipc.postMessage)) return;
      var s = Array.prototype.map
        .call(args, function (a) {
          try {
            return typeof a === 'object' ? JSON.stringify(a) : String(a);
          } catch (e) {
            return String(a);
          }
        })
        .join(' ');
      window.ipc.postMessage(level + ' ' + s);
    } catch (e) {
      /* ignore */
    }
  }
  ['log', 'warn', 'error', 'info'].forEach(function (m) {
    var orig = console[m];
    console[m] = function () {
      ipcLog(m.toUpperCase(), arguments);
      if (orig) orig.apply(console, arguments);
    };
  });
  window.addEventListener('error', function (e) {
    ipcLog('ONERROR', [e.message + ' @ ' + (e.filename || '') + ':' + (e.lineno || '')]);
  });

  // 防闪白底只给 html 不给 body（body 不透明会盖住负 z-index 的粒子层）。
  // 且 WKWebView 不合成负 z-index 的 2D canvas，故把粒子/闪电层抬到非负层
  // （仍在 z-index:10 的卡片之下）——否则原版粒子背景在本壳里整层不可见。
  var base = document.createElement('style');
  base.textContent =
    'html{background:#2e2e2e}#particles-js{z-index:0!important}#lightning-canvas{z-index:1!important}';
  var mount = document.head || document.documentElement;
  if (mount) {
    mount.appendChild(base);
  } else {
    // WebView2 在 document_start 注入时 DOM 尚空（WKWebView 那时已就绪）——直接 append 会
    // 抛错中断整个 shim（连带 __xuanjiApplyUserProperties 等全局都不定义，托盘切背景失效）。
    // 故 DOM 未就绪时延迟到就绪再注入，绝不阻断后续全局定义。
    document.addEventListener('DOMContentLoaded', function () {
      (document.head || document.documentElement).appendChild(base);
    });
  }

  /// 页面通过它注册 128 段 FFT 回调（WE→页面 提供的 API）。
  window.wallpaperRegisterAudioListener = function (cb) {
    window.__xuanjiAudioCb = typeof cb === 'function' ? cb : null;
  };

  /// 壳侧按节拍下发 128 段频谱：转给页面粒子回调 + 新特效引擎。
  window.__xuanjiPushAudio = function (arr) {
    try {
      if (window.__xuanjiAudioCb) window.__xuanjiAudioCb(arr);
    } catch (e) {
      /* ignore */
    }
    try {
      if (window.XuanjiFx && window.XuanjiFx.setAudio) window.XuanjiFx.setAudio(arr);
    } catch (e) {
      /* ignore */
    }
  };

  /// 壳侧注入设置的入口：转调页面自挂的 wallpaperPropertyListener。
  /// 页面尚未挂上时静默跳过（页面稍后用自身默认值渲染）。
  window.__xuanjiApplyUserProperties = function (props) {
    var l = window.wallpaperPropertyListener;
    if (l && typeof l.applyUserProperties === 'function') l.applyUserProperties(props);
  };
  window.__xuanjiApplyGeneralProperties = function (props) {
    var l = window.wallpaperPropertyListener;
    if (l && typeof l.applyGeneralProperties === 'function') l.applyGeneralProperties(props);
  };

  // 配置下发权归壳侧：页面加载完成后由 Rust 壳读持久化配置调 __xuanjiApplyUserProperties
  // 下发（含用户 overrides）。页面不再自 fetch 默认值，避免异步回冲壳注入的 overrides。
})();
