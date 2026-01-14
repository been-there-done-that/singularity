<script lang="ts">
	import './layout.css';
	import { Sidebar, Toast } from '$lib/ui';
	import { setAuth } from '$lib/state/auth.svelte';
	import { onMount } from 'svelte';

	let { children } = $props();

	onMount(() => {
		// For development: auto-login with a dev JWT
		// In production, this would come from your auth flow
		if (import.meta.env.DEV) {
			// Create a simple dev JWT (not for production!)
			const header = btoa(JSON.stringify({ alg: 'HS256', typ: 'JWT' }));
			const payload = btoa(
				JSON.stringify({
					sub: 'dev-user',
					roles: ['admin'],
					iss: 'https://singularity.local',
					aud: 'singularity',
					exp: Math.floor(Date.now() / 1000) + 3600 // 1 hour
				})
			);
			const signature = btoa('dev-signature');
			const devJwt = `${header}.${payload}.${signature}`;
			setAuth(devJwt);
		}
	});
</script>

<svelte:head>
	<title>Singularity Admin</title>
	<meta name="description" content="Singularity Kernel Administration Console" />
</svelte:head>

<!-- App Shell -->
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

<!-- Global Toast Notifications -->
<Toast />
