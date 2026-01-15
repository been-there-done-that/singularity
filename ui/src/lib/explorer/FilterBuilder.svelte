<script lang="ts">
    interface Field {
        name: string;
        type: string;
    }

    interface FilterRow {
        id: string; // for key
        field: string;
        op: string;
        value: string;
    }

    interface Props {
        fields: Field[];
        onChange: (filter: any) => void;
    }

    let { fields, onChange }: Props = $props();

    let filters = $state<FilterRow[]>([]);

    const operators = [
        { value: 'eq', label: '=' },
        { value: 'neq', label: '!=' },
        { value: 'gt', label: '>' },
        { value: 'lt', label: '<' },
        { value: 'like', label: 'like' },
    ];

    function addFilter() {
        if (fields.length === 0) return;
        filters.push({
            id: crypto.randomUUID(),
            field: fields[0].name,
            op: 'eq',
            value: ''
        });
        update();
    }

    function removeFilter(index: number) {
        filters.splice(index, 1);
        update();
    }

    function update() {
        if (filters.length === 0) {
            onChange(null);
            return;
        }

        // Build generic filter structure
        // Simple AND of all filters
        // API format: { "and": [ { "eq": ["field", "value"] }, ... ] }
        // If single: { "eq": ["field", "value"] }
        
        const conditions = filters.map(f => {
            // Convert value based on type?
            // For now everything as string or check field type.
            const fieldDef = fields.find(fd => fd.name === f.field);
            let val: any = f.value;
            if (fieldDef) {
                 if (fieldDef.type === 'Integer') val = parseInt(f.value) || 0;
                 else if (fieldDef.type === 'Float') val = parseFloat(f.value) || 0;
                 else if (fieldDef.type === 'Boolean') val = f.value === 'true';
            }
            return { [f.op]: [f.field, val] };
        });

        if (conditions.length === 1) {
            onChange(conditions[0]);
        } else {
            onChange({ and: conditions });
        }
    }
</script>

<div class="flex items-center flex-wrap gap-2">
    {#each filters as filter, i (filter.id)}
        <div class="flex items-center gap-1 bg-zinc-900/50 rounded-lg px-2 py-1 text-xs border border-zinc-800 shadow-sm animate-in fade-in slide-in-from-left-2 duration-200">
             <select 
                bind:value={filter.field} 
                onchange={update}
                class="bg-transparent text-zinc-300 font-medium focus:outline-none cursor-pointer hover:text-white"
             >
                 {#each fields as f}
                     <option value={f.name}>{f.name}</option>
                 {/each}
             </select>
             
             <select 
                bind:value={filter.op} 
                onchange={update}
                class="bg-zinc-800 rounded px-1 py-0.5 text-zinc-400 focus:outline-none cursor-pointer hover:text-zinc-200"
             >
                 {#each operators as op}
                     <option value={op.value}>{op.label}</option>
                 {/each}
             </select>
             
             <input 
                type="text" 
                bind:value={filter.value} 
                oninput={update}
                placeholder="Value..."
                class="w-24 bg-transparent border-b border-zinc-700 text-emerald-400 focus:border-emerald-500 focus:outline-none px-1"
             />
             
             <button 
                onclick={() => removeFilter(i)}
                class="ml-1 text-zinc-600 hover:text-red-400 transition-colors w-4 h-4 flex items-center justify-center rounded-full hover:bg-zinc-800"
             >
                 ✕
             </button>
        </div>
        
        {#if i < filters.length - 1}
            <span class="text-zinc-700 font-mono text-[10px] uppercase font-bold">AND</span>
        {/if}
    {/each}

    {#if fields.length > 0}
        <button 
            onclick={addFilter}
            class="px-2 py-1 text-xs font-medium text-zinc-500 hover:text-zinc-300 border border-dashed border-zinc-700 hover:border-zinc-500 rounded-md transition-all flex items-center gap-1"
        >
            <span class="text-[10px]">+</span> Filter
        </button>
    {/if}
</div>
