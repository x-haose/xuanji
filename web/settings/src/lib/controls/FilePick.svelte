<script lang="ts">
  import { values, api } from '../bridge';
  import type { Prop } from '../schema';

  let { prop }: { prop: Prop } = $props();
  let path = $derived(String($values[prop.key] ?? ''));
  let kind = $derived<'file' | 'directory'>(prop.type === 'directory' ? 'directory' : 'file');
</script>

<div class="row">
  <span class="label">{prop.text}</span>
  <div class="filepick">
    <span class="path" title={path}>{path || '未选择'}</span>
    <button onclick={() => api.pick(prop.key, kind)}>选择…</button>
  </div>
</div>
