<script lang="ts">
	import type { Snippet } from 'svelte';

	interface Props {
		open: boolean;
		onclose?: () => void;
		title?: string;
		header?: Snippet;
		children: Snippet;
		footer?: Snippet;
	}

	let { open = $bindable(), onclose, title, header, children, footer }: Props = $props();

	function handleKeydown(e: KeyboardEvent) {
		if (e.key === 'Escape' && open) {
			close();
		}
	}

	function close() {
		open = false;
		onclose?.();
	}

	function handleBackdropClick(e: MouseEvent) {
		if (e.target === e.currentTarget) {
			close();
		}
	}
</script>

<svelte:window onkeydown={handleKeydown} />

{#if open}
	<!-- Backdrop -->
	<!-- svelte-ignore a11y_click_events_have_key_events -->
	<!-- svelte-ignore a11y_no_static_element_interactions -->
	<div
		class="fixed inset-0 z-50 flex items-center justify-center bg-black/60 backdrop-blur-sm"
		onclick={handleBackdropClick}
	>
		<!-- Modal -->
		<div
			class="relative w-full max-w-lg mx-4 bg-zinc-900 border border-zinc-700 rounded-xl shadow-2xl"
			role="dialog"
			aria-modal="true"
			aria-labelledby={title ? 'modal-title' : undefined}
		>
			<!-- Header -->
			<div class="flex items-center justify-between px-6 py-4 border-b border-zinc-700">
				{#if header}
					{@render header()}
				{:else if title}
					<h2 id="modal-title" class="text-lg font-semibold text-white">{title}</h2>
				{/if}
				<button
					onclick={close}
					class="p-1 text-zinc-400 hover:text-white transition-colors rounded-lg hover:bg-zinc-800"
					aria-label="Close modal"
				>
					<svg
						class="w-5 h-5"
						fill="none"
						stroke="currentColor"
						viewBox="0 0 24 24"
						xmlns="http://www.w3.org/2000/svg"
					>
						<path
							stroke-linecap="round"
							stroke-linejoin="round"
							stroke-width="2"
							d="M6 18L18 6M6 6l12 12"
						></path>
					</svg>
				</button>
			</div>

			<!-- Body -->
			<div class="px-6 py-4">
				{@render children()}
			</div>

			<!-- Footer -->
			{#if footer}
				<div class="px-6 py-4 border-t border-zinc-700 flex justify-end gap-3">
					{@render footer()}
				</div>
			{/if}
		</div>
	</div>
{/if}
