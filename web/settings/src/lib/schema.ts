// 从 project.json 读取参数 schema（类型/默认/联动），驱动 UI 渲染。
import type { Values } from './bridge';

export type PropType = 'slider' | 'bool' | 'color' | 'combo' | 'file' | 'directory';

export interface ComboOption {
  label: string;
  value: string;
}

export interface Prop {
  key: string;
  type: PropType;
  text: string;
  value: unknown;
  order: number;
  min?: number;
  max?: number;
  options?: ComboOption[];
  condition?: string;
  group: string;
}

/// 分组显示顺序（左侧导航）。
export const GROUP_ORDER = ['背景', '玻璃质感', '音频律动', '通用 · 占卜'];

function groupOf(key: string): string {
  if (key.startsWith('glass')) return '玻璃质感';
  if (key.startsWith('audio')) return '音频律动';
  // 背景类型专属参数（粒子/线条/渐变/图片/视频/轮播）全并入「背景」，
  // 靠各自的 condition 按当前 bgtype 显隐——选了对应类型才出现。
  if (
    key.startsWith('bg') ||
    key.startsWith('gradient') ||
    key.startsWith('particle') ||
    key.startsWith('line') ||
    key === 'movedirection' ||
    key === 'videofile' ||
    key === 'slideshowinterval'
  ) {
    return '背景';
  }
  return '通用 · 占卜';
}

interface RawProp {
  type: PropType;
  text?: string;
  value: unknown;
  order?: number;
  min?: number;
  max?: number;
  options?: ComboOption[];
  condition?: string;
}

export async function loadSchema(): Promise<Prop[]> {
  // 相对当前 origin 根，兼容两平台的协议映射（mac: xuanji://localhost，win: http://xuanji.localhost）。
  const res = await fetch('/project.json');
  const json = (await res.json()) as { general: { properties: Record<string, RawProp> } };
  const props = json.general.properties;
  return Object.entries(props)
    // 剔除 WE 系统残留项（如 schemecolor，页面从不读，标签是未翻译的 ui_* i18n key）。
    .filter(([, p]) => !(p.text ?? '').startsWith('ui_'))
    .map(([key, p]): Prop => ({
      key,
      type: p.type,
      text: p.text ?? key,
      value: p.value,
      // bgtype 是背景选择器，永远排「背景」组最前。
      order: key === 'bgtype' ? -1 : (p.order ?? 0),
      min: p.min,
      max: p.max,
      options: p.options,
      condition: p.condition,
      group: groupOf(key),
    }))
    .sort((a, b) => a.order - b.order);
}

/// 求值 project.json 的联动表达式（形如 `bgtype.value === 'image'`）。
/// 严格模式禁 `with`，故把每个 key 显式绑成局部 `{value}`。表达式来自打包的
/// project.json（可信），非用户输入。
export function condOk(cond: string | undefined, values: Values): boolean {
  if (!cond) return true;
  const keys = Object.keys(values);
  const wrapped: Record<string, { value: unknown }> = {};
  for (const k of keys) wrapped[k] = { value: values[k] };
  const body = keys.map((k) => `const ${k}=s[${JSON.stringify(k)}];`).join('') + `return (${cond});`;
  try {
    return Boolean(new Function('s', body)(wrapped));
  } catch {
    return true;
  }
}
