<script lang="ts">
	import { Modal, Button } from '$lib/ui';
	import { executeWithCapability, KernelRequestError } from '$lib/kernel';

	interface Props {
		open: boolean;
		onSuccess?: () => void;
	}

	let { open = $bindable(), onSuccess }: Props = $props();

	let name = $state('');
	let namespace = $state('public');
	let loading = $state(false);
	let error = $state<string | null>(null);

	// Reset form when opening
	$effect(() => {
		if (open) {
			name = '';
			namespace = 'public';
			error = null;
			loading = false;
		}
	});

	async function handleSubmit() {
		if (!name.trim()) return;

		loading = true;
		error = null;

		try {
			await executeWithCapability({
				op: 'schema.create_model',
				resource: { resource_type: '__models', resource_id: null },
				input: {
					name: name.trim(),
					namespace: namespace.trim()
				}
			});

			open = false;
			onSuccess?.();
		} catch (e) {
			console.error('Failed to create model:', e);
			if (e instanceof KernelRequestError) {
				error = e.message;
			} else {
				error = e instanceof Error ? e.message : 'An unexpected error occurred';
			}
		} finally {
			loading = false;
		}
	}
</script>

<Modal bind:open title="Create New Model">
	<div class="space-y-4">
		{#if error}
			<div class="p-3 text-sm text-red-400 bg-red-950/30 border border-red-500/30 rounded-lg">
				{error}
			</div>
		{/if}

		<div class="space-y-1">
			<label for="model-name" class="block text-sm font-medium text-zinc-400">
				Model Name
			</label>
			<input
				id="model-name"
				type="text"
				bind:value={name}
				placeholder="e.g. posts, users, orders"
				class="w-full px-3 py-2 bg-zinc-950 border border-zinc-800 rounded-lg
					focus:outline-none focus:border-indigo-500 focus:ring-1 focus:ring-indigo-500
					placeholder-zinc-600 text-white transition-colors"
				disabled={loading}
				onkeydown={(e) => e.key === 'Enter' && handleSubmit()}
				autofocus
			/>
			<p class="text-xs text-zinc-500">
				Use lowercase letters, numbers, and underscores.
			</p>
		</div>

		<div class="space-y-1">
			<label for="model-namespace" class="block text-sm font-medium text-zinc-400">
				Namespace
			</label>
			<input
				id="model-namespace"
				type="text"
				bind:value={namespace}
				class="w-full px-3 py-2 bg-zinc-950 border border-zinc-800 rounded-lg
					focus:outline-none focus:border-indigo-500 focus:ring-1 focus:ring-indigo-500
					placeholder-zinc-600 text-white transition-colors"
				disabled={loading}
			/>
		</div>
	</div>

	{#snippet footer()}
		<Button variant="secondary" onclick={() => (open = false)} disabled={loading}>
			Cancel
		</Button>
		<Button 
			variant="primary" 
			onclick={handleSubmit} 
			disabled={loading || !name.trim()}
			loading={loading}
		>
			Create Model
		</Button>
	{/snippet}
</Modal>
