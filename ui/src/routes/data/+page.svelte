<script lang="ts">
    import { requestCapability, execute } from '$lib/kernel/pipeline';
    import ExplorerHeader from '$lib/explorer/ExplorerHeader.svelte';
    import FieldSelector from '$lib/explorer/FieldSelector.svelte';
    import ResultsTable from '$lib/explorer/ResultsTable.svelte';
    import PaginationFooter from '$lib/explorer/PaginationFooter.svelte';
    import type { QueryInput, Row, ExecutionMode } from '$lib/dsl/types';

    // -- State --
    let model = $state<string | null>(null);
    let fields = $state<any[]>([]); // populated by schema.describe
    
    // Canonical query object
    let query = $state<QueryInput>({
        select: [],
        where: null,
        joins: [],
        order_by: [],
        limit: 50,
        offset: 0
    });

    // Execution state
    let rows = $state<Row[]>([]);
    let totalCount = $state(0);
    let loading = $state(false);
    let executionMode = $state<ExecutionMode | null>(null);
    let error = $state<string | null>(null);
    let showDsl = $state(false);

    // -- Actions --

    async function handleModelSelect(newModel: string) {
        if (model === newModel) return;
        model = newModel;
        
        // Reset state
        fields = [];
        query = {
            select: [],
            where: null,
            joins: [],
            order_by: [],
            limit: 50,
            offset: 0
        };
        rows = [];
        totalCount = 0;
        executionMode = null;
        error = null;

        // Fetch fields
        try {
            loading = true;
            const grant = await requestCapability({
                op: 'schema.describe',
                resource: { resource_type: newModel, resource_id: null },
                input: {}
            });
            const schema = await execute<any>(grant.token, {});
            
            // Map schema fields to UI format
            // Assuming schema.describe returns { fields: [...] } structure
            fields = schema.fields.map((f: any) => ({
                name: f.name,
                type: f.type,
                nullable: f.nullable
            }));
            
            // Default select first 5 fields or all if less
            query.select = fields.slice(0, 5).map(f => f.name);
        } catch (e: any) {
            console.error('Failed to describe model:', e);
            error = e.message || 'Failed to describe model';
        } finally {
            loading = false;
        }
    }

    // Effect: Execute Query when relevant state changes
    $effect(() => {
        if (!model || query.select.length === 0) return;
        
        // Debounce slightly? Or just rely on Svelte scheduling.
        // For now direct execution.
        runQuery();
    });

    async function runQuery() {
        if (!model) return;
        
        loading = true;
        error = null;
        
        try {
            // 1. Count (parallel-ish)
            const countReq = requestCapability({
                op: 'data.count',
                resource: { resource_type: model, resource_id: null },
                input: {
                    select: [], // count doesn't select fields
                    where: query.where,
                    joins: query.joins
                }
            }).then(grant => execute<number>(grant.token, {}));

            // 2. Query
            const queryReq = requestCapability({
                op: 'data.query',
                resource: { resource_type: model, resource_id: null },
                input: query
            }).then(async grant => {
                 // In v0 we might want to capture headers for FAST/SLOW mode if exposed
                 // But execute() just returns body. 
                 // We might need to adjust pipeline.ts to return metadata if we want the mode.
                 // For now assume mode is in result or we mock it.
                 // Actually execute() returns T.
                 // Let's assume executeWithMode extension or just T for now.
                 return execute<Row[]>(grant.token, {});
            });

            const [countResult, queryResult] = await Promise.all([countReq, queryReq]);
            
            rows = queryResult;
            totalCount = countResult;
            
            // Determine mode (mock for now until pipeline supports metadata)
            // Or maybe query returns { rows: [], mode: ... } ? No pipeline says generic T.
            // If the kernel returns headers, we need pipeline support.
            executionMode = 'FAST'; // Placeholder
            
        } catch (e: any) {
            console.error('Execution failed:', e);
            error = e.message || 'Execution failed';
            rows = [];
            totalCount = 0;
        } finally {
            loading = false;
        }
    }
</script>

<div class="h-screen flex flex-col bg-black text-zinc-300 font-sans overflow-hidden">
    <!-- Header -->
    <ExplorerHeader 
        {model}
        mode={executionMode}
        {showDsl}
        onModelSelect={handleModelSelect}
        onToggleDsl={() => showDsl = !showDsl}
    />

    {#if !model}
        <div class="flex-1 flex items-center justify-center text-zinc-500">
            Select a model to begin exploration
        </div>
    {:else}
        <!-- Workspace -->
        <div class="flex-1 flex overflow-hidden">
            <!-- Left Sidebar -->
            <div class="w-64 flex flex-col border-r border-zinc-800 bg-zinc-900/30">
                <FieldSelector
                    {fields}
                    selected={query.select}
                    onSelect={cols => query.select = cols}
                />
            </div>
            
            <!-- Main Content -->
            <div class="flex-1 flex flex-col min-w-0">
                <!-- Filters Bar (Placeholder) -->
                <div class="h-12 border-b border-zinc-800 flex items-center px-4 gap-4 bg-zinc-900/30">
                   <div class="text-xs text-zinc-500 italic">Filter builder coming in v0.2</div>
                </div>

                <!-- Results -->
                <ResultsTable
                    {rows}
                    columns={query.select}
                    {loading}
                />
                
                <!-- Footer -->
                <PaginationFooter
                    limit={query.limit || 50}
                    offset={query.offset || 0}
                    total={totalCount}
                    onChange={(lim, off) => {
                        query.limit = lim;
                        query.offset = off;
                    }}
                />
            </div>
            
            <!-- DSL Preview -->
            {#if showDsl}
                <div class="w-80 border-l border-zinc-800 bg-zinc-950 p-4 font-mono text-xs overflow-auto">
                    <div class="text-zinc-500 mb-2 uppercase tracking-wider font-semibold">DSL Inspector</div>
                    <pre class="text-emerald-400 whitespace-pre-wrap">{JSON.stringify(query, null, 2)}</pre>
                </div>
            {/if}
        </div>
    {/if}

    {#if error}
        <div class="fixed bottom-4 right-4 bg-red-900/90 text-white px-4 py-3 rounded-lg shadow-lg max-w-md backdrop-blur border border-red-700 z-50">
            <div class="font-bold text-sm mb-1">Execution Error</div>
            <div class="text-xs opacity-90">{error}</div>
            <button class="absolute top-1 right-1 p-1 hover:bg-red-800 rounded" onclick={() => error = null}>✕</button>
        </div>
    {/if}
</div>
