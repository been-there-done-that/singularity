<script lang="ts">
    import type { Row } from '$lib/dsl/types';

    interface Props {
        rows: Row[];
        columns: string[];
        loading: boolean;
        onEdit?: (row: Row) => void;
        onDelete?: (row: Row) => void;
    }

    let { rows, columns, loading, onEdit, onDelete }: Props = $props();
</script>

<div class="flex-1 overflow-auto bg-zinc-900 relative">
    {#if loading}
        <div class="absolute inset-0 bg-zinc-900/50 backdrop-blur-[1px] z-10 flex items-center justify-center">
            <div class="w-6 h-6 border-2 border-emerald-500 border-t-transparent rounded-full animate-spin"></div>
        </div>
    {/if}

    <table class="w-full border-collapse text-left">
        <thead class="sticky top-0 bg-zinc-900 z-10">
            <tr>
                {#each columns as col}
                    <th class="px-4 py-2 text-xs font-semibold text-zinc-500 uppercase tracking-wider border-b border-zinc-800 bg-zinc-900/90 whitespace-nowrap">
                        {col}
                    </th>
                {/each}
                {#if columns.length === 0}
                    <th class="px-4 py-2 text-xs font-semibold text-zinc-500 border-b border-zinc-800">No columns selected</th>
                {/if}
                {#if onEdit || onDelete}
                     <th class="px-4 py-2 text-xs font-semibold text-zinc-500 uppercase tracking-wider border-b border-zinc-800 bg-zinc-900/90 w-20 text-right">
                        Actions
                     </th>
                {/if}
            </tr>
        </thead>
        <tbody class="divide-y divide-zinc-800/50">
            {#each rows as row}
                <tr class="group hover:bg-white/5 transition-colors">
                    {#each columns as col}
                        <td class="px-4 py-2 text-sm text-zinc-300 font-mono whitespace-nowrap">
                            {#if row[col] === null}
                                <span class="text-zinc-600 italic">null</span>
                            {:else if typeof row[col] === 'boolean'}
                                <span class={row[col] ? 'text-emerald-400' : 'text-red-400'}>{String(row[col])}</span>
                            {:else if typeof row[col] === 'object'}
                                <span class="text-zinc-500 text-xs">{JSON.stringify(row[col])}</span>
                            {:else}
                                {String(row[col])}
                            {/if}
                        </td>
                    {/each}
                    {#if onEdit || onDelete}
                        <td class="px-4 py-2 text-right whitespace-nowrap opacity-0 group-hover:opacity-100 transition-opacity">
                            <div class="flex items-center justify-end gap-2">
                                {#if onEdit}
                                    <button 
                                        class="p-1 hover:text-emerald-400 text-zinc-500 transition-colors" 
                                        onclick={() => onEdit(row)}
                                        title="Edit"
                                    >
                                        ✎
                                    </button>
                                {/if}
                                {#if onDelete}
                                    <button 
                                        class="p-1 hover:text-red-400 text-zinc-500 transition-colors" 
                                        onclick={() => onDelete(row)}
                                        title="Delete"
                                    >
                                        🗑
                                    </button>
                                {/if}
                            </div>
                        </td>
                    {/if}
                </tr>
            {/each}
            
            {#if rows.length === 0 && !loading}
                <tr>
                    <td colspan={(columns.length || 1) + (onEdit || onDelete ? 1 : 0)} class="px-4 py-12 text-center text-zinc-500 text-sm">
                        No results found
                    </td>
                </tr>
            {/if}
        </tbody>
    </table>
</div>
