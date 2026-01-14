<script lang="ts">
	interface Props {
		title: string;
		description?: string;
		icon?: 'folder' | 'search' | 'lock' | 'info';
	}

	let { title, description, icon = 'folder' }: Props = $props();

	const icons = {
		folder: `<path stroke-linecap="round" stroke-linejoin="round" stroke-width="1.5" d="M3 7v10a2 2 0 002 2h14a2 2 0 002-2V9a2 2 0 00-2-2h-6l-2-2H5a2 2 0 00-2 2z" />`,
		search: `<path stroke-linecap="round" stroke-linejoin="round" stroke-width="1.5" d="M21 21l-6-6m2-5a7 7 0 11-14 0 7 7 0 0114 0z" />`,
		lock: `<path stroke-linecap="round" stroke-linejoin="round" stroke-width="1.5" d="M12 15v2m-6 4h12a2 2 0 002-2v-6a2 2 0 00-2-2H6a2 2 0 00-2 2v6a2 2 0 002 2zm10-10V7a4 4 0 00-8 0v4h8z" />`,
		info: `<path stroke-linecap="round" stroke-linejoin="round" stroke-width="1.5" d="M13 16h-1v-4h-1m1-4h.01M21 12a9 9 0 11-18 0 9 9 0 0118 0z" />`
	};
</script>

<div class="flex flex-col items-center justify-center py-12 px-6 text-center">
	<div class="w-16 h-16 mb-4 rounded-full bg-zinc-800 flex items-center justify-center">
		<svg
			class="w-8 h-8 text-zinc-500"
			fill="none"
			stroke="currentColor"
			viewBox="0 0 24 24"
			xmlns="http://www.w3.org/2000/svg"
		>
			{@html icons[icon]}
		</svg>
	</div>
	<h3 class="text-lg font-medium text-zinc-300">{title}</h3>
	{#if description}
		<p class="mt-2 text-sm text-zinc-500 max-w-sm">{description}</p>
	{/if}
</div>
