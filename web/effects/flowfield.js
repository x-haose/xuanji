// 流场星尘：数千 GPU 粒子沿曲线噪声场(curl noise)漂流，如「气」的涌动 / 星尘飘移。
// 用 transform feedback 在 GPU 上迭代粒子状态，ping-pong 两组缓冲。
(function () {
  'use strict';

  var N = 2200;
  var NOISE =
    'float hash(vec2 p){p=fract(p*vec2(123.34,456.21));p+=dot(p,p+34.5);return fract(p.x*p.y);}\n' +
    'float noise(vec2 p){vec2 i=floor(p),f=fract(p);float a=hash(i),b=hash(i+vec2(1,0)),c=hash(i+vec2(0,1)),d=hash(i+vec2(1,1));vec2 u=f*f*(3.0-2.0*f);return mix(mix(a,b,u.x),mix(c,d,u.x),u.y);}\n' +
    'float fbm(vec2 p){float v=0.0,a=0.5;for(int i=0;i<4;i++){v+=a*noise(p);p*=2.0;a*=0.5;}return v;}\n';

  var UPDATE_VS =
    '#version 300 es\n' +
    'layout(location=0) in vec2 a_pos; layout(location=1) in float a_age;\n' +
    'uniform float u_time; uniform float u_dt; uniform vec2 u_mouse; uniform float u_minf; uniform float u_bass;\n' +
    'uniform sampler2D u_seal; uniform vec4 u_sealRect; uniform float u_sealOn;\n' +
    'out vec2 v_pos; out float v_age;\n' +
    NOISE +
    'float h1(float n){return fract(sin(n)*43758.5453123);}\n' +
    'float covAt(vec2 uv){ if(u_sealOn<0.5) return 0.0;\n' +
    ' vec2 s=(uv-u_sealRect.xy)/u_sealRect.zw;\n' +
    ' if(s.x<0.0||s.x>1.0||s.y<0.0||s.y>1.0) return 0.0;\n' +
    ' return texture(u_seal, vec2(s.x,1.0-s.y)).a; }\n' +
    'float pot(vec2 p){return fbm(p*1.6+vec2(u_time*0.05,0.0));}\n' +
    'vec2 curl(vec2 p){float e=0.01;\n' +
    ' float dx=pot(p+vec2(e,0.0))-pot(p-vec2(e,0.0));\n' +
    ' float dy=pot(p+vec2(0.0,e))-pot(p-vec2(0.0,e));\n' +
    ' return vec2(dy,-dx)/(2.0*e);}\n' +
    'void main(){\n' +
    ' float cC=covAt(a_pos*0.5+0.5);\n' + // 当前是否落在笔画上
    ' vec2 np=a_pos+curl(a_pos)*(0.45+u_bass*1.1)*(1.0-cC*0.32)*u_dt;\n' + // 笔画内轻微减速（尘流穿过，不堆死）
    ' if(u_sealOn>0.5){vec2 uv=np*0.5+0.5;float e=0.014;\n' + // 温和吸向笔画：勾勒字形而不过度堆积
    '  vec2 g=vec2(covAt(uv+vec2(e,0.0))-covAt(uv-vec2(e,0.0)), covAt(uv+vec2(0.0,e))-covAt(uv-vec2(0.0,e)));\n' +
    '  np+=g*0.012;}\n' +
    // 鼠标扰动：绕光标旋流 + 轻微吸引，强度随 influence 与距离衰减
    ' vec2 tom=u_mouse-a_pos;float dm=length(tom);\n' +
    ' np+=(vec2(-tom.y,tom.x)*3.0+tom*0.3)*exp(-dm*2.2)*u_minf*u_dt;\n' +
    ' float na=a_age+u_dt;\n' +
    ' float life=5.0+4.0*h1(float(gl_VertexID)*0.017);\n' +
    ' if(na>life||abs(np.x)>1.3||abs(np.y)>1.3){\n' +
    '  float s=float(gl_VertexID)*0.017+fract(u_time)*7.0;\n' +
    '  np=vec2(h1(s)*2.0-1.0,h1(s+3.7)*2.0-1.0);na=0.0;}\n' +
    ' v_pos=np;v_age=na;}';

  var UPDATE_FS = '#version 300 es\nprecision highp float;\nvoid main(){}';

  // 速度上色：流场是位置的确定函数，render 时在粒子位置重算 curl 即得其漂移速率，
  // 无需把速度存进 transform feedback（省一整套 buffer 改造）。pot/curl 与 UPDATE_VS 完全一致。
  var RENDER_VS =
    '#version 300 es\n' +
    'layout(location=0) in vec2 a_pos; layout(location=1) in float a_age;\n' +
    'uniform float u_px; uniform float u_time;\n' +
    'out float v_age; out float v_speed;\n' +
    NOISE +
    'float pot(vec2 p){return fbm(p*1.6+vec2(u_time*0.05,0.0));}\n' +
    'vec2 curl(vec2 p){float e=0.01;\n' +
    ' float dx=pot(p+vec2(e,0.0))-pot(p-vec2(e,0.0));\n' +
    ' float dy=pot(p+vec2(0.0,e))-pot(p-vec2(0.0,e));\n' +
    ' return vec2(dy,-dx)/(2.0*e);}\n' +
    'void main(){gl_Position=vec4(a_pos,0.0,1.0);\n' +
    ' gl_PointSize=(1.2+1.8*fract(a_age*0.37))*u_px;\n' +
    ' v_speed=clamp(length(curl(a_pos))*0.06,0.0,1.0);\n' + // 流场速率归一化到[0,1]
    ' v_age=a_age;}';

  var RENDER_FS =
    '#version 300 es\nprecision highp float;\n' +
    'in float v_age; in float v_speed; uniform vec3 u_accent; uniform float u_bass; out vec4 frag;\n' +
    'void main(){vec2 d=gl_PointCoord-0.5;float r=length(d);if(r>0.5)discard;\n' +
    ' float glow=smoothstep(0.5,0.0,r);\n' +
    ' float fade=smoothstep(0.0,0.8,v_age)*(1.0-smoothstep(6.0,9.0,v_age));\n' +
    ' float sp=v_speed;\n' +
    ' vec3 col=mix(vec3(0.34,0.46,0.74),vec3(0.96,0.98,1.0),sp);\n' + // 缓流暗靛(可见) → 疾流亮白
    ' col=mix(col,u_accent,sp*sp*0.4);\n' + // 疾流染当日五行主色
    ' float bright=0.42+sp*0.55;\n' + // 缓流≈原版亮度，疾流显著更亮，对比出层次
    ' frag=vec4(col*glow*fade*bright*(1.0+u_bass*2.6),1.0);}';

  function compile(gl, type, src) {
    var s = gl.createShader(type);
    gl.shaderSource(s, src);
    gl.compileShader(s);
    if (!gl.getShaderParameter(s, gl.COMPILE_STATUS)) {
      throw new Error('shader: ' + gl.getShaderInfoLog(s));
    }
    return s;
  }

  function program(gl, vs, fs, varyings) {
    var p = gl.createProgram();
    gl.attachShader(p, compile(gl, gl.VERTEX_SHADER, vs));
    gl.attachShader(p, compile(gl, gl.FRAGMENT_SHADER, fs));
    if (varyings) gl.transformFeedbackVaryings(p, varyings, gl.INTERLEAVED_ATTRIBS);
    gl.linkProgram(p);
    if (!gl.getProgramParameter(p, gl.LINK_STATUS)) {
      throw new Error('link: ' + gl.getProgramInfoLog(p));
    }
    return p;
  }

  window.XuanjiFx.register('flowfield', function (gl, canvas) {
    var updateProg = program(gl, UPDATE_VS, UPDATE_FS, ['v_pos', 'v_age']);
    var renderProg = program(gl, RENDER_VS, RENDER_FS);

    var init = new Float32Array(N * 3);
    for (var i = 0; i < N; i++) {
      init[i * 3] = Math.random() * 2 - 1;
      init[i * 3 + 1] = Math.random() * 2 - 1;
      init[i * 3 + 2] = Math.random() * 8;
    }
    function makeBuf(data) {
      var b = gl.createBuffer();
      gl.bindBuffer(gl.ARRAY_BUFFER, b);
      gl.bufferData(gl.ARRAY_BUFFER, data, gl.DYNAMIC_COPY);
      return b;
    }
    function makeVao(buf) {
      var v = gl.createVertexArray();
      gl.bindVertexArray(v);
      gl.bindBuffer(gl.ARRAY_BUFFER, buf);
      gl.enableVertexAttribArray(0);
      gl.vertexAttribPointer(0, 2, gl.FLOAT, false, 12, 0);
      gl.enableVertexAttribArray(1);
      gl.vertexAttribPointer(1, 1, gl.FLOAT, false, 12, 8);
      gl.bindVertexArray(null);
      return v;
    }
    var bufA = makeBuf(init);
    var bufB = makeBuf(N * 3 * 4);
    var vaoA = makeVao(bufA);
    var vaoB = makeVao(bufB);
    gl.bindBuffer(gl.ARRAY_BUFFER, null); // 关键：解绑，否则与 TF 绑定点冲突(INVALID_OPERATION)
    var tf = gl.createTransformFeedback();
    var read = { buf: bufA, vao: vaoA };
    var write = { buf: bufB, vao: vaoB };

    // 拖尾：粒子渲进持久缓冲，每帧只淡出一点，画出丝缕流线。
    var U = window.XuanjiFx.util;
    var FADE_FS =
      '#version 300 es\nprecision highp float;\nuniform vec4 u_color;out vec4 frag;\n' +
      'void main(){frag=u_color;}';
    var BLIT_FS =
      '#version 300 es\nprecision highp float;\nuniform sampler2D u_tex;uniform vec2 u_res;out vec4 frag;\n' +
      'void main(){frag=vec4(texture(u_tex,gl_FragCoord.xy/u_res).rgb,1.0);}';
    // 光标气眼：发光核心 + 柔晕，随 influence 亮起，粒子绕其旋 → 交互可见的锚点。
    var GLOW_FS =
      '#version 300 es\nprecision highp float;\n' +
      'uniform vec2 u_res;uniform vec2 u_mouse;uniform float u_minf;uniform vec3 u_accent;out vec4 frag;\n' +
      'void main(){vec2 d=gl_FragCoord.xy/u_res-u_mouse;d.x*=u_res.x/u_res.y;float r=length(d);\n' +
      ' float g=exp(-r*r*1400.0)*0.5+exp(-r*r*160.0)*0.07;\n' +
      ' frag=vec4(vec3(0.6,0.8,1.0)*g*u_minf,1.0);}';
    // 圣号显影：星尘拖尾流经笔画处提亮，圣名随尘流明灭浮现
    var SEAL_FS =
      '#version 300 es\nprecision highp float;\n' +
      'uniform sampler2D u_trail; uniform vec2 u_res; uniform vec3 u_accent; uniform float u_bass;\n' +
      U.SEAL_GLSL +
      'out vec4 frag;\n' +
      'void main(){ float sc=sealCov(gl_FragCoord.xy,u_res);\n' +
      ' if(sc<0.001){frag=vec4(0.0);return;}\n' +
      ' vec3 tr=texture(u_trail,gl_FragCoord.xy/u_res).rgb; float lum=max(tr.r,max(tr.g,tr.b));\n' +
      ' float g=sc*(0.32+0.55*min(lum,0.45))*(1.0+u_bass*1.1);\n' + // 主要吃均匀底(每字亮度匀) + 尘流处轻提亮(峰值封顶，不爆白团)
      ' frag=vec4((u_accent*0.6+vec3(0.4,0.55,0.9))*g,1.0);}';
    var fadeProg = program(gl, U.FS_VS, FADE_FS);
    var blitProg = program(gl, U.FS_VS, BLIT_FS);
    var glowProg = program(gl, U.FS_VS, GLOW_FS);
    var sealProg = program(gl, U.FS_VS, SEAL_FS);
    var quad = U.fullscreenTriangle(gl);
    gl.bindBuffer(gl.ARRAY_BUFFER, null);
    var gRes = gl.getUniformLocation(glowProg, 'u_res');
    var gMouse = gl.getUniformLocation(glowProg, 'u_mouse');
    var gMinf = gl.getUniformLocation(glowProg, 'u_minf');
    var gAccent = gl.getUniformLocation(glowProg, 'u_accent');
    var rAccent = gl.getUniformLocation(renderProg, 'u_accent');
    var rBass = gl.getUniformLocation(renderProg, 'u_bass');
    var rTime = gl.getUniformLocation(renderProg, 'u_time');

    var uTime = gl.getUniformLocation(updateProg, 'u_time');
    var uDt = gl.getUniformLocation(updateProg, 'u_dt');
    var uMouse = gl.getUniformLocation(updateProg, 'u_mouse');
    var uMinf = gl.getUniformLocation(updateProg, 'u_minf');
    var uBassU = gl.getUniformLocation(updateProg, 'u_bass');
    var rPx = gl.getUniformLocation(renderProg, 'u_px');
    var uFade = gl.getUniformLocation(fadeProg, 'u_color');
    var uBlitTex = gl.getUniformLocation(blitProg, 'u_tex');
    var uBlitRes = gl.getUniformLocation(blitProg, 'u_res');
    var sTrail = gl.getUniformLocation(sealProg, 'u_trail');
    var sRes = gl.getUniformLocation(sealProg, 'u_res');
    var sAccent = gl.getUniformLocation(sealProg, 'u_accent');
    var sBass = gl.getUniformLocation(sealProg, 'u_bass');

    var trail = null;
    function makeTrail(w, h) {
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
      gl.clearColor(0.02, 0.03, 0.06, 1.0);
      gl.clear(gl.COLOR_BUFFER_BIT);
      gl.bindFramebuffer(gl.FRAMEBUFFER, null);
      gl.bindTexture(gl.TEXTURE_2D, null);
      return { tex: tex, fbo: fbo, w: w, h: h };
    }
    trail = makeTrail(canvas.width, canvas.height);

    var px = canvas.height / 900;
    var diagDone = false;

    return {
      resize: function (nw, nh) {
        px = nh / 900;
        gl.deleteTexture(trail.tex);
        gl.deleteFramebuffer(trail.fbo);
        trail = makeTrail(nw, nh);
      },
      frame: function (t, dt) {
        var prevFbo = gl.getParameter(gl.FRAMEBUFFER_BINDING);

        // 迭代粒子状态：从 read 读、写入 write（关光栅化）
        gl.useProgram(updateProg);
        gl.uniform1f(uTime, t);
        gl.uniform1f(uDt, dt > 0 ? dt : 0.016);
        var m = window.XuanjiFx.mouse;
        gl.uniform2f(uMouse, m.x * 2.0 - 1.0, m.y * 2.0 - 1.0); // [0,1] → clip[-1,1]
        gl.uniform1f(uMinf, m.influence);
        gl.uniform1f(uBassU, window.XuanjiFx.audio.bass);
        U.bindSeal(gl, updateProg, canvas.width, canvas.height, 1);
        gl.bindVertexArray(read.vao);
        gl.bindTransformFeedback(gl.TRANSFORM_FEEDBACK, tf);
        gl.bindBufferBase(gl.TRANSFORM_FEEDBACK_BUFFER, 0, write.buf);
        gl.enable(gl.RASTERIZER_DISCARD);
        gl.beginTransformFeedback(gl.POINTS);
        gl.drawArrays(gl.POINTS, 0, N);
        gl.endTransformFeedback();
        gl.disable(gl.RASTERIZER_DISCARD);
        gl.bindBufferBase(gl.TRANSFORM_FEEDBACK_BUFFER, 0, null);
        gl.bindTransformFeedback(gl.TRANSFORM_FEEDBACK, null);

        // 拖尾缓冲：先淡出一层背景，再加色叠加粒子 → 留下流线
        gl.bindFramebuffer(gl.FRAMEBUFFER, trail.fbo);
        gl.viewport(0, 0, trail.w, trail.h);
        gl.enable(gl.BLEND);
        gl.blendFunc(gl.SRC_ALPHA, gl.ONE_MINUS_SRC_ALPHA);
        gl.useProgram(fadeProg);
        gl.uniform4f(uFade, 0.02, 0.03, 0.06, 0.055);
        gl.bindVertexArray(quad);
        gl.drawArrays(gl.TRIANGLES, 0, 3);
        gl.blendFunc(gl.SRC_ALPHA, gl.ONE);
        gl.useProgram(renderProg);
        gl.uniform1f(rPx, px);
        gl.uniform1f(rTime, t);
        gl.uniform3fv(rAccent, window.XuanjiFx.accent);
        gl.uniform1f(rBass, window.XuanjiFx.audio.bass);
        gl.bindVertexArray(write.vao);
        gl.drawArrays(gl.POINTS, 0, N);
        gl.disable(gl.BLEND);

        // 合成拖尾到场景（后期处理的 FBO，或默认帧缓冲）
        gl.bindFramebuffer(gl.FRAMEBUFFER, prevFbo);
        gl.viewport(0, 0, trail.w, trail.h);
        gl.useProgram(blitProg);
        gl.activeTexture(gl.TEXTURE0);
        gl.bindTexture(gl.TEXTURE_2D, trail.tex);
        gl.uniform1i(uBlitTex, 0);
        gl.uniform2f(uBlitRes, trail.w, trail.h);
        gl.bindVertexArray(quad);
        gl.drawArrays(gl.TRIANGLES, 0, 3);

        // 圣号显影：星尘拖尾流经笔画处提亮
        gl.enable(gl.BLEND);
        gl.blendFunc(gl.SRC_ALPHA, gl.ONE);
        gl.useProgram(sealProg);
        gl.uniform2f(sRes, trail.w, trail.h);
        gl.uniform3fv(sAccent, window.XuanjiFx.accent);
        gl.uniform1f(sBass, window.XuanjiFx.audio.bass);
        gl.activeTexture(gl.TEXTURE0);
        gl.bindTexture(gl.TEXTURE_2D, trail.tex);
        gl.uniform1i(sTrail, 0);
        U.bindSeal(gl, sealProg, trail.w, trail.h, 1);
        gl.bindVertexArray(quad);
        gl.drawArrays(gl.TRIANGLES, 0, 3);
        gl.disable(gl.BLEND);

        // 光标气眼（加色叠加到场景，随 influence 亮起）
        if (m.influence > 0.01) {
          gl.enable(gl.BLEND);
          gl.blendFunc(gl.SRC_ALPHA, gl.ONE);
          gl.useProgram(glowProg);
          gl.uniform2f(gRes, trail.w, trail.h);
          gl.uniform2f(gMouse, m.x, m.y);
          gl.uniform1f(gMinf, m.influence);
          gl.uniform3fv(gAccent, window.XuanjiFx.accent);
          gl.drawArrays(gl.TRIANGLES, 0, 3);
          gl.disable(gl.BLEND);
        }
        gl.bindVertexArray(null);

        var tmp = read;
        read = write;
        write = tmp;

        if (!diagDone) {
          diagDone = true;
          var err = gl.getError();
          if (err !== 0) console.error('[fx] flowfield glErr=' + err);
        }
      },
      dispose: function () {
        gl.deleteProgram(updateProg);
        gl.deleteProgram(renderProg);
        gl.deleteProgram(fadeProg);
        gl.deleteProgram(blitProg);
        gl.deleteProgram(glowProg);
        gl.deleteProgram(sealProg);
        gl.deleteTransformFeedback(tf);
        gl.deleteBuffer(bufA);
        gl.deleteBuffer(bufB);
        gl.deleteVertexArray(vaoA);
        gl.deleteVertexArray(vaoB);
        gl.deleteTexture(trail.tex);
        gl.deleteFramebuffer(trail.fbo);
      },
    };
  });
})();
