<script lang="ts">
	import './layout.css';
	import { Sidebar, Toast } from '$lib/ui';
	import { auth } from '$lib/state/auth.svelte';
	import { system } from '$lib/state/system.svelte';
	import { page } from '$app/stores';
	import { goto } from '$app/navigation';
	import { onMount } from 'svelte';

	let { children } = $props();
	let mounted = $state(false);

	onMount(async () => {
		await system.refreshStatus();
		mounted = true;
	});

	// Guard logic
	$effect(() => {
		if (!mounted) return;
		
		const path = ($page.url.pathname as string);
		
		// 1. System guard (Bootstrap vs OK)
		if (system.status === 'bootstrapping') {
			if (path !== '/setup') {
				goto('/setup');
			}
			return;
		}

		// 2. Auth guard (Logged in vs Not)
		if (system.status === 'ok') {
			// Skip for login/setup pages
			if (path === '/login' || path === '/setup') return;
			
			// If not authenticated, redirect to login
			if (!auth.isAuthenticated) {
				goto('/login');
			}
		}
	});

	// View derivers
	const isSetupPage = $derived(($page.url.pathname as string) === '/setup');
	const isLoginPage = $derived(($page.url.pathname as string) === '/login');
	const showShell = $derived(system.status === 'ok' && auth.isAuthenticated && !isLoginPage && !isSetupPage);
</script>

<svelte:head>
	<title>Singularity Admin</title>
	<meta name="description" content="Singularity Kernel Administration Console" />
</svelte:head>

{#if !mounted || system.isChecking}
	<!-- Initialization state -->
	<div class="min-h-screen flex items-center justify-center bg-zinc-950">
		<div class="flex flex-col items-center gap-4">
			<div class="w-8 h-8 border-2 border-zinc-700 border-t-indigo-500 rounded-full animate-spin"></div>
			<div class="text-zinc-500 text-sm font-medium tracking-tight">Initializing system...</div>
		</div>
	</div>
{:else if system.status === 'bootstrapping'}
	{#if isSetupPage}
		{@render children()}
	{:else}
		<div class="min-h-screen flex items-center justify-center bg-zinc-950">
			<div class="text-zinc-500 text-sm">Redirecting to setup...</div>
		</div>
	{/if}
{:else if isLoginPage}
	<!-- Login page -->
	{@render children()}
{:else if showShell}
	<!-- App Shell with sidebar -->
	<div class="flex min-h-screen">
		<Sidebar />
		<main class="flex-1 ml-64">
			<div class="p-6">
				{@render children()}
			</div>
		</main>
	</div>
{:else if system.status === 'degraded'}
	<!-- System Error state -->
	<div class="min-h-screen flex items-center justify-center bg-zinc-950">
		<div class="max-w-md w-full p-8 border border-red-900/30 bg-red-950/5 rounded-2xl text-center">
			<div class="text-red-500 mb-2 font-semibold">System degraded</div>
			<div class="text-zinc-400 text-sm mb-6">{system.error || 'Failed to connect to kernel.'}</div>
			<button 
				onclick={() => system.refreshStatus()}
				class="px-4 py-2 bg-zinc-900 hover:bg-zinc-800 text-zinc-300 rounded-lg text-sm transition-colors border border-zinc-800"
			>
				Retry connection
			</button>
		</div>
	</div>
{:else}
	<!-- Default fallback / Auth redirecting -->
	<div class="min-h-screen flex items-center justify-center bg-zinc-950">
		<div class="text-zinc-400 text-sm">Redirecting...</div>
	</div>
{/if}

<!-- Global Toast Notifications -->
<Toast />
