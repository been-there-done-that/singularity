<script lang="ts">
	import type { Snippet } from 'svelte';

	interface Props {
		variant?: 'primary' | 'secondary' | 'danger' | 'ghost';
		size?: 'sm' | 'md' | 'lg';
		loading?: boolean;
		disabled?: boolean;
		type?: 'button' | 'submit' | 'reset';
		onclick?: (e: MouseEvent) => void;
		children: Snippet;
	}

	let {
		variant = 'primary',
		size = 'md',
		loading = false,
		disabled = false,
		type = 'button',
		onclick,
		children
	}: Props = $props();

	const baseClasses =
		'inline-flex items-center justify-center font-medium rounded-lg transition-all duration-200 focus:outline-none focus:ring-2 focus:ring-offset-2 focus:ring-offset-zinc-900 disabled:opacity-50 disabled:cursor-not-allowed';

	const variantClasses = {
		primary:
			'bg-indigo-600 text-white hover:bg-indigo-500 focus:ring-indigo-500 shadow-lg shadow-indigo-500/25',
		secondary:
			'bg-zinc-700 text-zinc-100 hover:bg-zinc-600 focus:ring-zinc-500 border border-zinc-600',
		danger:
			'bg-red-600 text-white hover:bg-red-500 focus:ring-red-500 shadow-lg shadow-red-500/25',
		ghost: 'bg-transparent text-zinc-300 hover:bg-zinc-800 hover:text-white focus:ring-zinc-500'
	};

	const sizeClasses = {
		sm: 'px-3 py-1.5 text-sm gap-1.5',
		md: 'px-4 py-2 text-sm gap-2',
		lg: 'px-6 py-3 text-base gap-2'
	};
</script>

<button
	{type}
	class="{baseClasses} {variantClasses[variant]} {sizeClasses[size]}"
	disabled={disabled || loading}
	{onclick}
>
	{#if loading}
		<svg
			class="animate-spin -ml-1 h-4 w-4"
			xmlns="http://www.w3.org/2000/svg"
			fill="none"
			viewBox="0 0 24 24"
		>
			<circle class="opacity-25" cx="12" cy="12" r="10" stroke="currentColor" stroke-width="4"
			></circle>
			<path
				class="opacity-75"
				fill="currentColor"
				d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4zm2 5.291A7.962 7.962 0 014 12H0c0 3.042 1.135 5.824 3 7.938l3-2.647z"
			></path>
		</svg>
	{/if}
	{@render children()}
</button>
