<script lang="ts">
	/**
	 * Dashboard - Landing Page
	 * 
	 * Purpose:
	 * - Situational awareness
	 * - Safe entry point (no write actions)
	 * - Overview of user session
	 */

	import { auth } from '$lib/state/auth.svelte';
	import { executeWithCapability, KernelRequestError } from '$lib/kernel';

	// Real stats from kernel
	let loading = $state(true);
	let stats = $state({
		models: 0,
		objects: 0
	});

	// Load stats on mount
	$effect(() => {
		loadStats();
	});

	async function loadStats() {
		loading = true;
		try {
			// Get model count from schema.list_models
			const models = await executeWithCapability<Record<string, unknown>[]>(
				{
					op: 'schema.list_models',
					resource: { resource_type: '__models', resource_id: null },
					input: {}
				}
			);
			stats.models = Array.isArray(models) ? models.length : 0;
		} catch (e) {
			// Silently handle - dashboard is read-only overview
			console.error('Failed to load stats:', e);
		} finally {
			loading = false;
		}
	}
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
	<div class="grid grid-cols-1 md:grid-cols-2 gap-6 mb-8">
		<!-- Models -->
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
					{#if loading}
						<div class="h-7 w-8 bg-zinc-800 rounded animate-pulse"></div>
					{:else}
						<p class="text-2xl font-bold text-white">{stats.models}</p>
					{/if}
					<p class="text-sm text-zinc-400">Models</p>
				</div>
			</div>
		</div>

		<!-- Session Info -->
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
							d="M9 12l2 2 4-4m5.618-4.016A11.955 11.955 0 0112 2.944a11.955 11.955 0 01-8.618 3.04A12.02 12.02 0 003 9c0 5.591 3.824 10.29 9 11.622 5.176-1.332 9-6.03 9-11.622 0-1.042-.133-2.052-.382-3.016z"
						/>
					</svg>
				</div>
				<div>
					<p class="text-2xl font-bold text-white">Active</p>
					<p class="text-sm text-zinc-400">Session</p>
				</div>
			</div>
		</div>
	</div>

	<!-- Quick Links -->
	<div class="bg-zinc-900 border border-zinc-800 rounded-xl">
		<div class="px-6 py-4 border-b border-zinc-800">
			<h2 class="text-lg font-semibold text-white">Quick Actions</h2>
		</div>
		<div class="p-6 grid grid-cols-1 md:grid-cols-3 gap-4">
			<a 
				href="/schema" 
				class="flex items-center gap-3 p-4 bg-zinc-800/50 rounded-lg hover:bg-zinc-800 transition-colors"
			>
				<svg class="w-5 h-5 text-indigo-400" fill="none" stroke="currentColor" viewBox="0 0 24 24">
					<path stroke-linecap="round" stroke-linejoin="round" stroke-width="1.5" 
						d="M4 7v10c0 2.21 3.582 4 8 4s8-1.79 8-4V7M4 7c0 2.21 3.582 4 8 4s8-1.79 8-4M4 7c0-2.21 3.582-4 8-4s8 1.79 8 4" />
				</svg>
				<span class="text-sm text-white">View Schema</span>
			</a>
			<a 
				href="/data" 
				class="flex items-center gap-3 p-4 bg-zinc-800/50 rounded-lg hover:bg-zinc-800 transition-colors"
			>
				<svg class="w-5 h-5 text-emerald-400" fill="none" stroke="currentColor" viewBox="0 0 24 24">
					<path stroke-linecap="round" stroke-linejoin="round" stroke-width="1.5" 
						d="M4 6h16M4 10h16M4 14h16M4 18h16" />
				</svg>
				<span class="text-sm text-white">Browse Data</span>
			</a>
			<a 
				href="/objects" 
				class="flex items-center gap-3 p-4 bg-zinc-800/50 rounded-lg hover:bg-zinc-800 transition-colors"
			>
				<svg class="w-5 h-5 text-purple-400" fill="none" stroke="currentColor" viewBox="0 0 24 24">
					<path stroke-linecap="round" stroke-linejoin="round" stroke-width="1.5" 
						d="M5 19a2 2 0 01-2-2V7a2 2 0 012-2h4l2 2h4a2 2 0 012 2v1M5 19h14a2 2 0 002-2v-5a2 2 0 00-2-2H9a2 2 0 00-2 2v5a2 2 0 01-2 2z" />
				</svg>
				<span class="text-sm text-white">Browse Objects</span>
			</a>
		</div>
	</div>
</div>
