<script lang="ts">
	import { Modal, Button } from '$lib/ui';
	import { executeWithCapability, KernelRequestError } from '$lib/kernel';

	interface Props {
		open: boolean;
		model: {
			id: string;
			fields: Array<{ id: string; name: string }>;
		};
		onSuccess?: () => void;
	}

	let { open = $bindable(), model, onSuccess }: Props = $props();

	let name = $state('');
	let selectedFieldIds = $state<string[]>([]);
	let unique = $state(false);
	
	let loading = $state(false);
	let error = $state<string | null>(null);

	// Reset form
	$effect(() => {
		if (open) {
			name = '';
			selectedFieldIds = [];
			unique = false;
			error = null;
			loading = false;
		}
	});

	async function handleSubmit() {
		if (!name.trim() || selectedFieldIds.length === 0) return;

		loading = true;
		error = null;

		try {
			const input = {
				model_id: model.id,
				name: name.trim(),
				fields: selectedFieldIds,
				unique
			};

			await executeWithCapability({
				op: 'schema.create_index',
				resource: { resource_type: '__indexes', resource_id: null },
				input
			}, input);

			open = false;
			onSuccess?.();
		} catch (e) {
			console.error('Failed to create index:', e);
			if (e instanceof KernelRequestError) {
				error = e.message;
			} else {
				error = e instanceof Error ? e.message : 'An unexpected error occurred';
			}
		} finally {
			loading = false;
		}
	}

	function toggleField(id: string) {
		if (selectedFieldIds.includes(id)) {
			selectedFieldIds = selectedFieldIds.filter(f => f !== id);
		} else {
			selectedFieldIds = [...selectedFieldIds, id];
		}
	}
</script>

<Modal bind:open title="Add Index">
	<div class="space-y-4">
		{#if error}
			<div class="p-3 text-sm text-red-400 bg-red-950/30 border border-red-500/30 rounded-lg">
				{error}
			</div>
		{/if}

		<div class="space-y-1">
			<label for="index-name" class="block text-sm font-medium text-zinc-400">
				Index Name
			</label>
			<input
				id="index-name"
				type="text"
				bind:value={name}
				placeholder="e.g. idx_title, unique_email"
				class="w-full px-3 py-2 bg-zinc-950 border border-zinc-800 rounded-lg
					focus:outline-none focus:border-indigo-500 focus:ring-1 focus:ring-indigo-500
					placeholder-zinc-600 text-white transition-colors"
				disabled={loading}
				onkeydown={(e) => e.key === 'Enter' && handleSubmit()}
				autofocus
			/>
		</div>

		<div class="space-y-1">
			<label class="block text-sm font-medium text-zinc-400">
				Select Fields
			</label>
			<div class="max-h-48 overflow-y-auto p-2 bg-zinc-950 border border-zinc-800 rounded-lg space-y-2">
				{#each model.fields as field}
					<label class="flex items-center gap-2 cursor-pointer p-1 rounded hover:bg-zinc-800 transition-colors">
						<input
							type="checkbox"
							checked={selectedFieldIds.includes(field.id)}
							onchange={() => toggleField(field.id)}
							class="w-4 h-4 rounded border-zinc-600 bg-zinc-800 text-indigo-500 focus:ring-indigo-500"
							disabled={loading}
						/>
						<span class="text-sm font-mono text-zinc-300">{field.name}</span>
					</label>
				{/each}
			</div>
		</div>

		<div class="flex gap-4 pt-2">
			<label class="flex items-center gap-2 cursor-pointer">
				<input
					type="checkbox"
					bind:checked={unique}
					class="w-4 h-4 rounded border-zinc-600 bg-zinc-800 text-indigo-500 focus:ring-indigo-500"
					disabled={loading}
				/>
				<span class="text-sm text-zinc-300 font-medium">Unique Index</span>
			</label>
		</div>
	</div>

	{#snippet footer()}
		<Button variant="secondary" onclick={() => (open = false)} disabled={loading}>
			Cancel
		</Button>
		<Button 
			variant="primary" 
			onclick={handleSubmit} 
			disabled={loading || !name.trim() || selectedFieldIds.length === 0}
			loading={loading}
		>
			Create Index
		</Button>
	{/snippet}
</Modal>
