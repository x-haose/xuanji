<script lang="ts">
  import { values, api } from '../bridge';
  import { rgbStrToHex, hexToRgbStr } from '../color';
  import type { Prop } from '../schema';

  let { prop }: { prop: Prop } = $props();
  let hex = $derived(rgbStrToHex(String($values[prop.key] ?? prop.value)));
</script>

<label class="row">
  <span class="label">{prop.text}</span>
  <input
    type="color"
    value={hex}
    oninput={(e) => api.set(prop.key, hexToRgbStr(e.currentTarget.value))}
  />
</label>
