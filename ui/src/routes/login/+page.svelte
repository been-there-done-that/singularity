<script lang="ts">
	/**
	 * Login Page
	 * 
	 * Authenticates users via the kernel's /auth/login endpoint.
	 * On success, stores JWT and redirects to dashboard.
	 */
	import { Button } from '$lib/ui';
	import { setAuth } from '$lib/state/auth.svelte';
	import { goto } from '$app/navigation';

	let username = $state('admin');
	let password = $state('');
	let loading = $state(false);
	let error = $state<string | null>(null);
	let isRegister = $state(false);

	async function handleSubmit() {
		loading = true;
		error = null;

		try {
			const endpoint = isRegister ? '/auth/register' : '/auth/login';
		// Note: Register only sends username/password - roles always default to ["user"]
		// Admin promotion is a separate privileged operation
		const body = { username, password };

			const response = await fetch(`http://localhost:3000${endpoint}`, {
				method: 'POST',
				headers: { 'Content-Type': 'application/json' },
				body: JSON.stringify(body)
			});

			if (!response.ok) {
				const data = await response.json();
				throw new Error(data.error || `Authentication failed: ${response.status}`);
			}

			const data = await response.json();
			setAuth(data.token);
			goto('/');
		} catch (e) {
			error = e instanceof Error ? e.message : 'Authentication failed';
		} finally {
			loading = false;
		}
	}
</script>

<div class="min-h-screen flex items-center justify-center bg-zinc-950">
	<div class="w-full max-w-md p-8">
		<div class="text-center mb-8">
			<!-- Logo -->
			<div class="w-12 h-12 mx-auto mb-4 rounded-xl bg-linear-to-br from-indigo-500 to-purple-600 flex items-center justify-center">
				<svg class="w-7 h-7 text-white" viewBox="0 0 24 24" fill="currentColor">
					<path d="M12 2L2 7l10 5 10-5-10-5zM2 17l10 5 10-5M2 12l10 5 10-5"/>
				</svg>
			</div>
			<h1 class="text-2xl font-bold text-white">Singularity</h1>
			<p class="mt-2 text-zinc-400">{isRegister ? 'Create an account' : 'Sign in to continue'}</p>
		</div>

		<div class="bg-zinc-900 border border-zinc-800 rounded-xl p-6">
			{#if error}
				<div class="mb-4 p-3 bg-red-950/50 border border-red-500/30 rounded-lg">
					<p class="text-sm text-red-400">{error}</p>
				</div>
			{/if}

			<form onsubmit={(e) => { e.preventDefault(); handleSubmit(); }}>
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

					<div>
						<label for="password" class="block text-sm font-medium text-zinc-300 mb-2">
							Password
						</label>
						<input
							id="password"
							type="password"
							bind:value={password}
							class="w-full px-4 py-3 bg-zinc-800 border border-zinc-700 rounded-lg text-white
							       placeholder:text-zinc-500 focus:outline-none focus:ring-2 focus:ring-indigo-500"
							placeholder="Enter password"
						/>
					</div>

					<div class="pt-2">
						<Button type="submit" variant="primary" disabled={loading || !username || !password}>
							{#if loading}
								<svg class="w-5 h-5 animate-spin" fill="none" viewBox="0 0 24 24">
									<circle class="opacity-25" cx="12" cy="12" r="10" stroke="currentColor" stroke-width="4"/>
									<path class="opacity-75" fill="currentColor" d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4z"/>
								</svg>
								{isRegister ? 'Creating account...' : 'Signing in...'}
							{:else}
								{isRegister ? 'Create Account' : 'Sign In'}
							{/if}
						</Button>
					</div>
				</div>
			</form>

			<div class="mt-6 pt-4 border-t border-zinc-800">
				<button 
					type="button"
					onclick={() => { isRegister = !isRegister; error = null; }}
					class="w-full text-sm text-zinc-400 hover:text-white transition-colors"
				>
					{isRegister ? 'Already have an account? Sign in' : "Don't have an account? Create one"}
				</button>
			</div>
		</div>
	</div>
</div>

<style>
	:global(body) {
		background-color: #09090b;
	}
</style>
