<script lang="ts">
    import type { ExecutionMode } from '$lib/dsl/types';

    interface Props {
        mode: ExecutionMode | null;
    }

    let { mode }: Props = $props();

    const colors = {
        FAST: 'bg-emerald-500/10 text-emerald-400 border-emerald-500/20',
        SLOW: 'bg-amber-500/10 text-amber-400 border-amber-500/20'
    };
</script>

{#if mode}
    <div class="flex items-center gap-1.5 px-2 py-0.5 rounded text-xs font-mono font-medium border {colors[mode]}">
        <div class="app-mode-indicator {mode === 'FAST' ? 'bg-emerald-400' : 'bg-amber-400'}"></div>
        {mode}
    </div>
{/if}

<style>
    .app-mode-indicator {
        width: 6px;
        height: 6px;
        border-radius: 50%;
    }
</style>
