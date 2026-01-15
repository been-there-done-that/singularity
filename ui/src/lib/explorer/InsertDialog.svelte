<script lang="ts">
    interface Field {
        name: string;
        type: string;
        nullable: boolean;
    }

    interface Props {
        fields: Field[];
        loading: boolean;
        onClose: () => void;
        onSave: (data: any) => void;
    }

    let { fields, loading, onClose, onSave }: Props = $props();

    let formData = $state<Record<string, any>>({});
    
    // Filter out system fields
    const systemFields = new Set(['id', 'created_at', 'updated_at', 'owner_id']);
    let editableFields = $derived(fields.filter(f => !systemFields.has(f.name)));

    function handleSubmit(e: Event) {
        e.preventDefault();
        onSave(formData);
    }
</script>

<div class="fixed inset-0 z-50 flex items-center justify-center p-4 sm:p-6" role="dialog" aria-modal="true">
    <!-- Backdrop -->
    <!-- svelte-ignore a11y_click_events_have_key_events -->
    <!-- svelte-ignore a11y_no_static_element_interactions -->
    <div class="absolute inset-0 bg-black/60 backdrop-blur-sm" onclick={onClose}></div>

    <!-- Dialog -->
    <div class="relative w-full max-w-lg bg-zinc-900 border border-zinc-800 rounded-xl shadow-2xl flex flex-col max-h-[90vh] animate-in fade-in zoom-in-95 duration-200">
        <div class="flex items-center justify-between p-4 border-b border-zinc-800">
            <h2 class="text-lg font-semibold text-zinc-100">Insert Record</h2>
            <button class="text-zinc-400 hover:text-white" onclick={onClose}>✕</button>
        </div>

        <form onsubmit={handleSubmit} class="flex-1 overflow-auto p-4 space-y-4">
            {#each editableFields as field}
                <div class="space-y-1">
                    <label class="text-xs font-medium text-zinc-400 uppercase tracking-wide flex items-center gap-1">
                        {field.name}
                        {#if !field.nullable}<span class="text-red-400" title="Required">*</span>{/if}
                        <span class="text-zinc-600 text-[10px] lowercase">({field.type})</span>
                    </label>
                    
                    {#if field.type === 'Integer' || field.type === 'Float'}
                        <input 
                            type="number"
                            step={field.type === 'Integer' ? "1" : "any"}
                            bind:value={formData[field.name]}
                            class="w-full bg-black/50 border border-zinc-800 rounded px-3 py-2 text-sm text-zinc-200 focus:border-emerald-500 focus:outline-none transition-colors placeholder-zinc-700"
                            placeholder={field.type === 'Integer' ? "0" : "0.00"}
                            required={!field.nullable}
                        />
                    {:else if field.type === 'Boolean'}
                        <div class="flex items-center gap-2 mt-1">
                             <input 
                                type="checkbox"
                                bind:checked={formData[field.name]}
                                class="w-4 h-4 rounded border-zinc-700 bg-zinc-800 text-emerald-500 focus:ring-emerald-500/20"
                             />
                             <span class="text-sm text-zinc-400">Value</span>
                        </div>
                    {:else}
                         <input 
                            type="text"
                            bind:value={formData[field.name]}
                            class="w-full bg-black/50 border border-zinc-800 rounded px-3 py-2 text-sm text-zinc-200 focus:border-emerald-500 focus:outline-none transition-colors placeholder-zinc-700"
                            placeholder="Value..."
                            required={!field.nullable}
                        />
                    {/if}
                </div>
            {/each}
            
            {#if editableFields.length === 0}
                 <div class="text-center text-zinc-500 py-8">
                     No editable user fields found.
                 </div>
            {/if}
        </form>

        <div class="p-4 border-t border-zinc-800 flex justify-end gap-3 bg-zinc-900/50 rounded-b-xl">
            <button 
                type="button"
                onclick={onClose}
                class="px-4 py-2 text-sm font-medium text-zinc-400 hover:text-white transition-colors"
                disabled={loading}
            >
                Cancel
            </button>
            <button 
                onclick={handleSubmit}
                disabled={loading}
                class="px-4 py-2 text-sm font-medium bg-emerald-600 hover:bg-emerald-500 text-white rounded shadow-lg shadow-emerald-900/20 transition-all disabled:opacity-50 disabled:cursor-not-allowed flex items-center gap-2"
            >
                {#if loading}
                    <div class="w-4 h-4 border-2 border-white/30 border-t-white rounded-full animate-spin"></div>
                {/if}
                Insert
            </button>
        </div>
    </div>
</div>
