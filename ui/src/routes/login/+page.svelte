<script lang="ts">
	/**
	 * Login Page
	 * 
	 * For development: generates a valid JWT client-side using the dev secret.
	 * In production: would redirect to an external identity provider.
	 */
	import { Button } from '$lib/ui';
	import { auth } from '$lib/state/auth.svelte';
	import { goto } from '$app/navigation';

	let username = $state('admin');
	let loading = $state(false);
	let error = $state<string | null>(null);

	async function handleLogin() {
		loading = true;
		error = null;

		try {
			// For development: generate a JWT client-side
			// In production this would hit your identity provider
			const jwt = await generateDevToken(username);
			auth.setJwt(jwt);
			goto('/');
		} catch (e) {
			error = e instanceof Error ? e.message : 'Login failed';
		} finally {
			loading = false;
		}
	}

	/**
	 * DEV ONLY: Generate a JWT using the development secret.
	 * This matches the kernel's expected format.
	 * 
	 * WARNING: Never do this in production!
	 */
	async function generateDevToken(sub: string): Promise<string> {
		// We'll use the jose library for JWT generation
		// For now, use a pre-generated dev token approach via kernel endpoint
		
		// Call the kernel's dev token endpoint
		const response = await fetch('http://localhost:3000/v1/dev/token', {
			method: 'POST',
			headers: { 'Content-Type': 'application/json' },
			body: JSON.stringify({
				sub,
				roles: ['admin']
			})
		});

		if (!response.ok) {
			throw new Error(`Token generation failed: ${response.status}`);
		}

		const data = await response.json();
		return data.token;
	}
</script>

<div class="min-h-screen flex items-center justify-center bg-zinc-950">
	<div class="w-full max-w-md p-8">
		<div class="text-center mb-8">
			<!-- Logo -->
			<div class="w-12 h-12 mx-auto mb-4 rounded-xl bg-gradient-to-br from-indigo-500 to-purple-600 flex items-center justify-center">
				<svg class="w-7 h-7 text-white" viewBox="0 0 24 24" fill="currentColor">
					<path d="M12 2L2 7l10 5 10-5-10-5zM2 17l10 5 10-5M2 12l10 5 10-5"/>
				</svg>
			</div>
			<h1 class="text-2xl font-bold text-white">Singularity</h1>
			<p class="mt-2 text-zinc-400">Sign in to continue</p>
		</div>

		<div class="bg-zinc-900 border border-zinc-800 rounded-xl p-6">
			{#if error}
				<div class="mb-4 p-3 bg-red-950/50 border border-red-500/30 rounded-lg">
					<p class="text-sm text-red-400">{error}</p>
				</div>
			{/if}

			<form onsubmit={(e) => { e.preventDefault(); handleLogin(); }}>
				<div class="space-y-4">
					<div>
						<label for="username" class="block text-sm font-medium text-zinc-300 mb-2">
							Username
						</label>
						<input
							id="username"
							type="text"
							bind:value={username}
							class="w-full px-4 py-3 bg-zinc-800 border border-zinc-700 rounded-lg text-white
							       placeholder:text-zinc-500 focus:outline-none focus:ring-2 focus:ring-indigo-500"
							placeholder="Enter username"
						/>
					</div>

					<div class="pt-2">
						<Button type="submit" variant="primary" disabled={loading} class="w-full justify-center">
							{#if loading}
								<svg class="w-5 h-5 animate-spin" fill="none" viewBox="0 0 24 24">
									<circle class="opacity-25" cx="12" cy="12" r="10" stroke="currentColor" stroke-width="4"/>
									<path class="opacity-75" fill="currentColor" d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4z"/>
								</svg>
								Signing in...
							{:else}
								Sign In
							{/if}
						</Button>
					</div>
				</div>
			</form>

			<div class="mt-6 pt-4 border-t border-zinc-800">
				<p class="text-xs text-zinc-500 text-center">
					Development mode: tokens are generated locally
				</p>
			</div>
		</div>
	</div>
</div>

<style>
	:global(body) {
		background-color: #09090b;
	}
</style>
