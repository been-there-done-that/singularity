<script lang="ts">
	/**
	 * Toast Notification System
	 * 
	 * For displaying:
	 * - Capability denial messages
	 * - Network errors
	 * - Success confirmations
	 * 
	 * Rule: Never show raw backend errors - translate them.
	 */

	import { toast } from '$lib/state/toast.svelte';

	const typeStyles = {
		success: 'bg-emerald-500/10 border-emerald-500/30 text-emerald-400',
		error: 'bg-red-500/10 border-red-500/30 text-red-400',
		warning: 'bg-amber-500/10 border-amber-500/30 text-amber-400',
		info: 'bg-blue-500/10 border-blue-500/30 text-blue-400'
	};

	const typeIcons = {
		success: `<path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M5 13l4 4L19 7" />`,
		error: `<path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M6 18L18 6M6 6l12 12" />`,
		warning: `<path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M12 9v2m0 4h.01m-6.938 4h13.856c1.54 0 2.502-1.667 1.732-3L13.732 4c-.77-1.333-2.694-1.333-3.464 0L3.34 16c-.77 1.333.192 3 1.732 3z" />`,
		info: `<path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M13 16h-1v-4h-1m1-4h.01M21 12a9 9 0 11-18 0 9 9 0 0118 0z" />`
	};
</script>

<!-- Toast Container -->
<div class="fixed bottom-4 right-4 z-50 flex flex-col gap-2 pointer-events-none">
	{#each toast.toasts as item (item.id)}
		<div
			class="pointer-events-auto min-w-72 max-w-md px-4 py-3 border rounded-lg shadow-lg backdrop-blur-sm flex items-start gap-3 animate-in slide-in-from-right-5 duration-300 {typeStyles[
				item.type
			]}"
			role="alert"
		>
			<svg
				class="shrink-0 w-5 h-5 mt-0.5"
				fill="none"
				stroke="currentColor"
				viewBox="0 0 24 24"
			>
				{@html typeIcons[item.type]}
			</svg>
			<p class="text-sm flex-1">{item.message}</p>
			<button
				onclick={() => toast.remove(item.id)}
				class="shrink-0 opacity-60 hover:opacity-100 transition-opacity"
				aria-label="Dismiss"
			>
				<svg class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
					<path
						stroke-linecap="round"
						stroke-linejoin="round"
						stroke-width="2"
						d="M6 18L18 6M6 6l12 12"
					/>
				</svg>
			</button>
		</div>
	{/each}
</div>
