<script lang="ts">
	/**
	 * Dashboard - Landing Page
	 * 
	 * Purpose:
	 * - Situational awareness
	 * - Safe entry point (no write actions)
	 * - Overview of owned resources
	 */

	import { auth } from '$lib/state/auth.svelte';

	// Mock stats for now - these will come from the kernel
	const stats = $state({
		models: 3,
		objects: 127,
		ownedResources: 45,
		recentActivity: [
			{ action: 'Created model', resource: 'todos', time: '2 min ago' },
			{ action: 'Uploaded file', resource: 'invoices/jan.pdf', time: '15 min ago' },
			{ action: 'Updated record', resource: 'users/alice', time: '1 hour ago' }
		]
	});
</script>

<div class="max-w-6xl mx-auto">
	<!-- Header -->
	<div class="mb-8">
		<h1 class="text-3xl font-bold text-white">Dashboard</h1>
		<p class="mt-2 text-zinc-400">
			Welcome back{auth.subject ? `, ${auth.subject.id}` : ''}
		</p>
	</div>

	<!-- Stats Grid -->
	<div class="grid grid-cols-1 md:grid-cols-3 gap-6 mb-8">
		<!-- Models -->
		{#if auth.isAdmin}
			<div
				class="bg-zinc-900 border border-zinc-800 rounded-xl p-6 hover:border-zinc-700 transition-colors"
			>
				<div class="flex items-center gap-4">
					<div class="p-3 bg-indigo-500/10 rounded-lg">
						<svg
							class="w-6 h-6 text-indigo-400"
							fill="none"
							stroke="currentColor"
							viewBox="0 0 24 24"
						>
							<path
								stroke-linecap="round"
								stroke-linejoin="round"
								stroke-width="1.5"
								d="M4 7v10c0 2.21 3.582 4 8 4s8-1.79 8-4V7M4 7c0 2.21 3.582 4 8 4s8-1.79 8-4M4 7c0-2.21 3.582-4 8-4s8 1.79 8 4"
							/>
						</svg>
					</div>
					<div>
						<p class="text-2xl font-bold text-white">{stats.models}</p>
						<p class="text-sm text-zinc-400">Models</p>
					</div>
				</div>
			</div>
		{/if}

		<!-- Objects -->
		<div
			class="bg-zinc-900 border border-zinc-800 rounded-xl p-6 hover:border-zinc-700 transition-colors"
		>
			<div class="flex items-center gap-4">
				<div class="p-3 bg-emerald-500/10 rounded-lg">
					<svg
						class="w-6 h-6 text-emerald-400"
						fill="none"
						stroke="currentColor"
						viewBox="0 0 24 24"
					>
						<path
							stroke-linecap="round"
							stroke-linejoin="round"
							stroke-width="1.5"
							d="M5 19a2 2 0 01-2-2V7a2 2 0 012-2h4l2 2h4a2 2 0 012 2v1M5 19h14a2 2 0 002-2v-5a2 2 0 00-2-2H9a2 2 0 00-2 2v5a2 2 0 01-2 2z"
						/>
					</svg>
				</div>
				<div>
					<p class="text-2xl font-bold text-white">{stats.objects}</p>
					<p class="text-sm text-zinc-400">Objects</p>
				</div>
			</div>
		</div>

		<!-- Owned Resources -->
		<div
			class="bg-zinc-900 border border-zinc-800 rounded-xl p-6 hover:border-zinc-700 transition-colors"
		>
			<div class="flex items-center gap-4">
				<div class="p-3 bg-purple-500/10 rounded-lg">
					<svg
						class="w-6 h-6 text-purple-400"
						fill="none"
						stroke="currentColor"
						viewBox="0 0 24 24"
					>
						<path
							stroke-linecap="round"
							stroke-linejoin="round"
							stroke-width="1.5"
							d="M9 12l2 2 4-4m5.618-4.016A11.955 11.955 0 0112 2.944a11.955 11.955 0 01-8.618 3.04A12.02 12.02 0 003 9c0 5.591 3.824 10.29 9 11.622 5.176-1.332 9-6.03 9-11.622 0-1.042-.133-2.052-.382-3.016z"
						/>
					</svg>
				</div>
				<div>
					<p class="text-2xl font-bold text-white">{stats.ownedResources}</p>
					<p class="text-sm text-zinc-400">Your Resources</p>
				</div>
			</div>
		</div>
	</div>

	<!-- Recent Activity -->
	<div class="bg-zinc-900 border border-zinc-800 rounded-xl">
		<div class="px-6 py-4 border-b border-zinc-800">
			<h2 class="text-lg font-semibold text-white">Recent Activity</h2>
		</div>
		<div class="divide-y divide-zinc-800">
			{#each stats.recentActivity as activity}
				<div class="px-6 py-4 flex items-center justify-between">
					<div class="flex items-center gap-3">
						<div class="w-2 h-2 bg-emerald-400 rounded-full"></div>
						<div>
							<p class="text-sm text-white">{activity.action}</p>
							<p class="text-xs text-zinc-500">{activity.resource}</p>
						</div>
					</div>
					<span class="text-xs text-zinc-500">{activity.time}</span>
				</div>
			{/each}
		</div>
	</div>
</div>
