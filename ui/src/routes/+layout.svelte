<script lang="ts">
	import './layout.css';
	import { Sidebar, Toast } from '$lib/ui';
	import { auth } from '$lib/state/auth.svelte';
	import { page } from '$app/stores';
	import { goto } from '$app/navigation';
	import { onMount } from 'svelte';

	let { children } = $props();
	let mounted = $state(false);

	onMount(() => {
		mounted = true;
	});

	// Auth guard - redirect to login if not authenticated
	// Only runs after mount to avoid SSR issues and give state time to initialize
	$effect(() => {
		if (!mounted) return;
		
		// Skip for login page
		if ($page.url.pathname === '/login') return;
		
		// If not authenticated, redirect to login
		if (!auth.isAuthenticated) {
			goto('/login');
		}
	});

	// Check if on login page (don't show sidebar)
	const isLoginPage = $derived($page.url.pathname === '/login');
</script>

<svelte:head>
	<title>Singularity Admin</title>
	<meta name="description" content="Singularity Kernel Administration Console" />
</svelte:head>

{#if isLoginPage}
	<!-- Login page without sidebar -->
	{@render children()}
{:else if auth.isAuthenticated}
	<!-- App Shell with sidebar (only if authenticated) -->
	<div class="flex min-h-screen">
		<!-- Sidebar Navigation -->
		<Sidebar />

		<!-- Main Content Area -->
		<main class="flex-1 ml-64">
			<div class="p-6">
				{@render children()}
			</div>
		</main>
	</div>
{:else}
	<!-- Loading state while checking auth -->
	<div class="min-h-screen flex items-center justify-center bg-zinc-950">
		<div class="text-zinc-400">Checking authentication...</div>
	</div>
{/if}

<!-- Global Toast Notifications -->
<Toast />
