<script lang="ts">
  import { values, api } from '../bridge';
  import type { Prop } from '../schema';

  let { prop }: { prop: Prop } = $props();
  let min = $derived(prop.min ?? 0);
  let max = $derived(prop.max ?? 10);
  let val = $derived(Number($values[prop.key] ?? prop.value));
</script>

<label class="row">
  <span class="label">{prop.text}</span>
  <input
    type="range"
    {min}
    {max}
    step="1"
    value={val}
    oninput={(e) => api.set(prop.key, Number(e.currentTarget.value))}
  />
  <output>{val}</output>
</label>
