<script lang="ts">
  import { presets, api } from './bridge';

  let sel = $state('');
  let newName = $state('');

  function save() {
    if (!newName) return;
    api.savePreset(newName);
    newName = '';
  }
</script>

<div class="presets">
  <select bind:value={sel}>
    <option value="">— 预设 —</option>
    {#each $presets as name (name)}
      <option value={name}>{name}</option>
    {/each}
  </select>
  <button disabled={!sel} onclick={() => api.applyPreset(sel)}>应用</button>
  <button disabled={!sel} onclick={() => api.deletePreset(sel)}>删除</button>
  <input placeholder="新预设名" bind:value={newName} onkeydown={(e) => e.key === 'Enter' && save()} />
  <button disabled={!newName} onclick={save}>存为</button>
  <button class="reset" onclick={() => api.reset()}>恢复默认</button>
</div>
