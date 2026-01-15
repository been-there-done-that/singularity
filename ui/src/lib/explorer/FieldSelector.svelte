<script lang="ts">
    interface Field {
        name: string;
        type: string;
        nullable: boolean;
    }

    interface Props {
        fields: Field[];
        selected: string[];
        onSelect: (fields: string[]) => void;
    }

    let { fields, selected, onSelect }: Props = $props();

    function toggleField(name: string) {
        if (selected.includes(name)) {
            // Cannot deselect last field? Maybe allow it but disabled query.
            onSelect(selected.filter(f => f !== name));
        } else {
            onSelect([...selected, name]);
        }
    }
    
    function selectAll() {
        onSelect(fields.map(f => f.name));
    }
    
    function clearAll() {
        onSelect([]);
    }
</script>

<div class="flex flex-col h-full bg-zinc-900/30 border-r border-zinc-800">
    <div class="flex items-center justify-between p-3 border-b border-zinc-800">
        <span class="text-xs font-medium text-zinc-400 uppercase tracking-wider">Fields</span>
        <div class="flex gap-2">
            <button onclick={selectAll} class="text-[10px] text-zinc-500 hover:text-zinc-300">All</button>
            <button onclick={clearAll} class="text-[10px] text-zinc-500 hover:text-zinc-300">None</button>
        </div>
    </div>
    
    <div class="overflow-y-auto flex-1 p-2 space-y-0.5">
        {#each fields as field}
            <label class="flex items-center gap-2 px-2 py-1.5 rounded hover:bg-zinc-800/50 cursor-pointer group transition-colors">
                <input 
                    type="checkbox" 
                    checked={selected.includes(field.name)}
                    onchange={() => toggleField(field.name)}
                    class="w-3.5 h-3.5 rounded border-zinc-700 bg-zinc-800 text-emerald-500 focus:ring-emerald-500/20 focus:ring-offset-0"
                />
                <span class="text-sm text-zinc-300 group-hover:text-white transition-colors">{field.name}</span>
                <span class="text-[10px] text-zinc-600 font-mono ml-auto">{field.type}</span>
            </label>
        {/each}
        
        {#if fields.length === 0}
            <div class="p-4 text-center">
                <p class="text-xs text-zinc-600">No fields available</p>
            </div>
        {/if}
    </div>
</div>
