<script lang="ts">
  import { values, api } from '../bridge';
  import type { Prop } from '../schema';

  let { prop }: { prop: Prop } = $props();
  let cur = $derived(String($values[prop.key] ?? prop.value));
</script>

<label class="row">
  <span class="label">{prop.text}</span>
  <select value={cur} onchange={(e) => api.set(prop.key, e.currentTarget.value)}>
    {#each prop.options ?? [] as opt (opt.value)}
      <option value={opt.value}>{opt.label}</option>
    {/each}
  </select>
</label>
