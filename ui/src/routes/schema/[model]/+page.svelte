<script lang="ts">
	/**
	 * Model Detail - Schema Introspection
	 * 
	 * Shows model fields, types, and constraints.
	 * Fetches real data from the kernel.
	 */

	import { page } from '$app/state';
	import { Button, DangerZone, EmptyState } from '$lib/ui';
	import { executeWithCapability, KernelRequestError } from '$lib/kernel';

	const modelName = $derived(page.params.model ?? '');

	// System fields managed by the kernel
	const SYSTEM_FIELDS = ['id', 'owner_id', 'created_at', 'updated_at'];

	// Model data from kernel
	let model = $state<{
		id: string;
		name: string;
		namespace: string;
		createdAt: number;
		ownership: { column: string; principal: string } | null;
		fields: Array<{
			name: string;
			type: string;
			required: boolean;
			unique: boolean;
			default: string | null;
			isSystem: boolean;
		}>;
	} | null>(null);

	let loading = $state(true);
	let error = $state<string | null>(null);

	$effect(() => {
		if (modelName) {
			loadModel();
		}
	});

	async function loadModel() {
		loading = true;
		error = null;

		try {
			// 1. Fetch model by name from __models
			const models = await executeWithCapability<Record<string, unknown>[]>(
				{
					op: 'resource.read',
					resource: { resource_type: '__models', resource_id: null },
					input: { name: modelName }
				}
			);

			// Handle empty result
			if (!Array.isArray(models) || models.length === 0) {
				model = null;
				loading = false;
				return;
			}

			const modelData = models[0];
			const modelId = String(modelData.id);

			// 2. Fetch fields for this model from __fields
			const fields = await executeWithCapability<Record<string, unknown>[]>(
				{
					op: 'resource.read',
					resource: { resource_type: '__fields', resource_id: null },
					input: { model_id: modelId }
				}
			);

			const fieldList = Array.isArray(fields) ? fields : [];

			// 3. Map kernel response to UI model
			model = {
				id: modelId,
				name: String(modelData.name || modelName),
				namespace: String(modelData.namespace || 'default'),
				createdAt: Number(modelData.created_at) || 0,
				ownership: modelData.ownership as { column: string; principal: string } | null,
				fields: fieldList.map((f: Record<string, unknown>) => ({
					name: String(f.name || ''),
					type: String(f.field_type || 'text'),
					required: Boolean(f.required),
					unique: Boolean(f.unique_flag),
					default: f.default_val ? String(f.default_val) : null,
					// TODO: Use kernel metadata (f.is_system) when available
					isSystem: SYSTEM_FIELDS.includes(String(f.name || ''))
				}))
			};
		} catch (e) {
			if (e instanceof KernelRequestError) {
				error = e.code === 'POLICY_DENIED' 
					? 'You do not have permission to view this model'
					: e.message;
			} else {
				error = e instanceof Error ? e.message : 'Failed to load model';
			}
		} finally {
			loading = false;
		}
	}
</script>

