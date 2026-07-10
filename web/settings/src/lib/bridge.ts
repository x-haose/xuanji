// 壳 ↔ 设置窗 的桥。壳→窗 走全局回调；窗→壳 走 ipc.postMessage。
import { writable, get } from 'svelte/store';

export type Values = Record<string, unknown>;
export interface ScreenInfo {
  id: string;
  label: string;
}

export const values = writable<Values>({});
export const presets = writable<string[]>([]);
export const screens = writable<ScreenInfo[]>([]);
/// 当前编辑作用域：'' = 全局（所有屏），否则为某屏 id。
export const scope = writable<string>('');

/// 各作用域的解析值缓存：'' 键为全局，屏 id 键为该屏。切换作用域即换这里的一份。
let valuesByScope: Record<string, Values> = {};

declare global {
  interface Window {
    ipc?: { postMessage(s: string): void };
    __xuanjiInitSettings?: (p: {
      values: Values;
      presets: string[];
      screens?: ScreenInfo[];
      screenValues?: Record<string, Values>;
    }) => void;
    __xuanjiSetField?: (key: string, value: unknown, screen?: string | null) => void;
  }
}

function send(msg: object): void {
  window.ipc?.postMessage(JSON.stringify(msg));
}

/// 当前作用域对应的缓存键。
function scopeKey(): string {
  return get(scope);
}

export const api = {
  ready: () => send({ cmd: 'ready' }),
  set: (key: string, value: unknown) => {
    const sk = scopeKey();
    const next = { ...(valuesByScope[sk] ?? {}), [key]: value };
    valuesByScope[sk] = next;
    values.set(next);
    send({ cmd: 'set', key, value, screen: sk || null });
  },
  pick: (key: string, kind: 'file' | 'directory') =>
    send({ cmd: 'pick', key, kind, screen: scopeKey() || null }),
  savePreset: (name: string) => send({ cmd: 'savepreset', name }),
  applyPreset: (name: string) => send({ cmd: 'applypreset', name }),
  deletePreset: (name: string) => send({ cmd: 'deletepreset', name }),
  reset: () => send({ cmd: 'reset' }),
  /// 切换编辑作用域，把对应作用域的缓存值挂到 values。
  setScope: (id: string) => {
    scope.set(id);
    values.set(valuesByScope[id] ?? valuesByScope[''] ?? {});
  },
};

// 壳注入当前配置 / rfd 选择结果回填。
window.__xuanjiInitSettings = (p) => {
  valuesByScope = { '': p.values, ...(p.screenValues ?? {}) };
  presets.set(p.presets);
  screens.set(p.screens ?? []);
  // 保持当前作用域（若仍存在），否则回落全局。
  const sk = get(scope);
  const keep = sk in valuesByScope ? sk : '';
  scope.set(keep);
  values.set(valuesByScope[keep] ?? {});
};
window.__xuanjiSetField = (key, value, screen) => {
  const sk = screen ?? '';
  valuesByScope[sk] = { ...(valuesByScope[sk] ?? {}), [key]: value };
  if (sk === get(scope)) {
    values.update((v) => ({ ...v, [key]: value }));
  }
};
