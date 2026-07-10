<script lang="ts">
  import { values, api } from '../bridge';
  import type { Prop } from '../schema';

  let { prop }: { prop: Prop } = $props();
  let on = $derived(Boolean($values[prop.key] ?? prop.value));
</script>

<label class="row toggle">
  <span class="label">{prop.text}</span>
  <input
    type="checkbox"
    checked={on}
    onchange={(e) => api.set(prop.key, e.currentTarget.checked)}
  />
  <span class="switch" class:on aria-hidden="true"></span>
</label>
