<script lang="ts">
	/**
	 * Schema List - Model Overview
	 * 
	 * Admin-only view showing all models with ownership indicators.
	 * Non-admins see an empty state (not a 403).
	 */

	import { Button, EmptyState } from '$lib/ui';
	import { auth } from '$lib/state/auth.svelte';
	import { executeWithCapability } from '$lib/kernel';

	// Loading state
	let loading = $state(true);
	let error = $state<string | null>(null);

	// Models from kernel (mock for now)
	let models = $state<
		Array<{
			name: string;
			owner: string;
			isOwned: boolean;
			isSystem: boolean;
			fieldCount: number;
			recordCount: number;
		}>
	>([]);

	// Load models on mount
	$effect(() => {
		loadModels();
	});

	async function loadModels() {
		loading = true;
		error = null;

		try {
			// TODO: Replace with actual kernel call
			// const result = await executeWithCapability({
			//   op: 'schema.list_models',
			//   resource: { resource_type: '__models', resource_id: '*' },
			//   input: {}
			// });

			// Mock data for UI development
			await new Promise((r) => setTimeout(r, 500));
			models = [
				{
					name: 'todos',
					owner: 'dev-user',
					isOwned: true,
					isSystem: false,
					fieldCount: 5,
					recordCount: 42
				},
				{
					name: 'invoices',
					owner: 'dev-user',
					isOwned: true,
					isSystem: false,
					fieldCount: 8,
					recordCount: 156
				},
				{
					name: 'users',
					owner: 'system',
					isOwned: false,
					isSystem: true,
					fieldCount: 6,
					recordCount: 3
				}
			];
		} catch (e) {
			error = e instanceof Error ? e.message : 'Failed to load models';
		} finally {
			loading = false;
		}
	}
</script>

<div class="max-w-6xl mx-auto">
	<!-- Header -->
	<div class="flex items-center justify-between mb-8">
		<div>
			<h1 class="text-3xl font-bold text-white">Schema</h1>
			<p class="mt-2 text-zinc-400">Manage your data models</p>
		</div>
			{#if auth.isAdmin}
			<Button variant="primary">+ Create Model</Button>
		{/if}
	</div>

	{#if !auth.isAdmin}
		<!-- Non-admin empty state -->
		<div class="bg-zinc-900 border border-zinc-800 rounded-xl">
			<EmptyState
				title="You don't own any schemas"
				description="Schema management is available to administrators."
				icon="lock"
			/>
		</div>
	{:else if loading}
		<!-- Loading skeleton -->
		<div class="space-y-4">
			{#each [1, 2, 3] as _}
				<div class="bg-zinc-900 border border-zinc-800 rounded-xl p-6 animate-pulse">
					<div class="h-5 w-32 bg-zinc-800 rounded mb-3"></div>
					<div class="h-4 w-48 bg-zinc-800 rounded"></div>
				</div>
			{/each}
		</div>
	{:else if error}
		<!-- Error state -->
		<div class="bg-red-950/30 border border-red-500/30 rounded-xl p-6">
			<p class="text-red-400">{error}</p>
			<Button variant="secondary" onclick={loadModels}>Retry</Button>
		</div>
	{:else if models.length === 0}
		<!-- No models -->
		<div class="bg-zinc-900 border border-zinc-800 rounded-xl">
			<EmptyState
				title="No models yet"
				description="Create your first model to start building your schema."
				icon="folder"
			/>
		</div>
	{:else}
		<!-- Model List -->
		<div class="space-y-4">
			{#each models as model}
				<a
					href={model.isOwned ? `/schema/${model.name}` : '#'}
					class="block bg-zinc-900 border border-zinc-800 rounded-xl p-6 transition-all
						{model.isOwned
						? 'hover:border-zinc-700 cursor-pointer'
						: 'opacity-60 cursor-not-allowed'}"
				>
					<div class="flex items-center justify-between">
						<div class="flex items-center gap-4">
							<!-- Ownership indicator -->
							<div
								class="w-10 h-10 rounded-lg flex items-center justify-center
								{model.isSystem
									? 'bg-zinc-800'
									: model.isOwned
										? 'bg-indigo-500/10'
										: 'bg-zinc-800'}"
							>
								{#if model.isSystem}
									<svg class="w-5 h-5 text-zinc-500" fill="currentColor" viewBox="0 0 24 24">
										<path
											d="M12 1L3 5v6c0 5.55 3.84 10.74 9 12 5.16-1.26 9-6.45 9-12V5l-9-4z"
										/>
									</svg>
								{:else if model.isOwned}
									<svg class="w-5 h-5 text-indigo-400" fill="currentColor" viewBox="0 0 24 24">
										<path d="M9 16.17L4.83 12l-1.42 1.41L9 19 21 7l-1.41-1.41L9 16.17z" />
									</svg>
								{:else}
									<svg class="w-5 h-5 text-zinc-500" fill="currentColor" viewBox="0 0 24 24">
										<path
											d="M18 8h-1V6c0-2.76-2.24-5-5-5S7 3.24 7 6v2H6c-1.1 0-2 .9-2 2v10c0 1.1.9 2 2 2h12c1.1 0 2-.9 2-2V10c0-1.1-.9-2-2-2zM9 6c0-1.66 1.34-3 3-3s3 1.34 3 3v2H9V6z"
										/>
									</svg>
								{/if}
							</div>

							<div>
								<div class="flex items-center gap-2">
									<h3 class="text-lg font-semibold text-white">{model.name}</h3>
									{#if model.isSystem}
										<span
											class="px-2 py-0.5 text-xs font-medium bg-zinc-800 text-zinc-400 rounded"
										>
											system
										</span>
									{/if}
								</div>
								<p class="text-sm text-zinc-400">
									{model.fieldCount} fields · {model.recordCount} records
								</p>
							</div>
						</div>

						{#if model.isOwned}
							<svg class="w-5 h-5 text-zinc-500" fill="none" stroke="currentColor" viewBox="0 0 24 24">
								<path
									stroke-linecap="round"
									stroke-linejoin="round"
									stroke-width="2"
									d="M9 5l7 7-7 7"
								/>
							</svg>
						{/if}
					</div>
				</a>
			{/each}
		</div>
	{/if}
</div>
