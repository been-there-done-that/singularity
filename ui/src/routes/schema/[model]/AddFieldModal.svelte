<script lang="ts">
	import { Modal, Button } from '$lib/ui';
	import { executeWithCapability, KernelRequestError } from '$lib/kernel';

	interface Props {
		open: boolean;
		modelId: string;
		onSuccess?: () => void;
	}

	let { open = $bindable(), modelId, onSuccess }: Props = $props();

	let name = $state('');
	let type = $state('String');
	let required = $state(false);
	let unique = $state(false);
	let defaultValue = $state('');
	
	let loading = $state(false);
	let error = $state<string | null>(null);

	const TYPES = ['String', 'Int', 'Float', 'Bool', 'Json'];

	// Reset form
	$effect(() => {
		if (open) {
			name = '';
			type = 'String';
			required = false;
			unique = false;
			defaultValue = '';
			error = null;
			loading = false;
		}
	});

	async function handleSubmit() {
		if (!name.trim()) return;

		loading = true;
		error = null;

		try {
			// Parse default value based on type
			let parsedDefault: unknown = null;
			if (defaultValue.trim()) {
				if (type === 'Int') parsedDefault = parseInt(defaultValue);
				else if (type === 'Float') parsedDefault = parseFloat(defaultValue);
				else if (type === 'Bool') parsedDefault =defaultValue === 'true';
				else if (type === 'Json') parsedDefault = JSON.parse(defaultValue);
				else parsedDefault = defaultValue;
			}

			const input = {
				model_id: modelId,
				name: name.trim(),
				field_type: { type },
				required,
				unique,
				default: parsedDefault
			};

			await executeWithCapability({
				op: 'schema.add_field',
				resource: { resource_type: '__fields', resource_id: null },
				input
			}, input);

			open = false;
			onSuccess?.();
		} catch (e) {
			console.error('Failed to add field:', e);
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

<Modal bind:open title="Add Field">
	<div class="space-y-4">
		{#if error}
			<div class="p-3 text-sm text-red-400 bg-red-950/30 border border-red-500/30 rounded-lg">
				{error}
			</div>
		{/if}

		<div class="space-y-1">
			<label for="field-name" class="block text-sm font-medium text-zinc-400">
				Field Name
			</label>
			<input
				id="field-name"
				type="text"
				bind:value={name}
				placeholder="e.g. title, age, content"
				class="w-full px-3 py-2 bg-zinc-950 border border-zinc-800 rounded-lg
					focus:outline-none focus:border-indigo-500 focus:ring-1 focus:ring-indigo-500
					placeholder-zinc-600 text-white transition-colors"
				disabled={loading}
				onkeydown={(e) => e.key === 'Enter' && handleSubmit()}
				autofocus
			/>
		</div>

		<div class="space-y-1">
			<label for="field-type" class="block text-sm font-medium text-zinc-400">
				Type
			</label>
			<select
				id="field-type"
				bind:value={type}
				class="w-full px-3 py-2 bg-zinc-950 border border-zinc-800 rounded-lg
					focus:outline-none focus:border-indigo-500 focus:ring-1 focus:ring-indigo-500
					text-white transition-colors appearance-none"
				disabled={loading}
			>
				{#each TYPES as t}
					<option value={t}>{t}</option>
				{/each}
			</select>
		</div>

		<div class="flex gap-4">
			<label class="flex items-center gap-2 cursor-pointer">
				<input
					type="checkbox"
					bind:checked={required}
					class="w-4 h-4 rounded border-zinc-600 bg-zinc-800 text-indigo-500 focus:ring-indigo-500 focus:ring-offset-zinc-900"
					disabled={loading}
				/>
				<span class="text-sm text-zinc-300">Required</span>
			</label>

			<label class="flex items-center gap-2 cursor-pointer">
				<input
					type="checkbox"
					bind:checked={unique}
					class="w-4 h-4 rounded border-zinc-600 bg-zinc-800 text-indigo-500 focus:ring-indigo-500 focus:ring-offset-zinc-900"
					disabled={loading}
				/>
				<span class="text-sm text-zinc-300">Unique</span>
			</label>
		</div>

		<div class="space-y-1">
			<label for="field-default" class="block text-sm font-medium text-zinc-400">
				Default Value
			</label>
			<input
				id="field-default"
				type="text"
				bind:value={defaultValue}
				placeholder="Optional"
				class="w-full px-3 py-2 bg-zinc-950 border border-zinc-800 rounded-lg
					focus:outline-none focus:border-indigo-500 focus:ring-1 focus:ring-indigo-500
					placeholder-zinc-600 text-white transition-colors"
				disabled={loading}
			/>
			<p class="text-xs text-zinc-500">
				{#if type === 'Json'}
					Must be valid JSON string
				{:else if type === 'Bool'}
					"true" or "false"
				{/if}
			</p>
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
			Add Field
		</Button>
	{/snippet}
</Modal>
