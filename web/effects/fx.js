// 璇玑特效引擎：一块全屏 WebGL2 canvas 垫在占卜卡片之下，各特效作为新 bgtype 接入。
// 不改占卜核心；通过包裹 applyCustomBackground 感知 bgtype 切换，或 URL ?fx= 强制预览。
(function () {
  'use strict';

  var DPR = Math.min(window.devicePixelRatio || 1, 2);
  var registry = {}; // name -> factory(gl, canvas) -> { frame(t,dt), resize(w,h), dispose() }
  var DEBUG = false; // ?fx= 预览时置真，把状态显示到屏幕便于排查

  /// 屏幕左上角调试面板（无法读壁纸 console 时靠它反馈状态）。
  function hud(msg) {
    if (!DEBUG) return;
    var el = document.getElementById('xuanji-fx-hud');
    if (!el) {
      el = document.createElement('div');
      el.id = 'xuanji-fx-hud';
      el.style.cssText =
        'position:fixed;top:10px;left:10px;z-index:99999;color:#5f5;' +
        'font:12px monospace;background:rgba(0,0,0,.65);padding:8px;white-space:pre;pointer-events:none;';
      document.body.appendChild(el);
    }
    el.textContent = msg;
  }

  var canvas = null;
  var gl = null;
  var post = null; // 后期处理管线（bloom+grain+暗角+色散），失败则为 null 走直渲
  var current = null; // 当前特效实例
  var currentName = null;
  var raf = 0;
  var startTs = 0;
  var lastTs = 0;

  // 鼠标交互：底层动画永远自行运行，指针只是叠加的局部扰动。
  // mouse.influence 由「近期是否移动」驱动，停手后衰减归零 → 无缝回纯氛围。
  var mouse = { x: 0.5, y: 0.5, influence: 0 }; // 平滑后、暴露给特效（y 下→上）
  var pTarget = { x: 0.5, y: 0.5 }; // 壳侧送来的最新原始坐标
  var pInf = 0; // influence 目标：移动置 1，每帧衰减
  var autoMouse = false; // ?fxmouse=auto 时指针自动画圈，供自测
  var forced = null; // 强制/菜单选定的特效名；优先于 bgtype

  // 音频律动：壳侧按节拍喂 128 段频谱，这里聚合成 level/bass/mid/treble 供特效读。
  var audio = { level: 0, bass: 0, mid: 0, treble: 0 };
  function setAudio(arr) {
    if (!arr || !arr.length) return;
    // 尊重设置面板的音频参数（与粒子背景一致，这些是页面挂的全局变量）：
    // 关闭律动则归零；否则按律动强度/低频/高频权重缩放。
    if (window.audioReactive === false) {
      audio.bass = audio.mid = audio.treble = audio.level = 0;
      return;
    }
    var sens = typeof window.audioSensitivity === 'number' ? window.audioSensitivity : 1;
    var bw = typeof window.audioBass === 'number' ? window.audioBass : 1;
    var tw = typeof window.audioTreble === 'number' ? window.audioTreble : 1;
    var n = arr.length,
      bass = 0,
      mid = 0,
      treble = 0,
      all = 0;
    var b1 = Math.floor(n * 0.12),
      b2 = Math.floor(n * 0.45);
    for (var i = 0; i < n; i++) {
      var v = arr[i];
      all += v;
      if (i < b1) bass += v;
      else if (i < b2) mid += v;
      else treble += v;
    }
    // 目标值 → 平滑到 audio（起快落慢，视觉顺）
    var tb = (bass / Math.max(1, b1)) * sens * bw,
      tm = (mid / Math.max(1, b2 - b1)) * sens,
      tt = (treble / Math.max(1, n - b2)) * sens * tw,
      tl = (all / n) * sens;
    audio.bass += (tb - audio.bass) * (tb > audio.bass ? 0.85 : 0.3);
    audio.mid += (tm - audio.mid) * (tm > audio.mid ? 0.85 : 0.3);
    audio.treble += (tt - audio.treble) * (tt > audio.treble ? 0.85 : 0.3);
    audio.level += (tl - audio.level) * (tl > audio.level ? 0.85 : 0.3);
  }

  /// 决定当前该显示哪个特效：强制选定 > 已注册的 bgtype > 无。
  function pick() {
    if (forced) return forced;
    return registry[window.currentBgType] ? window.currentBgType : null;
  }

  function updatePointer(t) {
    if (autoMouse) {
      pTarget.x = 0.5 + 0.32 * Math.cos(t * 0.8);
      pTarget.y = 0.5 + 0.32 * Math.sin(t * 0.8);
      pInf = 1;
    }
    mouse.x += (pTarget.x - mouse.x) * 0.15;
    mouse.y += (pTarget.y - mouse.y) * 0.15;
    mouse.influence += (pInf - mouse.influence) * 0.12;
    pInf *= 0.92; // 停止移动后逐帧衰减 → influence 随之归零
    tiltCards();
  }

  /// 卡片随光标做轻微 3D 倾斜+视差，营造悬浮玻璃的厚度。跟随平滑坐标，停手保持。
  function tiltCards() {
    if (!document.body.classList.contains('fx-active')) return;
    var cards = document.querySelectorAll('.liquid-glass');
    if (!cards.length) return;
    var rx = ((mouse.y - 0.5) * 7).toFixed(2);
    var ry = ((mouse.x - 0.5) * 7).toFixed(2);
    var tf = 'perspective(1100px) rotateX(' + rx + 'deg) rotateY(' + ry + 'deg)';
    for (var i = 0; i < cards.length; i++) cards[i].style.transform = tf;
  }

  // 今日五行驱动配色：读日干(#code-gan-day)→五行→主色，各特效据此染色，每天自动换一套。
  var ELEMENTS = {
    甲: 'wood', 乙: 'wood', 丙: 'fire', 丁: 'fire', 戊: 'earth',
    己: 'earth', 庚: 'metal', 辛: 'metal', 壬: 'water', 癸: 'water',
  };
  var ACCENTS = {
    wood: [0.3, 0.68, 0.52], // 青
    fire: [0.88, 0.36, 0.28], // 赤
    earth: [0.82, 0.64, 0.34], // 黄
    metal: [0.82, 0.85, 0.92], // 金白
    water: [0.34, 0.54, 0.9], // 玄（深蓝）
  };
  var accent = [0.34, 0.54, 0.9]; // 稳定引用；updateElement 原地改内容
  function updateElement() {
    var el = document.getElementById('code-gan-day');
    var gan = el && el.textContent ? el.textContent.trim().charAt(0) : '';
    var c = ACCENTS[ELEMENTS[gan]];
    if (c) {
      accent[0] = c[0];
      accent[1] = c[1];
      accent[2] = c[2];
      // 供注入的 CSS(时钟光晕等)使用今日五行色
      document.body.style.setProperty(
        '--fx-accent',
        Math.round(c[0] * 255) + ',' + Math.round(c[1] * 255) + ',' + Math.round(c[2] * 255)
      );
    }
  }

  // ── 共享圣号资源：十字天经圣号字形，供各特效在笔画处「生长」出圣名 ──
  // tex：覆盖度纹理（片元 shader 采样）；points：笔画采样点云（星座/发射源）；
  // rect(w,h)：圣号在屏幕上的 uv 矩形（左侧竖幅，与 DOM 版位置一致，gl_FragCoord 下-上）。
  var seal = {
    ready: false, // 纹理已上传
    show: true, // 由 showtitle 开关联动
    tex: null,
    points: null, // Float32Array [lx,ly,...] 局部 uv（0..1，y 向下=从上到下）
    aspect: 520 / 6446, // 图宽/高
    _imgReady: false,
    H_UV: 0.8, // 竖幅占屏高
    X_UV: 0.03, // 左边距
    rect: function (w, h) {
      var huv = this.H_UV;
      var wuv = (huv * this.aspect) / (w / h); // 保持像素纵横比
      return [this.X_UV, (1 - huv) / 2, wuv, huv];
    },
  };
  /// 1×1 透明占位纹理：seal 未就绪时给采样单元兜底一张完整纹理（防 ANGLE-Metal 1282）。
  var _dummyTex = null;
  function dummyTex(gl) {
    if (_dummyTex) return _dummyTex;
    _dummyTex = gl.createTexture();
    gl.bindTexture(gl.TEXTURE_2D, _dummyTex);
    var px = new Uint8Array([0, 0, 0, 0]);
    gl.texImage2D(gl.TEXTURE_2D, 0, gl.RGBA, 1, 1, 0, gl.RGBA, gl.UNSIGNED_BYTE, px);
    gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MIN_FILTER, gl.NEAREST);
    gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MAG_FILTER, gl.NEAREST);
    gl.bindTexture(gl.TEXTURE_2D, null);
    return _dummyTex;
  }

  var sealImg = new Image();
  sealImg.onload = function () {
    seal.aspect = sealImg.naturalWidth / sealImg.naturalHeight;
    try {
      seal.points = buildSealPoints(sealImg, 1600);
    } catch (e) {
      seal.points = new Float32Array(0);
    }
    seal._imgReady = true;
    uploadSealTex();
  };
  sealImg.src = 'assets/img/tianzun-seal.png';

  /// 从圣号 PNG 的 alpha 通道按覆盖度加权采样出笔画点云（确定性，稳定不抖）。
  function buildSealPoints(img, n) {
    var cw = 170, ch = Math.round((cw * img.naturalHeight) / img.naturalWidth);
    var c = document.createElement('canvas');
    c.width = cw;
    c.height = ch;
    var ctx = c.getContext('2d');
    ctx.drawImage(img, 0, 0, cw, ch);
    var data = ctx.getImageData(0, 0, cw, ch).data;
    var cand = [];
    for (var y = 0; y < ch; y++) {
      for (var x = 0; x < cw; x++) {
        if (data[(y * cw + x) * 4 + 3] > 130) cand.push(x / cw, y / ch);
      }
    }
    var out = new Float32Array(n * 2);
    var s = 20260710;
    for (var i = 0; i < n; i++) {
      s = (s * 1103515245 + 12345) & 0x7fffffff;
      var k = (s % (cand.length / 2)) | 0;
      out[i * 2] = cand[k * 2];
      out[i * 2 + 1] = cand[k * 2 + 1];
    }
    return out;
  }

  /// gl 就绪且图已解码后把圣号上传成纹理（幂等）。
  function uploadSealTex() {
    if (!gl || seal.tex || !seal._imgReady) return;
    var tex = gl.createTexture();
    gl.bindTexture(gl.TEXTURE_2D, tex);
    gl.texImage2D(gl.TEXTURE_2D, 0, gl.RGBA8, gl.RGBA, gl.UNSIGNED_BYTE, sealImg);
    gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MIN_FILTER, gl.LINEAR);
    gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MAG_FILTER, gl.LINEAR);
    gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_WRAP_S, gl.CLAMP_TO_EDGE);
    gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_WRAP_T, gl.CLAMP_TO_EDGE);
    gl.bindTexture(gl.TEXTURE_2D, null);
    seal.tex = tex;
    seal.ready = true;
  }

  /// 特效激活时给玻璃卡片垫一层深色背衬，保证亮背景下文字仍清晰可读。
  function injectReadabilityStyle() {
    if (document.getElementById('xuanji-fx-style')) return;
    var s = document.createElement('style');
    s.id = 'xuanji-fx-style';
    s.textContent =
      'body.fx-active .liquid-glass{background:' +
      'linear-gradient(135deg,rgba(255,255,255,calc(var(--glass-alpha)*0.18))0%,' +
      'rgba(255,255,255,calc(var(--glass-alpha)*0.06))25%,' +
      'rgba(255,255,255,calc(var(--glass-alpha)*0.02))55%,' +
      'rgba(255,255,255,calc(var(--glass-alpha)*0.10))100%),rgba(8,10,16,0.55)!important;}' +
      // 液态玻璃质感：分层阴影(深度) + 顶部内高光边(折光) + 细边
      'body.fx-active .liquid-glass{box-shadow:0 26px 70px rgba(0,0,0,0.52),0 4px 16px rgba(0,0,0,0.34),' +
      'inset 0 1px 0 rgba(255,255,255,0.16),inset 0 0 60px rgba(255,255,255,0.02)!important;' +
      'border:1px solid rgba(255,255,255,0.09)!important;}' +
      // 星盘时钟：清晰的今日五行光环 + 内外辉光，数字读数染色发光
      'body.fx-active .analog-clock{box-shadow:0 0 0 1.5px rgba(255,255,255,0.22),' +
      '0 0 0 6px rgba(var(--fx-accent,120,140,230),0.14),' +
      '0 0 0 7.5px rgba(var(--fx-accent,120,140,230),0.55),' + // 清晰光环
      'inset 0 0 26px rgba(var(--fx-accent,120,140,230),0.20),' +
      '0 0 48px rgba(var(--fx-accent,120,140,230),0.45)!important;}' + // 更强外晕
      'body.fx-active .digital-clock{color:#fff!important;text-shadow:0 0 14px rgba(var(--fx-accent,120,140,230),0.9),0 0 4px rgba(var(--fx-accent,120,140,230),0.7)!important;letter-spacing:0.16em!important;font-variant-numeric:tabular-nums;}' +
      // 星盘激活时隐去原阿拉伯数字，改用注入的十二地支
      'body.fx-active .clock-face .num{opacity:0!important;}' +
      // 特效激活时隐藏 DOM 圣号——改由各特效在 canvas 内「生长」出圣名
      'body.fx-active #dao-title{display:none!important;}';
    document.head.appendChild(s);
  }

  /// 给现有圆表盘加性注入浑天仪元素：同心环 + 二十八宿刻度 + 黄道/赤道斜环。
  /// 用今日五行色，显隐由 body.fx-active 控制；不动指针旋转逻辑，幂等。
  /// 八字改造：把「天干/地支」小字行改成命盘式大四柱（年月日时，干上支下，大号五行色）。
  function enhanceBazi() {
    var code = document.getElementById('code');
    if (!code || document.getElementById('xj-bazi-pillars')) return;
    var gk = ['code-gan-year', 'code-gan-month', 'code-gan-day', 'code-gan-hour'];
    var zk = ['code-zhi-year', 'code-zhi-month', 'code-zhi-day', 'code-zhi-hour'];
    var gans = gk.map(function (id) {
      return document.getElementById(id);
    });
    var zhis = zk.map(function (id) {
      return document.getElementById(id);
    });
    var ok = gans.every(function (x) {
      return x && x.textContent.trim();
    });
    if (!ok) return; // 页面尚未填充，稍后重试
    var labels = ['年', '月', '日', '时'];
    var wrap = document.createElement('div');
    wrap.id = 'xj-bazi-pillars';
    wrap.style.cssText =
      'display:flex;gap:22px;justify-content:flex-start;margin:2px 0 16px;padding-left:4px;';
    var bigFont =
      'font-size:1.55em;font-weight:500;line-height:1.15;font-family:"Songti SC","STSong",serif;';
    for (var i = 0; i < 4; i++) {
      var col = document.createElement('div');
      col.style.cssText = 'display:flex;flex-direction:column;align-items:center;gap:5px;';
      var lab = document.createElement('div');
      lab.textContent = labels[i];
      lab.style.cssText = 'font-size:0.58em;color:rgba(255,255,255,0.35);letter-spacing:0.12em;';
      var gan = document.createElement('div');
      gan.textContent = gans[i].textContent.trim();
      gan.style.cssText = bigFont + 'color:' + getComputedStyle(gans[i]).color + ';';
      var zhi = document.createElement('div');
      zhi.textContent = zhis[i].textContent.trim();
      zhi.style.cssText = bigFont + 'color:' + getComputedStyle(zhis[i]).color + ';';
      col.appendChild(lab);
      col.appendChild(gan);
      col.appendChild(zhi);
      wrap.appendChild(col);
    }
    var tg = document.getElementById('code-tiangan');
    var dz = document.getElementById('code-dizhi');
    if (tg) tg.style.display = 'none';
    if (dz) dz.style.display = 'none';
    code.insertBefore(wrap, code.firstChild);
  }

  /// 宜忌拉成通栏页脚：与上方八字+万年历两栏同宽、居中，不再是窄块吊在中间。stop 还原。
  function enhanceBento() {
    var row = document.querySelector('.content-row');
    var yiji = document.getElementById('huangli-panel');
    if (!row || !yiji) return;
    // 页脚内容排成整齐两列网格（左右两栏都左对齐，不再参差）
    yiji.querySelectorAll('.hl-row').forEach(function (r) {
      r.style.display = 'grid';
      r.style.gridTemplateColumns = '1fr 1fr';
      r.style.columnGap = '48px';
      r.style.alignItems = 'start';
    });
    var w = row.offsetWidth; // 网格化后再量上排宽度，据此定页脚同宽
    if (w > 0) {
      yiji.style.boxSizing = 'border-box';
      yiji.style.width = w + 'px';
      yiji.style.maxWidth = 'none';
    }
    var containers = document.querySelectorAll('.container');
    if (containers[1]) containers[1].style.paddingTop = '30px';
  }
  function restoreBento() {
    var yiji = document.getElementById('huangli-panel');
    if (yiji) {
      yiji.style.width = '';
      yiji.style.maxWidth = '';
      yiji.style.boxSizing = '';
    }
    var containers = document.querySelectorAll('.container');
    if (containers[1]) containers[1].style.paddingTop = '12px';
  }

  /// 四化徽章：把四化四颗星（禄绿/权紫/科蓝/忌红）做成彩色药丸 + 禄权科忌小标。
  function enhanceSihua() {
    var ids = ['code-sihua-year', 'code-sihua-month', 'code-sihua-day', 'code-sihua-hour'];
    var labels = ['禄', '权', '科', '忌'];
    function rgba(c, a) {
      var m = c.match(/(\d+),\s*(\d+),\s*(\d+)/);
      return m ? 'rgba(' + m[1] + ',' + m[2] + ',' + m[3] + ',' + a + ')' : 'rgba(255,255,255,' + a + ')';
    }
    ids.forEach(function (id) {
      var el = document.getElementById(id);
      if (!el || el.getAttribute('data-xj-pill')) return;
      var spans = el.querySelectorAll('span[style*="color"]');
      if (spans.length < 4) return;
      for (var i = 0; i < 4; i++) {
        var s = spans[i];
        var col = getComputedStyle(s).color;
        s.style.display = 'inline-flex';
        s.style.alignItems = 'center';
        s.style.gap = '4px';
        s.style.padding = '1px 10px 2px';
        s.style.margin = '0 4px';
        s.style.borderRadius = '999px';
        s.style.background = rgba(col, 0.14);
        s.style.border = '1px solid ' + rgba(col, 0.4);
        var lab = document.createElement('span');
        lab.textContent = labels[i];
        lab.style.cssText = 'font-size:0.66em;opacity:0.72;';
        s.insertBefore(lab, s.firstChild);
      }
      el.setAttribute('data-xj-pill', '1');
    });
  }

  /// 本日进度环：把「本日进度: X%」文字换成发光圆环 + 大号百分比。
  function enhanceProgress() {
    var el = document.getElementById('code-progress');
    if (!el) return;
    var m = el.textContent.match(/([\d.]+)\s*%/);
    if (!m) return;
    var pct = Math.max(0, Math.min(100, parseFloat(m[1])));
    var r = 24,
      c = 2 * Math.PI * r,
      off = (c * (1 - pct / 100)).toFixed(1);
    var box = document.getElementById('xj-progress');
    if (!box) {
      box = document.createElement('div');
      box.id = 'xj-progress';
      box.style.cssText = 'display:flex;align-items:center;gap:16px;margin-top:14px;';
      el.style.display = 'none';
      el.parentNode.insertBefore(box, el.nextSibling);
    }
    box.innerHTML =
      '<svg width="60" height="60" viewBox="0 0 60 60">' +
      '<circle cx="30" cy="30" r="' +
      r +
      '" fill="none" stroke="rgba(255,255,255,0.1)" stroke-width="4"/>' +
      '<circle cx="30" cy="30" r="' +
      r +
      '" fill="none" stroke="rgba(130,185,255,0.92)" stroke-width="4" stroke-linecap="round"' +
      ' stroke-dasharray="' +
      c.toFixed(1) +
      '" stroke-dashoffset="' +
      off +
      '" transform="rotate(-90 30 30)" style="filter:drop-shadow(0 0 4px rgba(130,185,255,0.6))"/>' +
      '</svg>' +
      '<div><div style="font-size:0.6em;color:rgba(255,255,255,0.42);letter-spacing:0.08em">本日进度</div>' +
      '<div style="font-size:1.35em;font-weight:600;color:#eaf2ff">' +
      pct.toFixed(1) +
      '%</div></div>';
  }
  function restoreProgress() {
    var box = document.getElementById('xj-progress');
    if (box) box.remove();
    var el = document.getElementById('code-progress');
    if (el) el.style.display = '';
  }

  /// 特效激活时的排版调整：时钟放大、卡片间距。stop 还原。
  function enhanceLayout() {
    document.documentElement.style.setProperty('--clock-size', '230px'); // 时钟放大
    var row = document.querySelector('.content-row');
    if (row) row.style.gap = '48px'; // 八字↔右列横向间距
  }
  function restoreLayout() {
    document.documentElement.style.removeProperty('--clock-size');
    var row = document.querySelector('.content-row');
    if (row) row.style.gap = '';
  }

  var DIZHI = ['子', '丑', '寅', '卯', '辰', '巳', '午', '未', '申', '酉', '戌', '亥'];

  /// 按真实时间填充 12 小时表盘的六个时辰位并高亮当前时辰。
  /// 每时辰 2h、时针半天一圈：上午(0–12)显示 子丑寅卯辰巳，下午(12–24)显示 午未申酉戌亥；
  /// 第 k 位(0=顶,顺时针每 60°)对应时针在 2k 点方向所处的时辰。
  function updateShichen() {
    var texts = document.querySelectorAll('#xuanji-armilla .xj-dizhi');
    if (!texts.length) return;
    var now = new Date();
    var h = now.getHours();
    var base = h >= 12 ? 6 : 0; // 下午从午(6)起，上午从子(0)起
    var cur = Math.floor(((h + 1) % 24) / 2); // 当前时辰全局索引(0=子)
    texts.forEach(function (t) {
      var shi = base + +t.getAttribute('data-k');
      t.textContent = DIZHI[shi];
      var on = shi === cur;
      t.setAttribute('opacity', on ? '1' : '0.5');
      t.setAttribute('font-size', on ? '15' : '11');
      t.setAttribute('font-weight', on ? '600' : '300');
    });
  }

  function enhanceClock() {
    var svg = document.querySelector('.clock-face');
    if (!svg || document.getElementById('xuanji-armilla')) return;
    var NS = 'http://www.w3.org/2000/svg';
    var col = 'rgba(var(--fx-accent,120,140,230),1)';
    var g = document.createElementNS(NS, 'g');
    g.id = 'xuanji-armilla';
    g.setAttribute('fill', 'none');
    g.setAttribute('stroke', col);
    function ring(r, w, o) {
      var c = document.createElementNS(NS, 'circle');
      c.setAttribute('cx', 100);
      c.setAttribute('cy', 100);
      c.setAttribute('r', r);
      c.setAttribute('stroke-width', w);
      c.setAttribute('opacity', o);
      g.appendChild(c);
    }
    ring(97, 1, 0.85); // 外环
    ring(70, 0.6, 0.4); // 内环
    // 黄道 + 赤道斜环（浑天仪天球圈）
    function ellipse(rx, ry, rot, o) {
      var e = document.createElementNS(NS, 'ellipse');
      e.setAttribute('cx', 100);
      e.setAttribute('cy', 100);
      e.setAttribute('rx', rx);
      e.setAttribute('ry', ry);
      e.setAttribute('transform', 'rotate(' + rot + ' 100 100)');
      e.setAttribute('stroke-width', 0.8);
      e.setAttribute('opacity', o);
      g.appendChild(e);
    }
    ellipse(90, 32, 22, 0.4);
    ellipse(90, 16, 0, 0.28);
    // 二十八宿刻度（外环内侧一圈短刻度）
    for (var i = 0; i < 28; i++) {
      var a = (i / 28) * Math.PI * 2 - Math.PI / 2;
      var l = document.createElementNS(NS, 'line');
      l.setAttribute('x1', (100 + Math.cos(a) * 96).toFixed(1));
      l.setAttribute('y1', (100 + Math.sin(a) * 96).toFixed(1));
      l.setAttribute('x2', (100 + Math.cos(a) * 90).toFixed(1));
      l.setAttribute('y2', (100 + Math.sin(a) * 90).toFixed(1));
      l.setAttribute('stroke-width', 0.7);
      l.setAttribute('opacity', 0.55);
      g.appendChild(l);
    }
    // 六个时辰位（顶起顺时针每 60° = 时针 2 点间隔），文本由 updateShichen 按上下午填充
    for (var k = 0; k < 6; k++) {
      var ta = (k / 6) * Math.PI * 2 - Math.PI / 2;
      var t = document.createElementNS(NS, 'text');
      t.setAttribute('x', (100 + Math.cos(ta) * 82).toFixed(1));
      t.setAttribute('y', (100 + Math.sin(ta) * 82).toFixed(1));
      t.setAttribute('text-anchor', 'middle');
      t.setAttribute('dominant-baseline', 'central');
      t.setAttribute('font-family', "'PingFang SC','Songti SC',serif");
      t.setAttribute('fill', col);
      t.setAttribute('stroke', 'none');
      t.setAttribute('class', 'xj-dizhi');
      t.setAttribute('data-k', k);
      g.appendChild(t);
    }
    svg.appendChild(g);
    updateShichen(); // 按当前上下午填充地支并高亮当前时辰

    // 时钟块下移一点，离顶边更舒展
    var wrap = document.querySelector('.clock-wrapper');
    if (wrap) wrap.style.top = '110px';

    // CSS 压不过原生样式，直接改内联：藏原阿拉伯数字、数字读数染白+发光。
    svg.querySelectorAll('.num').forEach(function (n) {
      n.style.opacity = '0';
    });
    var dc = document.getElementById('digital-clock') || document.querySelector('.digital-clock');
    if (dc) {
      dc.style.color = '#fff';
      dc.style.textShadow =
        '0 0 14px rgba(var(--fx-accent,120,140,230),0.9),0 0 4px rgba(var(--fx-accent,120,140,230),0.7)';
      dc.style.letterSpacing = '0.16em';
    }
  }

  function ensureCanvas() {
    if (canvas) return true;
    injectReadabilityStyle();
    canvas = document.createElement('canvas');
    canvas.id = 'xuanji-fx';
    canvas.style.cssText =
      'position:fixed;inset:0;width:100%;height:100%;z-index:0;pointer-events:none;display:none;';
    document.body.insertBefore(canvas, document.body.firstChild);
    gl = canvas.getContext('webgl2', { antialias: true, alpha: false, premultipliedAlpha: false });
    if (!gl) {
      // 无 WebGL2：静默退场，页面回落到原生 bgtype。
      canvas.remove();
      canvas = null;
      return false;
    }
    try {
      post = createPost(gl);
    } catch (e) {
      post = null;
      console.error('[fx] 后期处理初始化失败，改直渲: ' + (e && e.message ? e.message : e));
    }
    uploadSealTex(); // gl 就绪，若图已解码则上传圣号纹理
    return true;
  }

  function resize() {
    if (!canvas || !gl) return;
    var w = Math.max(1, Math.round(window.innerWidth * DPR));
    var h = Math.max(1, Math.round(window.innerHeight * DPR));
    if (canvas.width !== w || canvas.height !== h) {
      canvas.width = w;
      canvas.height = h;
      gl.viewport(0, 0, w, h);
      if (post) post.resize(w, h);
      if (current && current.resize) current.resize(w, h);
    }
  }

  function loop(ts) {
    raf = requestAnimationFrame(loop);
    if (!current) return;
    if (!startTs) startTs = ts;
    var t = (ts - startTs) / 1000;
    var dt = lastTs ? (ts - lastTs) / 1000 : 0;
    lastTs = ts;
    var d = Math.min(dt, 0.05); // 夹住 dt，防止后台唤醒后大跳
    updatePointer(t);
    if (post) {
      post.begin();
      current.frame(t, d);
      post.end(t);
    } else {
      current.frame(t, d);
    }
  }

  function stop() {
    if (raf) cancelAnimationFrame(raf), (raf = 0);
    if (current && current.dispose) current.dispose();
    current = null;
    currentName = null;
    startTs = lastTs = 0;
    if (canvas) canvas.style.display = 'none';
    document.body.classList.remove('fx-active');
    var arm = document.getElementById('xuanji-armilla');
    if (arm) arm.remove();
    // 还原时钟原生外观
    var svg = document.querySelector('.clock-face');
    if (svg) {
      svg.querySelectorAll('.num').forEach(function (n) {
        n.style.opacity = '';
      });
    }
    var dc = document.getElementById('digital-clock') || document.querySelector('.digital-clock');
    if (dc) {
      dc.style.color = '';
      dc.style.textShadow = '';
      dc.style.letterSpacing = '';
    }
    var wrap = document.querySelector('.clock-wrapper');
    if (wrap) wrap.style.top = '';
    var cards = document.querySelectorAll('.liquid-glass');
    for (var i = 0; i < cards.length; i++) cards[i].style.transform = '';
    restoreBento();
    restoreLayout();
    restoreProgress();
    // 还原八字小字行
    var pil = document.getElementById('xj-bazi-pillars');
    if (pil) pil.remove();
    var tg = document.getElementById('code-tiangan');
    var dz = document.getElementById('code-dizhi');
    if (tg) tg.style.display = '';
    if (dz) dz.style.display = '';
  }

  /// 切换到指定特效；name 未注册或为空则停止（回落原生背景）。
  function set(name) {
    if (name === currentName) return;
    if (!name || !registry[name]) {
      hud('fx: ' + name + ' 未注册（已注册: ' + Object.keys(registry).join(',') + '）');
      stop();
      return;
    }
    if (!ensureCanvas()) {
      hud('fx: ' + name + ' — WebGL2 不可用，已回落');
      return;
    }
    if (current && current.dispose) current.dispose();
    canvas.style.display = 'block';
    // 特效背景是动态内容，玻璃卡片应对其做磨砂，去掉纯色模式加的禁磨砂类。
    document.body.classList.remove('no-glass-blur');
    document.body.classList.add('fx-active'); // 启用卡片深色背衬保可读
    enhanceClock(); // 幂等：给时钟加浑天仪叠层
    enhanceLayout(); // 时钟放大 + 卡片间距
    enhanceBento(); // 宜忌通栏页脚
    enhanceBazi(); // 八字大四柱
    enhanceProgress(); // 本日进度环
    enhanceSihua(); // 四化彩色徽章
    resize();
    try {
      current = registry[name](gl, canvas);
    } catch (e) {
      var msg = e && e.message ? e.message : e;
      hud('fx: ' + name + ' 初始化失败\n' + msg);
      console.error('[fx] ' + name + ' 初始化失败: ' + msg);
      stop();
      return;
    }
    currentName = name;
    startTs = lastTs = 0;
    if (!raf) raf = requestAnimationFrame(loop);
    hud('fx: ' + name + ' 运行中\ncanvas ' + canvas.width + 'x' + canvas.height);
  }

  window.addEventListener('resize', resize);

  // 各特效共用的 GL 小工具。
  var util = {
    /// 配合 fullscreenTriangle 的顶点着色器源码。
    FS_VS:
      '#version 300 es\nlayout(location=0) in vec2 a_pos;\n' +
      'void main(){gl_Position=vec4(a_pos,0.0,1.0);}',
    /// 圣号采样 GLSL 片段：拼进各特效 fragment shader，返回当前像素的笔画覆盖度 [0,1]。
    SEAL_GLSL:
      'uniform sampler2D u_seal; uniform vec4 u_sealRect; uniform float u_sealOn;\n' +
      'float sealCov(vec2 fc, vec2 res){ if(u_sealOn<0.5) return 0.0;\n' +
      ' vec2 s=(fc/res - u_sealRect.xy)/u_sealRect.zw;\n' +
      ' if(s.x<0.0||s.x>1.0||s.y<0.0||s.y>1.0) return 0.0;\n' +
      ' return texture(u_seal, vec2(s.x, 1.0-s.y)).a; }\n',
    /// 设定圣号相关 uniform 并把纹理绑到 unit（供 SEAL_GLSL 采样）。
    bindSeal: function (gl, prog, w, h, unit) {
      var s = window.XuanjiFx.seal;
      var on = s.ready && s.show ? 1 : 0;
      gl.uniform1f(gl.getUniformLocation(prog, 'u_sealOn'), on);
      // 采样单元必须始终绑一张「完整纹理」：sampler2D 指向不完整纹理时，WKWebView/ANGLE-Metal
      // 在 draw 时报 INVALID_OPERATION(1282)——完整性是静态校验，即便采样在未走的分支里也算。
      // 故 seal 未上传/关闭时也绑 1×1 占位纹理（flowfield 在顶点+TF 采样，是最严格必炸路径）。
      gl.activeTexture(gl.TEXTURE0 + unit);
      gl.bindTexture(gl.TEXTURE_2D, s.tex || dummyTex(gl));
      gl.uniform1i(gl.getUniformLocation(prog, 'u_seal'), unit);
      gl.activeTexture(gl.TEXTURE0);
      if (!on) return;
      var r = s.rect(w, h);
      gl.uniform4f(gl.getUniformLocation(prog, 'u_sealRect'), r[0], r[1], r[2], r[3]);
    },
    /// 编译链接一个着色器程序；失败抛错并附日志。
    program: function (gl, vsSrc, fsSrc) {
      function sh(type, src) {
        var s = gl.createShader(type);
        gl.shaderSource(s, src);
        gl.compileShader(s);
        if (!gl.getShaderParameter(s, gl.COMPILE_STATUS)) {
          throw new Error('shader: ' + gl.getShaderInfoLog(s));
        }
        return s;
      }
      var p = gl.createProgram();
      gl.attachShader(p, sh(gl.VERTEX_SHADER, vsSrc));
      gl.attachShader(p, sh(gl.FRAGMENT_SHADER, fsSrc));
      gl.linkProgram(p);
      if (!gl.getProgramParameter(p, gl.LINK_STATUS)) {
        throw new Error('link: ' + gl.getProgramInfoLog(p));
      }
      return p;
    },
    /// 绑定一个覆盖全屏的三角形 VAO（clip 空间 [-1,3]），供全屏 fragment 特效使用。
    fullscreenTriangle: function (gl) {
      var vao = gl.createVertexArray();
      gl.bindVertexArray(vao);
      var buf = gl.createBuffer();
      gl.bindBuffer(gl.ARRAY_BUFFER, buf);
      gl.bufferData(gl.ARRAY_BUFFER, new Float32Array([-1, -1, 3, -1, -1, 3]), gl.STATIC_DRAW);
      gl.enableVertexAttribArray(0);
      gl.vertexAttribPointer(0, 2, gl.FLOAT, false, 0, 0);
      gl.bindVertexArray(null);
      return vao;
    },
  };

  // 后期处理：特效渲到离屏纹理 → 提亮泛光(bloom) + 颗粒 + 暗角 + 边缘色散 → 屏幕。
  // 一道共享管线，四个特效整体质感统一跃升。bloom 在 1/4 分辨率做，开销小。
  function createPost(gl) {
    var BRIGHT_FS =
      '#version 300 es\nprecision highp float;\n' +
      'uniform sampler2D u_tex; uniform vec2 u_res; out vec4 frag;\n' +
      'void main(){vec2 uv=gl_FragCoord.xy/u_res;vec3 c=texture(u_tex,uv).rgb;\n' +
      ' float l=max(c.r,max(c.g,c.b));frag=vec4(c*smoothstep(0.45,0.85,l),1.0);}';
    var BLUR_FS =
      '#version 300 es\nprecision highp float;\n' +
      'uniform sampler2D u_tex; uniform vec2 u_res; uniform vec2 u_dir; out vec4 frag;\n' +
      'void main(){vec2 uv=gl_FragCoord.xy/u_res;vec2 o=u_dir/u_res;\n' +
      ' vec3 s=texture(u_tex,uv).rgb*0.227027;\n' +
      ' s+=(texture(u_tex,uv+o).rgb+texture(u_tex,uv-o).rgb)*0.1945946;\n' +
      ' s+=(texture(u_tex,uv+o*2.0).rgb+texture(u_tex,uv-o*2.0).rgb)*0.1216216;\n' +
      ' s+=(texture(u_tex,uv+o*3.0).rgb+texture(u_tex,uv-o*3.0).rgb)*0.054054;\n' +
      ' s+=(texture(u_tex,uv+o*4.0).rgb+texture(u_tex,uv-o*4.0).rgb)*0.016216;\n' +
      ' frag=vec4(s,1.0);}';
    var COMP_FS =
      '#version 300 es\nprecision highp float;\n' +
      'uniform sampler2D u_scene; uniform sampler2D u_bloom; uniform vec2 u_res; uniform float u_time; out vec4 frag;\n' +
      'float hash(vec2 p){return fract(sin(dot(p,vec2(12.9898,78.233)))*43758.5453);}\n' +
      'void main(){vec2 uv=gl_FragCoord.xy/u_res;vec2 d=uv-0.5;\n' +
      ' float ca=dot(d,d)*0.006;\n' + // 边缘色散
      ' vec3 col;col.r=texture(u_scene,uv-d*ca).r;col.g=texture(u_scene,uv).g;col.b=texture(u_scene,uv+d*ca).b;\n' +
      ' col+=texture(u_bloom,uv).rgb*0.85;\n' + // 叠加泛光
      ' col*=smoothstep(1.15,0.35,length(d)*1.35);\n' + // 暗角
      ' col+=(hash(gl_FragCoord.xy+fract(u_time)*vec2(37.0,17.0))-0.5)*0.035;\n' + // 颗粒
      ' frag=vec4(col,1.0);}';

    var bright = util.program(gl, util.FS_VS, BRIGHT_FS);
    var blur = util.program(gl, util.FS_VS, BLUR_FS);
    var comp = util.program(gl, util.FS_VS, COMP_FS);
    var quad = util.fullscreenTriangle(gl);
    gl.bindBuffer(gl.ARRAY_BUFFER, null);

    var scene = null,
      b1 = null,
      b2 = null;

    function makeTarget(w, h) {
      var tex = gl.createTexture();
      gl.bindTexture(gl.TEXTURE_2D, tex);
      gl.texImage2D(gl.TEXTURE_2D, 0, gl.RGBA8, w, h, 0, gl.RGBA, gl.UNSIGNED_BYTE, null);
      gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MIN_FILTER, gl.LINEAR);
      gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MAG_FILTER, gl.LINEAR);
      gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_WRAP_S, gl.CLAMP_TO_EDGE);
      gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_WRAP_T, gl.CLAMP_TO_EDGE);
      var fbo = gl.createFramebuffer();
      gl.bindFramebuffer(gl.FRAMEBUFFER, fbo);
      gl.framebufferTexture2D(gl.FRAMEBUFFER, gl.COLOR_ATTACHMENT0, gl.TEXTURE_2D, tex, 0);
      gl.bindFramebuffer(gl.FRAMEBUFFER, null);
      gl.bindTexture(gl.TEXTURE_2D, null);
      return { tex: tex, fbo: fbo, w: w, h: h };
    }
    function free(tgt) {
      if (!tgt) return;
      gl.deleteTexture(tgt.tex);
      gl.deleteFramebuffer(tgt.fbo);
    }
    function pass(prog, target) {
      gl.bindFramebuffer(gl.FRAMEBUFFER, target ? target.fbo : null);
      var w = target ? target.w : scene.w,
        h = target ? target.h : scene.h;
      gl.viewport(0, 0, w, h);
      gl.useProgram(prog);
      gl.uniform2f(gl.getUniformLocation(prog, 'u_res'), w, h);
    }

    return {
      resize: function (w, h) {
        free(scene);
        free(b1);
        free(b2);
        scene = makeTarget(w, h);
        var bw = Math.max(1, w >> 2),
          bh = Math.max(1, h >> 2);
        b1 = makeTarget(bw, bh);
        b2 = makeTarget(bw, bh);
      },
      begin: function () {
        gl.bindFramebuffer(gl.FRAMEBUFFER, scene.fbo);
        gl.viewport(0, 0, scene.w, scene.h);
      },
      end: function (t) {
        gl.disable(gl.BLEND);
        gl.bindVertexArray(quad);
        // 提亮：scene → b1
        pass(bright, b1);
        gl.activeTexture(gl.TEXTURE0);
        gl.bindTexture(gl.TEXTURE_2D, scene.tex);
        gl.uniform1i(gl.getUniformLocation(bright, 'u_tex'), 0);
        gl.drawArrays(gl.TRIANGLES, 0, 3);
        // 高斯模糊：横(b1→b2) + 纵(b2→b1)，两轮
        for (var i = 0; i < 2; i++) {
          pass(blur, b2);
          gl.bindTexture(gl.TEXTURE_2D, b1.tex);
          gl.uniform1i(gl.getUniformLocation(blur, 'u_tex'), 0);
          gl.uniform2f(gl.getUniformLocation(blur, 'u_dir'), 1.5, 0.0);
          gl.drawArrays(gl.TRIANGLES, 0, 3);
          pass(blur, b1);
          gl.bindTexture(gl.TEXTURE_2D, b2.tex);
          gl.uniform1i(gl.getUniformLocation(blur, 'u_tex'), 0);
          gl.uniform2f(gl.getUniformLocation(blur, 'u_dir'), 0.0, 1.5);
          gl.drawArrays(gl.TRIANGLES, 0, 3);
        }
        // 合成到屏幕
        pass(comp, null);
        gl.activeTexture(gl.TEXTURE0);
        gl.bindTexture(gl.TEXTURE_2D, scene.tex);
        gl.uniform1i(gl.getUniformLocation(comp, 'u_scene'), 0);
        gl.activeTexture(gl.TEXTURE1);
        gl.bindTexture(gl.TEXTURE_2D, b1.tex);
        gl.uniform1i(gl.getUniformLocation(comp, 'u_bloom'), 1);
        gl.uniform1f(gl.getUniformLocation(comp, 'u_time'), t);
        gl.drawArrays(gl.TRIANGLES, 0, 3);
        gl.activeTexture(gl.TEXTURE0);
        gl.bindVertexArray(null);
      },
    };
  }

  window.XuanjiFx = {
    register: function (name, factory) {
      registry[name] = factory;
    },
    set: set,
    has: function (name) {
      return !!registry[name];
    },
    util: util,
    seal: seal, // 共享圣号资源：各特效在笔画处生长出圣名
    mouse: mouse, // 特效读 mouse.x / mouse.y / mouse.influence（每帧更新）
    accent: accent, // 今日五行主色 [r,g,b]（稳定引用，内容随日期更新）
    audio: audio, // 音频律动 {level,bass,mid,treble}（0..1，每拍更新）
    setAudio: setAudio, // 壳侧频谱注入入口
    /// 壳侧全局鼠标监听经 IPC 调用：喂入该屏归一化坐标并标记「刚移动过」。
    pointer: function (nx, ny) {
      pTarget.x = nx;
      pTarget.y = ny;
      pInf = 1;
    },
    /// 壳侧菜单栏切换调用：持久选定某特效（空/假值 = 回原生背景）。
    select: function (name) {
      forced = name || null;
      set(forced);
    },
  };

  // 接线：URL ?fx= 强制预览优先；否则跟随 bgtype。包裹 applyCustomBackground，
  // 使异步的 applyUserProperties 再次触发时也能重新压制（如去掉 no-glass-blur）。
  window.addEventListener('load', function () {
    var params = new URLSearchParams(location.search);
    forced = params.get('fx') || null;
    DEBUG = params.has('fxdebug'); // 需要排查时 URL 加 &fxdebug=1
    autoMouse = params.get('fxmouse') === 'auto'; // 自测：指针自动画圈
    updateElement();
    setInterval(updateElement, 30000); // 跨零点自动换今日五行色，无需重启
    enhanceClock();
    enhanceBento();
    enhanceBazi();
    setTimeout(function () {
      enhanceClock();
      enhanceBento();
      enhanceBazi();
      enhanceProgress();
      enhanceSihua();
    }, 800); // 时钟/布局/八字若晚于此刻才填好，兜底补上
    setInterval(enhanceProgress, 60000); // 进度环随时间更新
    setInterval(updateShichen, 60000); // 每分钟校准当前时辰高亮

    // 轮播：?fx=cycle 每 12s 自动切下一个特效，零权限、无需重启，供快速过目全部。
    if (forced === 'cycle') {
      var names = Object.keys(registry);
      var ci = 0;
      set(names[0]);
      setInterval(function () {
        ci = (ci + 1) % names.length;
        set(names[ci]);
      }, 12000);
      return;
    }

    var orig = window.applyCustomBackground;
    if (typeof orig === 'function') {
      window.applyCustomBackground = function () {
        var r = orig.apply(this, arguments);
        set(pick());
        return r;
      };
    }
    set(pick());
  });
})();
