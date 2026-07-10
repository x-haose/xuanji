// 壳 ↔ 设置窗 的桥。壳→窗 走全局回调；窗→壳 走 ipc.postMessage。
import { writable } from 'svelte/store';

export type Values = Record<string, unknown>;
export const values = writable<Values>({});
export const presets = writable<string[]>([]);

declare global {
  interface Window {
    ipc?: { postMessage(s: string): void };
    __xuanjiInitSettings?: (p: { values: Values; presets: string[] }) => void;
    __xuanjiSetField?: (key: string, value: unknown) => void;
  }
}

function send(msg: object): void {
  window.ipc?.postMessage(JSON.stringify(msg));
}

export const api = {
  ready: () => send({ cmd: 'ready' }),
  set: (key: string, value: unknown) => {
    values.update((v) => ({ ...v, [key]: value }));
    send({ cmd: 'set', key, value });
  },
  pick: (key: string, kind: 'file' | 'directory') => send({ cmd: 'pick', key, kind }),
  savePreset: (name: string) => send({ cmd: 'savepreset', name }),
  applyPreset: (name: string) => send({ cmd: 'applypreset', name }),
  deletePreset: (name: string) => send({ cmd: 'deletepreset', name }),
  reset: () => send({ cmd: 'reset' }),
};

// 壳注入当前配置 / rfd 选择结果回填。
window.__xuanjiInitSettings = (p) => {
  values.set(p.values);
  presets.set(p.presets);
};
window.__xuanjiSetField = (key, value) => {
  values.update((v) => ({ ...v, [key]: value }));
};
