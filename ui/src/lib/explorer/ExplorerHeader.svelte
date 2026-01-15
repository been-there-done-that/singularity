<script lang="ts">
    import ModelSelector from './ModelSelector.svelte';
    import ExecutionModeBadge from './ExecutionModeBadge.svelte';
    import type { ExecutionMode } from '$lib/dsl/types';

    interface Props {
        model: string | null;
        mode: ExecutionMode | null;
        showDsl: boolean;
        onModelSelect: (model: string) => void;
        onToggleDsl: () => void;
    }

    let { model, mode, showDsl, onModelSelect, onToggleDsl }: Props = $props();
</script>

<div class="flex items-center justify-between p-4 border-b border-zinc-800 bg-zinc-900/50 backdrop-blur-sm sticky top-0 z-10">
    <div class="flex items-center gap-4">
        <ModelSelector 
            value={model} 
            onSelect={onModelSelect} 
        />
        
        {#if model}
            <ExecutionModeBadge {mode} />
        {/if}
    </div>

    <div class="flex items-center gap-3">
        <button 
            onclick={onToggleDsl}
            class="text-xs font-medium px-3 py-1.5 rounded border border-zinc-700 hover:bg-zinc-800 transition-colors {showDsl ? 'bg-zinc-800 text-white' : 'text-zinc-400'}"
        >
            DSL
        </button>
    </div>
</div>
