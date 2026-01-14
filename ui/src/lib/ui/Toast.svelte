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

	interface Toast {
		id: string;
		type: 'success' | 'error' | 'warning' | 'info';
		message: string;
		duration?: number;
	}

	let toasts = $state<Toast[]>([]);

	function addToast(toast: Omit<Toast, 'id'>) {
		const id = crypto.randomUUID();
		toasts = [...toasts, { ...toast, id }];

		const duration = toast.duration ?? 5000;
		setTimeout(() => {
			removeToast(id);
		}, duration);
	}

	function removeToast(id: string) {
		toasts = toasts.filter((t) => t.id !== id);
	}

	// Expose toast functions globally
	export function toast(message: string, type: Toast['type'] = 'info', duration?: number) {
		addToast({ message, type, duration });
	}

	export const success = (message: string) => toast(message, 'success');
	export const error = (message: string) => toast(message, 'error');
	export const warning = (message: string) => toast(message, 'warning');
	export const info = (message: string) => toast(message, 'info');

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
	{#each toasts as toast (toast.id)}
		<div
			class="pointer-events-auto min-w-72 max-w-md px-4 py-3 border rounded-lg shadow-lg backdrop-blur-sm flex items-start gap-3 animate-in slide-in-from-right-5 duration-300 {typeStyles[
				toast.type
			]}"
			role="alert"
		>
			<svg
				class="w-5 h-5 flex-shrink-0 mt-0.5"
				fill="none"
				stroke="currentColor"
				viewBox="0 0 24 24"
			>
				{@html typeIcons[toast.type]}
			</svg>
			<p class="text-sm flex-1">{toast.message}</p>
			<button
				onclick={() => removeToast(toast.id)}
				class="flex-shrink-0 opacity-60 hover:opacity-100 transition-opacity"
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
