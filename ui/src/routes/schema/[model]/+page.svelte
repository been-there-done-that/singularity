<script lang="ts">
	/**
	 * Model Detail - Schema Introspection
	 * 
	 * Shows model fields, types, and constraints.
	 * Only accessible for owned models.
	 */

	import { page } from '$app/state';
	import { Button, DangerZone, EmptyState } from '$lib/ui';

	const modelName = $derived(page.params.model ?? '');

	// Mock model data - will come from kernel
	let model = $state<{
		name: string;
		owner: string;
		createdAt: string;
		fields: Array<{
			name: string;
			type: string;
			required: boolean;
			unique: boolean;
			default: string | null;
		}>;
	} | null>(null);

	let loading = $state(true);

	$effect(() => {
		loadModel();
	});

	async function loadModel() {
		loading = true;
		await new Promise((r) => setTimeout(r, 300));

		// Mock data
		model = {
			name: modelName,
			owner: 'dev-user',
			createdAt: '2024-01-15T10:30:00Z',
			fields: [
				{ name: 'id', type: 'uuid', required: true, unique: true, default: 'gen_random_uuid()' },
				{ name: 'title', type: 'string', required: true, unique: false, default: null },
				{ name: 'completed', type: 'boolean', required: true, unique: false, default: 'false' },
				{ name: 'owner_id', type: 'uuid', required: true, unique: false, default: null },
				{ name: 'created_at', type: 'timestamp', required: true, unique: false, default: 'now()' }
			]
		};

		loading = false;
	}

	async function handleDropField(fieldName: string) {
		console.log('Drop field:', fieldName);
		// TODO: Call kernel schema.drop_field
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
	{:else if model}
		<!-- Header -->
		<div class="flex items-center justify-between mb-8">
			<div>
				<h1 class="text-3xl font-bold text-white">{model.name}</h1>
				<p class="mt-2 text-zinc-400">
					Created {new Date(model.createdAt).toLocaleDateString()}
				</p>
			</div>
			<Button variant="primary">+ Add Field</Button>
		</div>

		<!-- Fields Table -->
		<div class="bg-zinc-900 border border-zinc-800 rounded-xl overflow-hidden mb-8">
			<div class="px-6 py-4 border-b border-zinc-800">
				<h2 class="text-lg font-semibold text-white">Fields</h2>
			</div>

			<div class="overflow-x-auto">
				<table class="w-full">
					<thead class="bg-zinc-800/50">
						<tr>
							<th class="px-6 py-3 text-left text-xs font-medium text-zinc-400 uppercase">Name</th>
							<th class="px-6 py-3 text-left text-xs font-medium text-zinc-400 uppercase">Type</th>
							<th class="px-6 py-3 text-left text-xs font-medium text-zinc-400 uppercase"
								>Required</th
							>
							<th class="px-6 py-3 text-left text-xs font-medium text-zinc-400 uppercase">Unique</th
							>
							<th class="px-6 py-3 text-left text-xs font-medium text-zinc-400 uppercase"
								>Default</th
							>
						</tr>
					</thead>
					<tbody class="divide-y divide-zinc-800">
						{#each model.fields as field}
							<tr class="hover:bg-zinc-800/30 transition-colors">
								<td class="px-6 py-4">
									<code class="text-sm font-mono text-indigo-400">{field.name}</code>
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
		</div>

		<!-- Danger Zone -->
		<DangerZone
			title="Drop Model"
			description="Permanently delete this model and all its data. This cannot be undone."
			confirmText="Delete Model"
			onconfirm={() => console.log('Delete model')}
		/>
	{:else}
		<EmptyState title="Model not found" description="This model does not exist or you don't have access to it." icon="lock" />
	{/if}
</div>
