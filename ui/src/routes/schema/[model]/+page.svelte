<script lang="ts">
	/**
	 * Model Detail - Schema Introspection
	 * 
	 * Shows model fields, types, and constraints.
	 * Fetches real data from the kernel.
	 */

	import { page } from '$app/state';
	import { goto } from '$app/navigation';
	import { auth } from '$lib/state/auth.svelte';
	import { Button, DangerZone, EmptyState } from '$lib/ui';
	import { executeWithCapability, KernelRequestError } from '$lib/kernel';
	import AddFieldModal from './AddFieldModal.svelte';
	import AddIndexModal from './AddIndexModal.svelte';

	const modelName = $derived(page.params.model ?? '');

	// System fields managed by the kernel
	const SYSTEM_FIELDS = ['id', 'owner_id', 'created_at', 'updated_at'];

	// Model data from kernel
	let model = $state<{
		id: string;
		name: string;
		namespace: string;
		createdAt: number;
		isOwned: boolean;
		fields: Array<{
			id: string;
			name: string;
			type: string;
			required: boolean;
			unique: boolean;
			default: string | null;
			isSystem: boolean;
		}>;
		indexes: Array<{
			id: string;
			name: string;
			fields: string[];
			unique: boolean;
			createdAt: number;
		}>;
	} | null>(null);

	let loading = $state(true);
	let error = $state<string | null>(null);
	let isAddOpen = $state(false);
	let isAddIndexOpen = $state(false);

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

			// 2.5 Fetch indexes for this model from __indexes
			const indexes = await executeWithCapability<Record<string, unknown>[]>(
				{
					op: 'resource.read',
					resource: { resource_type: '__indexes', resource_id: null },
					input: { model_id: modelId }
				}
			);

			const indexList = Array.isArray(indexes) ? indexes : [];

			// 3. Map kernel response to UI model
			model = {
				id: modelId,
				name: String(modelData.name || modelName),
				namespace: String(modelData.namespace || 'default'),
				createdAt: Number(modelData.created_at) || 0,
				isOwned: modelData.owner_id === auth.subject?.id,
				fields: fieldList.map((f: Record<string, unknown>) => {
					const ft = f.field_type as any;
					const typeName = (typeof ft === 'object' && ft !== null && ft.type) 
						? String(ft.type) 
						: String(f.field_type || 'text');

					return {
						id: String(f.id || ''),
						name: String(f.name || ''),
						type: typeName,
						required: Boolean(f.required),
						unique: Boolean(f.unique_flag),
						default: f.default_val ? String(f.default_val) : null,
						// TODO: Use kernel metadata (f.is_system) when available
						isSystem: SYSTEM_FIELDS.includes(String(f.name || ''))
					};
				}),
				indexes: indexList.map((idx: any) => ({
					id: String(idx.id),
					name: String(idx.name),
					fields: Array.isArray(idx.fields) ? idx.fields.map(String) : [],
					unique: Boolean(idx.unique),
					createdAt: Number(idx.created_at) || 0
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

	async function handleDeleteModel() {
		if (!model) return;

		try {
			await executeWithCapability({
				op: 'resource.delete',
				resource: { resource_type: '__models', resource_id: model.id },
				input: {}
			});

			// Navigate back to list on success
			goto('/schema');
		} catch (e) {
			if (e instanceof KernelRequestError) {
				error = e.message;
			} else {
				error = e instanceof Error ? e.message : 'Failed to delete model';
			}
		}
	}

	async function handleDeleteField(fieldName: string) {
		if (!model || !confirm(`Are you sure you want to delete field "${fieldName}"? This action cannot be undone.`)) return;

		try {
			await executeWithCapability({
				op: 'schema.drop_field',
				resource: { resource_type: '__fields', resource_id: null },
				input: {
					model_id: model.id,
					name: fieldName
				}
			}, {
				model_id: model.id,
				name: fieldName
			});

			await loadModel();
		} catch (e) {
			console.error('Failed to delete field:', e);
			if (e instanceof KernelRequestError) {
				error = e.message;
			} else {
				error = e instanceof Error ? e.message : 'Failed to delete field';
			}
		}
	}

	async function handleDeleteIndex(indexId: string) {
		if (!model || !confirm(`Are you sure you want to delete this index?`)) return;

		try {
			await executeWithCapability({
				op: 'schema.drop_index',
				resource: { resource_type: '__indexes', resource_id: indexId },
				input: {}
			});

			await loadModel();
		} catch (e) {
			console.error('Failed to delete index:', e);
			if (e instanceof KernelRequestError) {
				error = e.message;
			} else {
				error = e instanceof Error ? e.message : 'Failed to delete index';
			}
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
					{#if model.isOwned}
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


		<!-- Fields Table -->
		<div class="bg-zinc-900 border border-zinc-800 rounded-xl overflow-hidden mb-8">
			<div class="px-6 py-4 border-b border-zinc-800 flex items-center justify-between">
				<h2 class="text-lg font-semibold text-white">Fields</h2>
				<Button size="sm" onclick={() => isAddOpen = true}>+ Add Field</Button>
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
								<th class="px-6 py-3 text-right text-xs font-medium text-zinc-400 uppercase">Actions</th>
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
									<td class="px-6 py-4 text-right">
										{#if !field.isSystem}
											<button 
												onclick={() => handleDeleteField(field.name)}
												class="text-zinc-500 hover:text-red-400 transition-colors"
												title="Delete field"
											>
												<svg class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
													<path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M19 7l-.867 12.142A2 2 0 0116.138 21H7.862a2 2 0 01-1.995-1.858L5 7m5 4v6m4-6v6m1-10V4a1 1 0 00-1-1h-4a1 1 0 00-1 1v3M4 7h16" />
												</svg>
											</button>
										{/if}
									</td>
								</tr>
							{/each}
						</tbody>
					</table>
				</div>
			{/if}
		</div>

		<!-- Danger Zone -->
		<div class="mt-8">
			<DangerZone
				title="Drop Model"
				description="Permanently delete this model and all its data. This cannot be undone."
				confirmText="Delete Model"
				onconfirm={handleDeleteModel}
			/>
		</div>

		<!-- Indexes Table -->
		<div class="bg-zinc-900 border border-zinc-800 rounded-xl overflow-hidden mt-8">
			<div class="px-6 py-4 border-b border-zinc-800 flex items-center justify-between">
				<h2 class="text-lg font-semibold text-white">Indexes</h2>
				<Button size="sm" onclick={() => isAddIndexOpen = true}>+ Add Index</Button>
			</div>

			{#if model.indexes.length === 0}
				<div class="p-6">
					<EmptyState
						title="No indexes"
						description="This model has no custom indexes."
						icon="search"
					/>
				</div>
			{:else}
				<div class="overflow-x-auto">
					<table class="w-full">
						<thead class="bg-zinc-800/50">
							<tr>
								<th class="px-6 py-3 text-left text-xs font-medium text-zinc-400 uppercase">Name</th>
								<th class="px-6 py-3 text-left text-xs font-medium text-zinc-400 uppercase">Fields</th>
								<th class="px-6 py-3 text-left text-xs font-medium text-zinc-400 uppercase">Unique</th>
								<th class="px-6 py-3 text-right text-xs font-medium text-zinc-400 uppercase">Actions</th>
							</tr>
						</thead>
						<tbody class="divide-y divide-zinc-800">
							{#each model.indexes as index}
								<tr class="hover:bg-zinc-800/30 transition-colors">
									<td class="px-6 py-4">
										<code class="text-sm font-mono text-zinc-300">{index.name}</code>
									</td>
									<td class="px-6 py-4">
										<div class="flex flex-wrap gap-1">
											{#each index.fields as fieldId}
												<span class="px-1.5 py-0.5 text-xs bg-zinc-800 text-indigo-400 rounded font-mono">
													{model.fields.find(f => f.id === fieldId)?.name || fieldId.split('-').pop()}
												</span>
											{/each}
										</div>
									</td>
									<td class="px-6 py-4">
										{#if index.unique}
											<span class="text-emerald-400">✓</span>
										{:else}
											<span class="text-zinc-600">—</span>
										{/if}
									</td>
									<td class="px-6 py-4 text-right">
										<button 
											onclick={() => handleDeleteIndex(index.id)}
											class="text-zinc-500 hover:text-red-400 transition-colors"
											title="Delete index"
										>
											<svg class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
												<path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M19 7l-.867 12.142A2 2 0 0116.138 21H7.862a2 2 0 01-1.995-1.858L5 7m5 4v6m4-6v6m1-10V4a1 1 0 00-1-1h-4a1 1 0 00-1 1v3M4 7h16" />
											</svg>
										</button>
									</td>
								</tr>
							{/each}
						</tbody>
					</table>
				</div>
			{/if}
		</div>

		<AddFieldModal bind:open={isAddOpen} modelId={model.id} onSuccess={loadModel} />
		<AddIndexModal bind:open={isAddIndexOpen} model={model} onSuccess={loadModel} />
	{:else}
		<EmptyState 
			title="Model not found" 
			description="This model does not exist or you don't have access to it." 
			icon="lock" 
		/>
	{/if}
</div>
