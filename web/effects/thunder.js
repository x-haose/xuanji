// 雷法符箓：翻涌暗云 + 程序化闪电偶发劈落 + 击闪辉光。呼应「九天应元雷声普化天尊」。
(function () {
  'use strict';
  var U = window.XuanjiFx.util;

  var FS =
    '#version 300 es\nprecision highp float;\n' +
    'uniform vec2 u_res; uniform float u_time; uniform vec2 u_mouse; uniform float u_minf; uniform vec3 u_accent; uniform float u_bass; out vec4 frag;\n' +
    'float hash(vec2 p){p=fract(p*vec2(123.34,456.21));p+=dot(p,p+34.5);return fract(p.x*p.y);}\n' +
    'float noise(vec2 p){vec2 i=floor(p),f=fract(p);float a=hash(i),b=hash(i+vec2(1,0)),c=hash(i+vec2(0,1)),d=hash(i+vec2(1,1));vec2 u=f*f*(3.0-2.0*f);return mix(mix(a,b,u.x),mix(c,d,u.x),u.y);}\n' +
    'float fbm(vec2 p){float v=0.0,a=0.5;for(int i=0;i<5;i++){v+=a*noise(p);p*=2.0;a*=0.5;}return v;}\n' +
    // 一次击闪的时间包络：陡起快落 + 高频闪烁
    'float strike(float t,float seed){float ph=fract(t*0.33+seed);\n' +
    ' float f=(1.0-smoothstep(0.0,0.14,ph))*smoothstep(0.0,0.015,ph);\n' +
    ' f*=0.55+0.45*noise(vec2(t*40.0,seed*13.0));return f;}\n' +
    // 闪电路径 x(y)：多倍频抖动 + 细锯齿，形成折线感
    'float boltX(float y,float s){return 0.16*(fbm(vec2(s*7.0,y*4.0+s))-0.5)\n' +
    ' +0.08*(fbm(vec2(s*3.0,y*10.0-s))-0.5)+0.022*sin(y*24.0+s*6.0);}\n' +
    // 单段辉光：核心 + 外晕，w 控制粗细
    'float seg(vec2 p,float x,float w){float d=abs(p.x-x);\n' +
    ' return 0.0026*w/(d*d+0.00022)+0.012*w/(d+0.06);}\n' + // 核心更细更亮，外晕更收
    // 一道闪电：主干 + 两条斜向分叉（仅在分叉点以下存在）
    'float bolt(vec2 p,float baseX,float s,float t){\n' +
    ' float g=seg(p,baseX+boltX(p.y,s),1.0);\n' +
    ' float fy=0.05+0.5*fract(s*13.0);float sl=0.8*(fract(s*29.0)-0.5);\n' +
    ' float bx=baseX+boltX(fy,s)+(fy-p.y)*sl+0.05*(fbm(vec2(s*11.0,p.y*7.0))-0.5);\n' +
    ' g+=seg(p,bx,0.5)*(p.y<fy?1.0:0.0);\n' +
    ' float fy2=0.5*fract(s*17.0);float sl2=0.7*(fract(s*41.0)-0.5);\n' +
    ' float bx2=baseX+boltX(fy2,s)+(fy2-p.y)*sl2;\n' +
    ' g+=seg(p,bx2,0.35)*(p.y<fy2?1.0:0.0);\n' +
    ' return g*strike(t,s);}\n' +
    // 光标劈雷：离散击闪(大部分时间灭)，故随光标是「一次次劈现」而非横滑
    'float cstrike(float t){float ph=fract(t*0.7);\n' +
    ' return (1.0-smoothstep(0.0,0.11,ph))*smoothstep(0.0,0.02,ph)*(0.5+0.5*noise(vec2(t*40.0,9.0)));}\n' +
    'float mbolt(vec2 p,float mx,float t){\n' +
    ' float g=seg(p,mx+boltX(p.y,3.1),0.85);\n' +
    ' float fy=0.12;g+=seg(p,mx+boltX(fy,3.1)+(fy-p.y)*0.35,0.4)*(p.y<fy?1.0:0.0);\n' +
    ' float fy2=0.3;g+=seg(p,mx+boltX(fy2,5.7)-(fy2-p.y)*0.3,0.3)*(p.y<fy2?1.0:0.0);\n' +
    ' return g*cstrike(t);}\n' +
    'void main(){\n' +
    ' vec2 uv=gl_FragCoord.xy/u_res;\n' +
    ' vec2 p=(gl_FragCoord.xy-0.5*u_res)/u_res.y;\n' +
    ' float t=u_time;\n' +
    // 体积云：域扭曲翻涌 + 密度梯度自阴影(伪体积光,上亮下暗) + 底部云雾
    ' vec2 q=vec2(fbm(p*1.6+vec2(t*0.02,0.0)),fbm(p*1.6+vec2(3.1,1.7)-t*0.015));\n' +
    ' float cl=fbm(p*1.9+1.7*q+vec2(0.0,t*0.03));\n' + // 翻涌云体
    ' float above=fbm((p+vec2(0.0,0.055))*1.9+1.7*q+vec2(0.0,t*0.03));\n' +
    ' float shade=clamp((cl-above)*5.0+0.5,0.0,1.0);\n' + // 密度向上增=受光
    ' vec3 base=mix(vec3(0.015,0.015,0.045),vec3(0.22,0.20,0.34),smoothstep(0.28,0.72,cl));\n' +
    ' vec3 storm=base*(0.5+0.75*shade);\n' + // 体积明暗(更亮更立体)
    ' float fog=smoothstep(0.1,-0.45,p.y)*fbm(p*3.0+vec2(t*0.06,0.0))*0.28;\n' + // 底部云雾
    ' storm+=vec3(0.12,0.13,0.2)*fog;\n' +
    ' storm*=1.0+u_bass*0.8;\n' + // 低频鼓点让云层微亮

    // 三道闪电（不同位置/时相）
    ' float b=bolt(p,-0.45,0.17,t)+bolt(p,0.05,0.53,t)+bolt(p,0.5,0.81,t);\n' +
    ' float amb=strike(t,0.17)+strike(t,0.53)+strike(t,0.81);\n' + // 击闪时全局提亮
    ' vec3 col=storm+storm*amb*1.2;\n' +
    ' vec3 boltc=vec3(0.6,0.75,1.0);\n' + // 蓝白电光
    ' b+=step(0.62,u_bass)*strike(t*3.7,u_bass)*0.5;\n' + // 仅强拍触发额外闪电
    ' col+=boltc*b;\n' +
    ' col+=vec3(0.18,0.26,0.42)*smoothstep(0.5,0.9,u_bass)*0.5;\n' + // 重拍轻微蓝辉(阈值门控,不整屏泛白)
    // 光标电荷辉光（蓄势）+ 朝光标离散劈雷（爆发）
    ' float mg=exp(-dot(p-u_mouse,p-u_mouse)*30.0)*(0.45+0.55*noise(vec2(t*30.0,3.0)));\n' +
    ' col+=vec3(0.5,0.66,1.0)*mg*u_minf*0.5;\n' +
    ' col+=boltc*mbolt(p,u_mouse.x,t)*u_minf;\n' +
    ' col*=smoothstep(1.35,0.2,length(p));\n' +
    ' frag=vec4(col,1.0);}';

  window.XuanjiFx.register('thunder', function (gl, canvas) {
    var prog = U.program(gl, U.FS_VS, FS);
    var quad = U.fullscreenTriangle(gl);
    var uRes = gl.getUniformLocation(prog, 'u_res');
    var uTime = gl.getUniformLocation(prog, 'u_time');
    var uMouse = gl.getUniformLocation(prog, 'u_mouse');
    var uMinf = gl.getUniformLocation(prog, 'u_minf');
    var uAccent = gl.getUniformLocation(prog, 'u_accent');
    var w = canvas.width, h = canvas.height;
    return {
      resize: function (nw, nh) {
        w = nw;
        h = nh;
      },
      frame: function (t) {
        gl.disable(gl.BLEND);
        gl.useProgram(prog);
        gl.uniform2f(uRes, w, h);
        gl.uniform1f(uTime, t);
        var mo = window.XuanjiFx.mouse; // [0,1] → 居中·纵横校正空间
        gl.uniform2f(uMouse, (mo.x - 0.5) * (w / h), mo.y - 0.5);
        gl.uniform1f(uMinf, mo.influence);
        gl.uniform3fv(uAccent, window.XuanjiFx.accent);
        gl.uniform1f(gl.getUniformLocation(prog, 'u_bass'), window.XuanjiFx.audio.bass);
        gl.bindVertexArray(quad);
        gl.drawArrays(gl.TRIANGLES, 0, 3);
      },
      dispose: function () {
        gl.deleteProgram(prog);
      },
    };
  });
})();
