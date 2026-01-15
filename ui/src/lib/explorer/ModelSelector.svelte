<script lang="ts">
    import { onMount } from 'svelte';
    import { requestCapability, execute } from '$lib/kernel/pipeline';
    import type { KernelError } from '$lib/kernel/types';

    interface Props {
        value: string | null;
        onSelect: (model: string) => void;
    }

    let { value, onSelect }: Props = $props();

    let models = $state<string[]>([]);
    let loading = $state(true);
    let error = $state<string | null>(null);

    async function loadModels() {
        try {
            loading = true;
            // Phase 1: Request Capability
            const grant = await requestCapability({
                op: 'schema.list_models',
                resource: { resource_type: 'system', resource_id: null },

                input: {}
            });

            // Phase 2: Execute
            const result = await execute<{ name: string }[]>(grant.token, {});
            models = result.map(m => m.name).sort();
        } catch (e: any) {
            console.error('Failed to load models:', e);
            error = e.message || 'Failed to load models';
        } finally {
            loading = false;
        }
    }

    onMount(() => {
        loadModels();
    });
</script>

<div class="relative">
    <select
        value={value || ""}
        onchange={(e) => onSelect(e.currentTarget.value)}
        class="appearance-none bg-zinc-900 border border-zinc-700 rounded-lg px-3 py-1.5 pr-8 text-sm text-zinc-200 focus:outline-none focus:border-zinc-500 min-w-[160px]"
        disabled={loading}
    >
        <option value="" disabled>
            {loading ? "Loading..." : "Select Model"}
        </option>
        {#each models as model}
            <option value={model}>{model}</option>
        {/each}
    </select>
    
    <!-- Chevron -->
    <div class="absolute right-2 top-1/2 -translate-y-1/2 pointer-events-none text-zinc-500">
        <svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="m6 9 6 6 6-6"/></svg>
    </div>
</div>

{#if error}
    <div class="text-red-400 text-xs mt-1 absolute">{error}</div>
{/if}
