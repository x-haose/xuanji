// 水墨流体：染料场沿「曲线噪声速度 + 鼠标推力」半拉格朗日平流，缓慢衰减，
// 噪声墨源不断涌出。墨真的随流被推走/卷绕/堆积。RGBA8 ping-pong，无需浮点纹理。
(function () {
  'use strict';
  var U = window.XuanjiFx.util;

  var NOISE =
    'float hash(vec2 p){p=fract(p*vec2(123.34,456.21));p+=dot(p,p+34.5);return fract(p.x*p.y);}\n' +
    'float noise(vec2 p){vec2 i=floor(p),f=fract(p);float a=hash(i),b=hash(i+vec2(1,0)),c=hash(i+vec2(0,1)),d=hash(i+vec2(1,1));vec2 u=f*f*f*(f*(f*6.0-15.0)+10.0);return mix(mix(a,b,u.x),mix(c,d,u.x),u.y);}\n' + // 五次插值:梯度平滑,curl 不抖
    'float fbm(vec2 p){float v=0.0,a=0.5;for(int i=0;i<5;i++){v+=a*noise(p);p*=1.9;a*=0.5;}return v;}\n';

  // 平流 + 墨源 + 衰减：把上一帧染料沿速度场向后采样
  var SIM_FS =
    '#version 300 es\nprecision highp float;\n' +
    'uniform sampler2D u_dye; uniform vec2 u_res; uniform float u_time; uniform float u_dt;\n' +
    'uniform vec2 u_mouse; uniform float u_minf; uniform float u_audio; out vec4 frag;\n' +
    'uniform float u_amount; uniform float u_diffuse;\n' + // 专用参数：墨量 / 扩散(流速场强度)，默认皆 1
    U.SEAL_GLSL +
    NOISE +
    'float pot(vec2 p){return fbm(p*2.3+vec2(u_time*0.08,u_time*0.05));}\n' + // 势场适度翻涌保持流动
    'vec2 curl(vec2 p){float e=0.002;\n' +
    ' float dx=pot(p+vec2(e,0.0))-pot(p-vec2(e,0.0));\n' +
    ' float dy=pot(p+vec2(0.0,e))-pot(p-vec2(0.0,e));\n' +
    ' return vec2(dy,-dx)/(2.0*e);}\n' +
    'void main(){vec2 uv=gl_FragCoord.xy/u_res;\n' +
    ' vec2 vel=curl(uv)*0.9*0.0022*u_diffuse;\n' + // 固定每帧位移，温和不冲散；u_diffuse=扩散强度
    ' vec2 tom=u_mouse-uv;float dm2=dot(tom,tom);\n' +
    ' vel+=(vec2(-tom.y,tom.x)*1.6+tom*0.5)*exp(-dm2*26.0)*u_minf*0.004;\n' + // 鼠标搅动
    ' float d=texture(u_dye,uv-vel).r*0.99;\n' + // 固定步长向后平流 + 衰减(不受帧率抖动)
    ' float well=smoothstep(0.42-u_audio*0.12,0.74,fbm(uv*2.2-vec2(u_time*0.02,u_time*0.03)));\n' + // 音量越大墨涌越盛
    ' d=max(d,well*(0.85+u_audio*0.5)*u_amount);\n' + // u_amount=墨量

    ' d=max(d,exp(-dm2*140.0)*u_minf*0.9);\n' + // 鼠标注墨
    ' d=max(d,sealCov(gl_FragCoord.xy,u_res)*0.22);\n' + // 圣号周围少量流墨氛围（淡，任其被流场洇开），清晰笔画在渲染期叠加
    ' frag=vec4(clamp(d,0.0,1.0),0.0,0.0,1.0);}';

  // 染料 → 墨色（深底淡墨雾 + 焦墨 + 飞白 + 五行微染）
  var RENDER_FS =
    '#version 300 es\nprecision highp float;\n' +
    'uniform sampler2D u_dye; uniform vec2 u_res; uniform vec3 u_accent; uniform float u_time; uniform float u_contrast; out vec4 frag;\n' +
    NOISE +
    U.SEAL_GLSL +
    'void main(){vec2 uv=gl_FragCoord.xy/u_res;float d=texture(u_dye,uv).r;\n' +
    ' d=clamp((d-0.5)*u_contrast+0.5,0.0,1.0);\n' + // u_contrast=浓淡对比（绕中点拉伸，默认 1 不变）


    ' vec3 paper=vec3(0.022,0.022,0.028);vec3 wash=vec3(0.42,0.43,0.41);\n' + // 更黑的底 + 收暗的淡墨
    ' vec3 col=mix(paper,wash,smoothstep(0.08,0.62,d));\n' + // 墨显现，靠暗底+焦墨拉对比留白
    ' col=mix(col,vec3(0.01,0.01,0.014),smoothstep(0.8,1.0,d)*0.6);\n' + // 焦墨更浓
    ' col+=vec3(0.9,0.88,0.82)*smoothstep(0.4,0.55,d)*(1.0-smoothstep(0.55,0.75,d))*0.18;\n' + // 飞白收弱
    ' vec2 p=(uv-0.5)*vec2(u_res.x/u_res.y,1.0);col*=smoothstep(0.95,0.2,length(p));\n' +
    // 圣号在暗角之上叠加（否则左缘暗角把它压暗看不清）：清晰浓墨 + 轻微发光触发泛光 + 随墨流荡漾
    ' vec2 swp=vec2(noise(uv*7.0+vec2(u_time*0.25,0.0)),noise(uv*7.0+vec2(5.0,u_time*0.2)))-0.5;\n' +
    ' float sc=sealCov(gl_FragCoord.xy+swp*u_res.y*0.013,u_res);\n' +
    ' col=mix(col,vec3(0.6,0.6,0.57),clamp(sc,0.0,1.0));\n' + // 柔和墨灰：读得清但不炸 bloom/色散，保留水墨味
    ' frag=vec4(col,1.0);}';

  window.XuanjiFx.register('ink', function (gl, canvas) {
    var simProg = U.program(gl, U.FS_VS, SIM_FS);
    var renderProg = U.program(gl, U.FS_VS, RENDER_FS);
    var quad = U.fullscreenTriangle(gl);
    gl.bindBuffer(gl.ARRAY_BUFFER, null);

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
      gl.clearColor(0.0, 0.0, 0.0, 1.0);
      gl.clear(gl.COLOR_BUFFER_BIT);
      gl.bindFramebuffer(gl.FRAMEBUFFER, null);
      gl.bindTexture(gl.TEXTURE_2D, null);
      return { tex: tex, fbo: fbo, w: w, h: h };
    }
    // 全分辨率模拟（更细腻）
    function simSize() {
      return [Math.max(1, canvas.width), Math.max(1, canvas.height)];
    }
    var sz = simSize();
    var a = makeTarget(sz[0], sz[1]);
    var b = makeTarget(sz[0], sz[1]);
    var read = a,
      write = b;

    var uni = function (p, n) {
      return gl.getUniformLocation(p, n);
    };

    return {
      resize: function () {
        gl.deleteTexture(a.tex);
        gl.deleteFramebuffer(a.fbo);
        gl.deleteTexture(b.tex);
        gl.deleteFramebuffer(b.fbo);
        sz = simSize();
        a = makeTarget(sz[0], sz[1]);
        b = makeTarget(sz[0], sz[1]);
        read = a;
        write = b;
      },
      frame: function (t0, dt) {
        var prevFbo = gl.getParameter(gl.FRAMEBUFFER_BINDING);
        var Fx = window.XuanjiFx;
        var m = Fx.mouse;
        var d = dt > 0 ? Math.min(dt, 0.033) : 0.016;
        // 专用参数（默认即原观感）：墨量 / 扩散 / 对比 / 速度(缩放时间)
        var t = t0 * Fx.pct('ink_speed', 1);

        // 模拟一步：read → write
        gl.bindFramebuffer(gl.FRAMEBUFFER, write.fbo);
        gl.viewport(0, 0, write.w, write.h);
        gl.useProgram(simProg);
        gl.uniform2f(uni(simProg, 'u_res'), write.w, write.h);
        gl.uniform1f(uni(simProg, 'u_time'), t);
        gl.uniform1f(uni(simProg, 'u_dt'), d);
        gl.uniform2f(uni(simProg, 'u_mouse'), m.x, m.y);
        gl.uniform1f(uni(simProg, 'u_minf'), m.influence);
        gl.uniform1f(uni(simProg, 'u_audio'), Fx.audio.level);
        gl.uniform1f(uni(simProg, 'u_amount'), Fx.pct('ink_amount', 1));
        gl.uniform1f(uni(simProg, 'u_diffuse'), Fx.pct('ink_diffuse', 1));
        gl.activeTexture(gl.TEXTURE0);
        gl.bindTexture(gl.TEXTURE_2D, read.tex);
        gl.uniform1i(uni(simProg, 'u_dye'), 0);
        window.XuanjiFx.util.bindSeal(gl, simProg, write.w, write.h, 1);
        gl.bindVertexArray(quad);
        gl.drawArrays(gl.TRIANGLES, 0, 3);

        // 渲染到场景
        gl.bindFramebuffer(gl.FRAMEBUFFER, prevFbo);
        gl.viewport(0, 0, canvas.width, canvas.height);
        gl.useProgram(renderProg);
        gl.uniform2f(uni(renderProg, 'u_res'), canvas.width, canvas.height);
        gl.uniform1f(uni(renderProg, 'u_time'), t);
        gl.uniform1f(uni(renderProg, 'u_contrast'), Fx.pct('ink_contrast', 1));
        gl.uniform3fv(uni(renderProg, 'u_accent'), Fx.accent);
        gl.activeTexture(gl.TEXTURE0);
        gl.bindTexture(gl.TEXTURE_2D, write.tex);
        gl.uniform1i(uni(renderProg, 'u_dye'), 0);
        window.XuanjiFx.util.bindSeal(gl, renderProg, canvas.width, canvas.height, 1);
        gl.drawArrays(gl.TRIANGLES, 0, 3);
        gl.bindVertexArray(null);

        var tmp = read;
        read = write;
        write = tmp;
      },
      dispose: function () {
        gl.deleteProgram(simProg);
        gl.deleteProgram(renderProg);
        gl.deleteTexture(a.tex);
        gl.deleteFramebuffer(a.fbo);
        gl.deleteTexture(b.tex);
        gl.deleteFramebuffer(b.fbo);
      },
    };
  });
})();