<div class="max-w-4xl mx-auto">
	<!-- Back link -->
	<a
		href="/schema"
		class="inline-flex items-center gap-2 text-sm text-zinc-400 hover:text-white transition-colors mb-6"
	>
		<svg class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
			<path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M15 19l-7-7 7-7" />
		</svg>
		Back to Schema
	</a>

	{#if loading}
		<div class="animate-pulse">
			<div class="h-8 w-48 bg-zinc-800 rounded mb-4"></div>
			<div class="h-4 w-64 bg-zinc-800 rounded mb-8"></div>
			<div class="bg-zinc-900 border border-zinc-800 rounded-xl p-6">
				<div class="space-y-4">
					{#each [1, 2, 3, 4] as _}
						<div class="h-12 bg-zinc-800 rounded"></div>
					{/each}
				</div>
			</div>
		</div>
	{:else if error}
		<!-- Error state -->
		<div class="bg-red-950/30 border border-red-500/30 rounded-xl p-6">
			<p class="text-red-400 mb-4">{error}</p>
			<Button variant="secondary" onclick={loadModel}>Retry</Button>
		</div>
	{:else if model}
		<!-- Header -->
		<div class="flex items-center justify-between mb-8">
			<div>
				<div class="flex items-center gap-3">
					<h1 class="text-3xl font-bold text-white">{model.name}</h1>
					{#if model.ownership}
						<span class="px-2 py-0.5 text-xs font-medium bg-indigo-500/10 text-indigo-400 rounded">
							owned
						</span>
					{/if}
				</div>
				<p class="mt-2 text-zinc-400">
					{model.namespace} • Created {new Date(model.createdAt * 1000).toLocaleDateString()}
				</p>
			</div>
		</div>

		<!-- Ownership Info -->
		{#if model.ownership}
			<div class="bg-indigo-500/5 border border-indigo-500/20 rounded-xl p-4 mb-6">
				<div class="flex items-center gap-2 text-sm">
					<svg class="w-4 h-4 text-indigo-400" fill="currentColor" viewBox="0 0 24 24">
						<path d="M12 1L3 5v6c0 5.55 3.84 10.74 9 12 5.16-1.26 9-6.45 9-12V5l-9-4z" />
					</svg>
					<span class="text-zinc-300">
						Ownership enforced via <code class="text-indigo-400">{model.ownership.column}</code>
					</span>
				</div>
			</div>
		{/if}

		<!-- Fields Table -->
		<div class="bg-zinc-900 border border-zinc-800 rounded-xl overflow-hidden mb-8">
			<div class="px-6 py-4 border-b border-zinc-800">
				<h2 class="text-lg font-semibold text-white">Fields</h2>
			</div>

			{#if model.fields.length === 0}
				<div class="p-6">
					<EmptyState
						title="No fields"
						description="This model has no fields defined."
						icon="folder"
					/>
				</div>
			{:else}
				<div class="overflow-x-auto">
					<table class="w-full">
						<thead class="bg-zinc-800/50">
							<tr>
								<th class="px-6 py-3 text-left text-xs font-medium text-zinc-400 uppercase">Name</th>
								<th class="px-6 py-3 text-left text-xs font-medium text-zinc-400 uppercase">Type</th>
								<th class="px-6 py-3 text-left text-xs font-medium text-zinc-400 uppercase">Required</th>
								<th class="px-6 py-3 text-left text-xs font-medium text-zinc-400 uppercase">Unique</th>
								<th class="px-6 py-3 text-left text-xs font-medium text-zinc-400 uppercase">Default</th>
							</tr>
						</thead>
						<tbody class="divide-y divide-zinc-800">
							{#each model.fields as field}
								<tr class="hover:bg-zinc-800/30 transition-colors">
									<td class="px-6 py-4">
										<div class="flex items-center gap-2">
											<code class="text-sm font-mono text-indigo-400">{field.name}</code>
											{#if field.isSystem}
												<span class="px-1.5 py-0.5 text-xs bg-zinc-800 text-zinc-500 rounded">
													system
												</span>
											{/if}
										</div>
									</td>
									<td class="px-6 py-4">
										<span
											class="px-2 py-0.5 text-xs font-medium bg-zinc-800 text-zinc-300 rounded font-mono"
										>
											{field.type}
										</span>
									</td>
									<td class="px-6 py-4">
										{#if field.required}
											<span class="text-emerald-400">✓</span>
										{:else}
											<span class="text-zinc-600">—</span>
										{/if}
									</td>
									<td class="px-6 py-4">
										{#if field.unique}
											<span class="text-emerald-400">✓</span>
										{:else}
											<span class="text-zinc-600">—</span>
										{/if}
									</td>
									<td class="px-6 py-4">
										{#if field.default}
											<code class="text-xs font-mono text-zinc-400">{field.default}</code>
										{:else}
											<span class="text-zinc-600">—</span>
										{/if}
									</td>
								</tr>
							{/each}
						</tbody>
					</table>
				</div>
			{/if}
		</div>

		<!-- Danger Zone (read-only for now) -->
		<div class="opacity-50 pointer-events-none">
			<DangerZone
				title="Drop Model"
				description="Permanently delete this model and all its data. This cannot be undone."
				confirmText="Delete Model"
				onconfirm={() => console.log('Delete model')}
			/>
		</div>
	{:else}
		<EmptyState 
			title="Model not found" 
			description="This model does not exist or you don't have access to it." 
			icon="lock" 
		/>
	{/if}
</div>
