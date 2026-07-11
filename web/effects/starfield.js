// 星空北斗：fbm 星云 + 繁星闪烁 + 北斗七星连线，整体缓慢周天旋转。呼应「璇玑玉衡」。
(function () {
  'use strict';
  var U = window.XuanjiFx.util;

  var NEBULA_FS =
    '#version 300 es\nprecision highp float;\n' +
    'uniform vec2 u_res; uniform float u_time; uniform vec3 u_accent; uniform float u_audio; out vec4 frag;\n' +
    U.SEAL_GLSL +
    'float hash(vec2 p){p=fract(p*vec2(123.34,456.21));p+=dot(p,p+34.5);return fract(p.x*p.y);}\n' +
    'float noise(vec2 p){vec2 i=floor(p),f=fract(p);float a=hash(i),b=hash(i+vec2(1,0)),c=hash(i+vec2(0,1)),d=hash(i+vec2(1,1));vec2 u=f*f*(3.0-2.0*f);return mix(mix(a,b,u.x),mix(c,d,u.x),u.y);}\n' +
    'float fbm(vec2 p){float v=0.0,a=0.5;for(int i=0;i<6;i++){v+=a*noise(p);p*=2.0;a*=0.5;}return v;}\n' +
    // 流星：按周期偶发，沿斜线掠过，头亮尾淡
    'float meteor(vec2 p,float ut,float sd){float ph=fract(ut*0.05+sd);float act=1.0-smoothstep(0.0,0.12,ph);\n' +
    ' vec2 dir=normalize(vec2(-1.0,-0.35-0.3*fract(sd*7.0)));\n' +
    ' vec2 st=vec2(0.7+0.5*fract(sd*3.0),0.55);vec2 pos=st+dir*(ph/0.12)*1.7;\n' +
    ' vec2 tail=pos-dir*0.2;vec2 pa=p-tail,ba=pos-tail;float h=clamp(dot(pa,ba)/dot(ba,ba),0.0,1.0);\n' +
    ' float d=length(pa-ba*h);return exp(-d*d*1100.0)*h*act;}\n' +
    'uniform vec2 u_par;\n' +
    'void main(){vec2 p=(gl_FragCoord.xy-0.5*u_res)/u_res.y - u_par*0.5;float t=u_time*0.02;\n' +
    ' float n=fbm(p*3.0+vec2(t,t*0.5));float n2=fbm(p*6.0-vec2(t*0.3,t));\n' +
    ' vec3 base=vec3(0.02,0.03,0.06);vec3 neb=mix(vec3(0.16,0.05,0.36),vec3(0.0,0.34,0.45),n2);\n' +
    ' float dens=smoothstep(0.42,0.92,n)*(0.5+0.5*n2);\n' +
    ' vec3 col=base+neb*dens*0.6;\n' +
    // 银河带：一道斜向的密星辉光带
    ' float band=exp(-pow((dot(p,vec2(0.45,1.0))+0.05)/0.34,2.0));\n' +
    ' col+=vec3(0.5,0.58,0.75)*band*(0.12+0.3*n)*0.5;\n' +
    // 流星偶发
    ' col+=vec3(0.9,0.95,1.0)*(meteor(p,u_time,0.13)+meteor(p,u_time,0.57))*1.3;\n' +
    // 圣号星云：笔画处漫起一层清亮辉光托住星宿；随时间轻漾(域扭曲)让字形呼吸
    ' vec2 swp=vec2(fbm(p*2.5+vec2(t*0.1,0.0)),fbm(p*2.5+vec2(3.0,t*0.08)))-0.5;\n' +
    ' float sc=sealCov(gl_FragCoord.xy+swp*u_res.y*0.010,u_res);\n' +
    ' col+=(u_accent*0.55+vec3(0.34,0.4,0.52))*sc*(0.62+0.12*fbm(p*5.0+vec2(t,t*0.4)));\n' + // 字先作一团柔光成形，托住星宿

    ' col*=smoothstep(1.25,0.2,length(p));\n' +
    ' col*=1.0+u_audio*1.2;\n' + // 随总能量提亮
    ' frag=vec4(col,1.0);}';

  var STAR_VS =
    '#version 300 es\n' +
    'layout(location=0) in vec2 a_pos; layout(location=1) in float a_size; layout(location=2) in float a_seed;\n' +
    'uniform float u_time; uniform float u_aspect; uniform float u_px; uniform vec2 u_par; uniform float u_bass; uniform float u_rot; uniform float u_jitter;\n' +
    'out float v_seed;\n' +
    'void main(){vec2 p=a_pos;p.x*=u_aspect;float a=u_time*0.01*u_rot;float c=cos(a),s=sin(a);\n' +
    ' p=mat2(c,-s,s,c)*p;p.x/=u_aspect;\n' +
    ' p+=u_jitter*0.004*vec2(sin(u_time*1.3+a_seed*30.0),cos(u_time*1.1+a_seed*20.0));\n' + // 圣号星各自微漂：字形活起来
    ' p+=u_par;gl_Position=vec4(p,0.0,1.0);\n' +
    ' gl_PointSize=a_size*u_px*(1.0+u_bass*2.2);v_seed=a_seed;}'; // 低频胀星

  var STAR_FS =
    '#version 300 es\nprecision highp float;\n' +
    'in float v_seed; uniform float u_time; uniform float u_bright; uniform vec3 u_accent; uniform float u_bass;\n' +
    'uniform vec3 u_starcol; uniform float u_starmix; out vec4 frag;\n' + // u_starmix>0：统一染成 u_starcol（圣号星宿用）
    'void main(){vec2 d=gl_PointCoord-0.5;float r=length(d);if(r>0.5)discard;\n' +
    ' float glow=smoothstep(0.5,0.0,r);float tw=0.55+0.45*sin(u_time*2.0+v_seed*6.283);\n' +
    ' vec3 col=mix(vec3(0.7,0.8,1.0),vec3(1.0,0.95,0.82),fract(v_seed*7.0));\n' +
    ' col=mix(col,u_starcol,u_starmix);\n' +
    ' frag=vec4(col*glow*tw*u_bright*(1.0+u_bass*1.7),1.0);}'; // 低频提亮

  var LINE_VS =
    '#version 300 es\nlayout(location=0) in vec2 a_pos;\n' +
    'uniform float u_time; uniform float u_aspect; uniform vec2 u_par;\n' +
    'void main(){vec2 p=a_pos;p.x*=u_aspect;float a=u_time*0.01;float c=cos(a),s=sin(a);\n' +
    ' p=mat2(c,-s,s,c)*p;p.x/=u_aspect;p+=u_par;gl_Position=vec4(p,0.0,1.0);}';

  var LINE_FS =
    '#version 300 es\nprecision highp float;\nuniform vec3 u_accent;out vec4 frag;\n' +
    'void main(){frag=vec4(vec3(0.45,0.62,0.85),1.0)*0.4;}';

  // 北斗七星（0=天枢 1=天璇 2=天玑 3=天权 4=玉衡 5=开阳 6=摇光）
  var DIPPER = [
    [-0.5, 0.55], [-0.52, 0.38], [-0.3, 0.33], [-0.28, 0.48],
    [-0.1, 0.5], [0.1, 0.46], [0.3, 0.38],
  ];
  var DIPPER_SEG = [0, 1, 1, 2, 2, 3, 3, 0, 3, 4, 4, 5, 5, 6]; // 斗魁四边 + 斗柄

  // 确定性伪随机，避免每帧/每次抖动。
  function rng(seed) {
    var s = seed;
    return function () {
      s = (s * 1103515245 + 12345) & 0x7fffffff;
      return s / 0x7fffffff;
    };
  }

  function buildStars(gl, n) {
    var r = rng(20260709);
    var pos = new Float32Array(n * 2);
    var size = new Float32Array(n);
    var seed = new Float32Array(n);
    for (var i = 0; i < n; i++) {
      pos[i * 2] = r() * 2 - 1;
      pos[i * 2 + 1] = r() * 2 - 1;
      size[i] = 1.0 + r() * r() * 2.5;
      seed[i] = r();
    }
    return attribVao(gl, pos, size, seed);
  }

  function attribVao(gl, pos, size, seed) {
    var vao = gl.createVertexArray();
    gl.bindVertexArray(vao);
    bindAttr(gl, 0, pos, 2);
    bindAttr(gl, 1, size, 1);
    bindAttr(gl, 2, seed, 1);
    gl.bindVertexArray(null);
    return vao;
  }

  function bindAttr(gl, loc, data, comps) {
    var b = gl.createBuffer();
    gl.bindBuffer(gl.ARRAY_BUFFER, b);
    gl.bufferData(gl.ARRAY_BUFFER, data, gl.STATIC_DRAW);
    gl.enableVertexAttribArray(loc);
    gl.vertexAttribPointer(loc, comps, gl.FLOAT, false, 0, 0);
  }

  window.XuanjiFx.register('starfield', function (gl, canvas) {
    var nebula = U.program(gl, fsQuadVs(), NEBULA_FS);
    var starProg = U.program(gl, STAR_VS, STAR_FS);
    var lineProg = U.program(gl, LINE_VS, LINE_FS);
    var quad = U.fullscreenTriangle(gl);

    var STAR_N = 1400;
    var stars = buildStars(gl, STAR_N);

    // 北斗：亮星 + 连线
    var dpos = new Float32Array(DIPPER.length * 2);
    var dsize = new Float32Array(DIPPER.length);
    var dseed = new Float32Array(DIPPER.length);
    for (var i = 0; i < DIPPER.length; i++) {
      dpos[i * 2] = DIPPER[i][0];
      dpos[i * 2 + 1] = DIPPER[i][1];
      dsize[i] = 6.0;
      dseed[i] = i / DIPPER.length;
    }
    var dipperStars = attribVao(gl, dpos, dsize, dseed);
    var lpos = new Float32Array(DIPPER_SEG.length * 2);
    for (var j = 0; j < DIPPER_SEG.length; j++) {
      lpos[j * 2] = DIPPER[DIPPER_SEG[j]][0];
      lpos[j * 2 + 1] = DIPPER[DIPPER_SEG[j]][1];
    }
    var lineVao = gl.createVertexArray();
    gl.bindVertexArray(lineVao);
    bindAttr(gl, 0, lpos, 2);
    gl.bindVertexArray(null);

    var w = canvas.width, h = canvas.height;
    var px = h / 900;

    function uni(p, n) {
      return gl.getUniformLocation(p, n);
    }

    // 圣号星宿：笔画点云 → 亮星（不随天旋，圣名正立）。位置依纵横比，尺寸变化时重建。
    var sealVao = null, sealCount = 0, sealKey = '';
    function buildSealStars() {
      var s = window.XuanjiFx.seal;
      if (!s.points || !s.points.length) return;
      var r = s.rect(w, h), x0 = r[0], y0 = r[1], wuv = r[2], huv = r[3];
      var m = s.points.length / 2;
      var pos = new Float32Array(m * 2), size = new Float32Array(m), seed = new Float32Array(m);
      for (var i = 0; i < m; i++) {
        var ux = x0 + s.points[i * 2] * wuv;
        var uy = y0 + (1.0 - s.points[i * 2 + 1]) * huv; // 图 y 向下 → uv 下-上
        pos[i * 2] = ux * 2 - 1;
        pos[i * 2 + 1] = uy * 2 - 1;
        size[i] = 0.9 + (((i * 2654435761) >>> 0) % 1000) / 1000 * 0.9; // 更小更匀，密集成笔画
        seed[i] = ((i * 40503) % 997) / 997;
      }
      if (sealVao) gl.deleteVertexArray(sealVao);
      sealVao = attribVao(gl, pos, size, seed);
      sealCount = m;
      sealKey = w + 'x' + h;
    }

    return {
      resize: function (nw, nh) {
        w = nw;
        h = nh;
        px = h / 900;
      },
      frame: function (t) {
        // 每帧清 scene：nebula 未铺满 + 星点加色叠加，不清会累积成放射拖尾
        // （thunder/flowfield/ink 各自全屏覆盖 scene，无需此步）。
        gl.clearColor(0, 0, 0, 1);
        gl.clear(gl.COLOR_BUFFER_BIT);
        var aspect = w / h;
        // 视差：星层随光标反向微移（跟随平滑坐标，停手保持不回弹）。
        var mo = window.XuanjiFx.mouse;
        var acc = window.XuanjiFx.accent;
        var au = window.XuanjiFx.audio;
        var parx = -(mo.x - 0.5),
          pary = -(mo.y - 0.5);
        gl.disable(gl.BLEND);
        gl.useProgram(nebula);
        gl.uniform2f(uni(nebula, 'u_res'), w, h);
        gl.uniform1f(uni(nebula, 'u_time'), t);
        gl.uniform2f(uni(nebula, 'u_par'), parx * 0.06, pary * 0.06); // 星云移得少
        gl.uniform3fv(uni(nebula, 'u_accent'), acc);
        gl.uniform1f(uni(nebula, 'u_audio'), au.level);
        U.bindSeal(gl, nebula, w, h, 1);
        gl.bindVertexArray(quad);
        gl.drawArrays(gl.TRIANGLES, 0, 3);

        gl.enable(gl.BLEND);
        gl.blendFunc(gl.SRC_ALPHA, gl.ONE); // 加色混合，星点叠加发光

        gl.useProgram(lineProg);
        gl.uniform1f(uni(lineProg, 'u_time'), t);
        gl.uniform1f(uni(lineProg, 'u_aspect'), aspect);
        gl.uniform2f(uni(lineProg, 'u_par'), parx * 0.1, pary * 0.1);
        gl.uniform3fv(uni(lineProg, 'u_accent'), acc);
        gl.bindVertexArray(lineVao);
        gl.drawArrays(gl.LINES, 0, DIPPER_SEG.length);

        gl.useProgram(starProg);
        gl.uniform1f(uni(starProg, 'u_time'), t);
        gl.uniform1f(uni(starProg, 'u_aspect'), aspect);
        gl.uniform1f(uni(starProg, 'u_px'), px);
        gl.uniform2f(uni(starProg, 'u_par'), parx * 0.1, pary * 0.1); // 星点移得多
        gl.uniform3fv(uni(starProg, 'u_accent'), acc);
        gl.uniform1f(uni(starProg, 'u_bass'), au.bass);
        gl.uniform1f(uni(starProg, 'u_rot'), 1.0); // 繁星 + 北斗随天缓旋
        gl.uniform1f(uni(starProg, 'u_jitter'), 0.0);
        gl.uniform1f(uni(starProg, 'u_starmix'), 0.0); // 繁星保留冷暖变化
        gl.uniform1f(uni(starProg, 'u_bright'), 1.0);
        gl.bindVertexArray(stars);
        gl.drawArrays(gl.POINTS, 0, STAR_N);

        gl.uniform1f(uni(starProg, 'u_bright'), 2.2); // 北斗更亮
        gl.bindVertexArray(dipperStars);
        gl.drawArrays(gl.POINTS, 0, DIPPER.length);

        // 圣号星宿：星光连缀成圣名，正立不随天旋，统一染成清亮五行色
        var sl = window.XuanjiFx.seal;
        if (sl.ready && sl.show && sl.points && sl.points.length) {
          if (sealKey !== w + 'x' + h) buildSealStars();
          if (sealVao) {
            gl.uniform2f(uni(starProg, 'u_par'), 0.0, 0.0); // 圣号固定，不随视差平移（消除分层错位）
            gl.uniform1f(uni(starProg, 'u_rot'), 0.0);
            gl.uniform1f(uni(starProg, 'u_jitter'), 1.0); // 星子各自微漂，字形活起来
            gl.uniform3f(uni(starProg, 'u_starcol'),
              Math.min(1, acc[0] * 0.5 + 0.55), Math.min(1, acc[1] * 0.5 + 0.6), Math.min(1, acc[2] * 0.5 + 0.7));
            gl.uniform1f(uni(starProg, 'u_starmix'), 1.0); // 纯净五行色星子，去彩虹杂点
            gl.uniform1f(uni(starProg, 'u_bright'), 1.35);
            gl.bindVertexArray(sealVao);
            gl.drawArrays(gl.POINTS, 0, sealCount);
          }
        }
      },
      dispose: function () {
        [nebula, starProg, lineProg].forEach(function (p) {
          gl.deleteProgram(p);
        });
        if (sealVao) gl.deleteVertexArray(sealVao);
      },
    };
  });

  // 全屏三角形顶点着色器（配合 util.fullscreenTriangle）。
  function fsQuadVs() {
    return (
      '#version 300 es\nlayout(location=0) in vec2 a_pos;\n' +
      'void main(){gl_Position=vec4(a_pos,0.0,1.0);}'
    );
  }
})();
