<script lang="ts">
  import { onMount } from 'svelte';
  import { values, api, screens, scope } from './lib/bridge';
  import { loadSchema, condOk, GROUP_ORDER, type Prop } from './lib/schema';
  import { accentFor } from './lib/color';
  import Presets from './lib/Presets.svelte';
  import Slider from './lib/controls/Slider.svelte';
  import Toggle from './lib/controls/Toggle.svelte';
  import Color from './lib/controls/Color.svelte';
  import Combo from './lib/controls/Combo.svelte';
  import FilePick from './lib/controls/FilePick.svelte';

  let schema = $state<Prop[]>([]);
  let active = $state('');

  onMount(async () => {
    schema = await loadSchema();
    active = GROUP_ORDER.find((g) => schema.some((p) => p.group === g)) ?? '';
    api.ready();
  });

  // 只列当前真有可调项的组：联动隐藏（如 bgtype≠粒子时粒子组）整组不显示，避免空面板。
  let groups = $derived(
    GROUP_ORDER.filter((g) => schema.some((p) => p.group === g && condOk(p.condition, $values)))
  );
  let visible = $derived(schema.filter((p) => p.group === active && condOk(p.condition, $values)));
  let accent = $derived(accentFor(String($values.userRiGan ?? '甲')));

  // 当前分组因切换背景类型等被整组隐藏时，自动跳到第一个有内容的组。
  $effect(() => {
    if (groups.length > 0 && !groups.includes(active)) {
      active = groups[0];
    }
  });
</script>

<main style="--accent:{accent}">
  <header>
    <h1>璇玑 <span>· 设置</span></h1>
    {#if $screens.length > 1}
      <label class="scope">
        <span>作用范围</span>
        <select value={$scope} onchange={(e) => api.setScope(e.currentTarget.value)}>
          <option value="">所有屏幕</option>
          {#each $screens as s (s.id)}
            <option value={s.id}>{s.label}</option>
          {/each}
        </select>
      </label>
    {/if}
    <Presets />
  </header>
  <div class="body">
    <nav>
      {#each groups as g (g)}
        <button class:active={g === active} onclick={() => (active = g)}>{g}</button>
      {/each}
    </nav>
    <section class="panel">
      {#each visible as prop (prop.key)}
        {#if prop.type === 'slider'}
          <Slider {prop} />
        {:else if prop.type === 'bool'}
          <Toggle {prop} />
        {:else if prop.type === 'color'}
          <Color {prop} />
        {:else if prop.type === 'combo'}
          <Combo {prop} />
        {:else}
          <FilePick {prop} />
        {/if}
      {/each}
      {#if visible.length === 0}
        <p class="empty">该分组当前无可调项。</p>
      {/if}
    </section>
  </div>
</main>

<style>
  .scope {
    display: flex;
    align-items: center;
    gap: 8px;
    margin-left: auto;
    font-size: 0.85rem;
    color: var(--text-dim);
  }
  .scope select {
    min-width: 140px;
  }
</style>
