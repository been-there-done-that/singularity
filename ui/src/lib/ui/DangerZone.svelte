<script lang="ts">
	import type { Snippet } from 'svelte';
	import Button from './Button.svelte';

	interface Props {
		title: string;
		description?: string;
		confirmText?: string;
		confirmCheckbox?: string;
		loading?: boolean;
		onconfirm: () => void | Promise<void>;
		children?: Snippet;
	}

	let {
		title,
		description,
		confirmText = 'Delete',
		confirmCheckbox = 'I understand this action is irreversible',
		loading = false,
		onconfirm,
		children
	}: Props = $props();

	let confirmed = $state(false);
	let isExecuting = $state(false);

	async function handleConfirm() {
		if (!confirmed || isExecuting) return;

		isExecuting = true;
		try {
			await onconfirm();
		} finally {
			isExecuting = false;
			confirmed = false;
		}
	}
</script>

<div class="mt-8 border-2 border-red-500/30 rounded-xl bg-red-950/20 p-6">
	<div class="flex items-start gap-3">
		<div class="flex-shrink-0">
			<svg
				class="w-6 h-6 text-red-500"
				fill="none"
				stroke="currentColor"
				viewBox="0 0 24 24"
				xmlns="http://www.w3.org/2000/svg"
			>
				<path
					stroke-linecap="round"
					stroke-linejoin="round"
					stroke-width="2"
					d="M12 9v2m0 4h.01m-6.938 4h13.856c1.54 0 2.502-1.667 1.732-3L13.732 4c-.77-1.333-2.694-1.333-3.464 0L3.34 16c-.77 1.333.192 3 1.732 3z"
				></path>
			</svg>
		</div>
		<div class="flex-1">
			<h3 class="text-lg font-semibold text-red-400">{title}</h3>
			{#if description}
				<p class="mt-1 text-sm text-zinc-400">{description}</p>
			{/if}
			{#if children}
				<div class="mt-3">
					{@render children()}
				</div>
			{/if}

			<label class="mt-4 flex items-center gap-2 cursor-pointer">
				<input
					type="checkbox"
					bind:checked={confirmed}
					class="w-4 h-4 rounded border-zinc-600 bg-zinc-800 text-red-500 focus:ring-red-500 focus:ring-offset-zinc-900"
				/>
				<span class="text-sm text-zinc-300">{confirmCheckbox}</span>
			</label>

			<div class="mt-4">
				<Button
					variant="danger"
					disabled={!confirmed}
					loading={loading || isExecuting}
					onclick={handleConfirm}
				>
					{confirmText}
				</Button>
			</div>
		</div>
	</div>
</div>
