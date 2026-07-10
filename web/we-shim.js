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

  // 文档解析伊始就铺深色底，消除页面 CSS 默认白底在设置注入前的闪白。
  var base = document.createElement('style');
  base.textContent = 'html,body{background:#2e2e2e}';
  (document.head || document.documentElement).appendChild(base);

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

  /// 复刻 WE「加载后下发一次全部属性」的行为：读 project.json 的默认值注入页面，
  /// 使壁纸按原版配置渲染（深色底 + 深色主题），而非页面的白底代码兜底。
  /// ponytail: M2 起改由壳侧持久化配置下发，此处 fetch 默认值作为过渡地基。
  window.addEventListener('load', function () {
    fetch('xuanji://localhost/project.json')
      .then(function (r) { return r.json(); })
      .then(function (cfg) {
        var props = cfg && cfg.general && cfg.general.properties;
        if (props) window.__xuanjiApplyUserProperties(props);
      })
      .catch(function () {});
  });
})();
