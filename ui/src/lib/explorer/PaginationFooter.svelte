<script lang="ts">
    interface Props {
        limit: number;
        offset: number;
        total: number;
        onChange: (limit: number, offset: number) => void;
    }

    let { limit, offset, total, onChange }: Props = $props();

    let currentPage = $derived(Math.floor(offset / limit) + 1);
    let totalPages = $derived(Math.max(1, Math.ceil(total / limit)));
    
    // Derived range display
    let startItem = $derived(total === 0 ? 0 : offset + 1);
    let endItem = $derived(Math.min(offset + limit, total));

    function goToPage(page: number) {
        if (page < 1 || page > totalPages) return;
        onChange(limit, (page - 1) * limit);
    }
</script>

<div class="flex items-center justify-between px-4 py-2 border-t border-zinc-800 bg-zinc-900 text-sm">
    <div class="text-zinc-500">
        Showing <span class="text-zinc-300 font-medium">{startItem}</span> to <span class="text-zinc-300 font-medium">{endItem}</span> of <span class="text-zinc-300 font-medium">{total}</span>
    </div>

    <div class="flex items-center gap-2">
        <button 
            disabled={currentPage === 1}
            onclick={() => goToPage(currentPage - 1)}
            class="px-2 py-1 rounded border border-zinc-700 hover:bg-zinc-800 disabled:opacity-50 disabled:cursor-not-allowed text-zinc-400"
        >
            Previous
        </button>
        
        <span class="text-zinc-400 font-mono px-2">
            Page {currentPage} / {totalPages}
        </span>
        
        <button 
            disabled={currentPage === totalPages}
            onclick={() => goToPage(currentPage + 1)}
            class="px-2 py-1 rounded border border-zinc-700 hover:bg-zinc-800 disabled:opacity-50 disabled:cursor-not-allowed text-zinc-400"
        >
            Next
        </button>
        
        <select 
            value={limit}
            onchange={(e) => onChange(Number(e.currentTarget.value), 0)}
            class="ml-4 bg-zinc-900 border border-zinc-700 rounded text-xs text-zinc-300 px-2 py-1"
        >
            <option value={10}>10 / page</option>
            <option value={20}>20 / page</option>
            <option value={50}>50 / page</option>
            <option value={100}>100 / page</option>
        </select>
    </div>
</div>
