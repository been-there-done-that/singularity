<script lang="ts">
    import { system } from '$lib/state/system.svelte';
    import { auth, setAuth } from '$lib/state/auth.svelte';
    import { kernelFetch } from '$lib/kernel';
    import { goto } from '$app/navigation';
    import { toast } from '$lib/ui';

    let username = $state('');
    let password = $state('');
    let bootstrapCode = $state('');
    let isLoading = $state(false);
    let error = $state<string | null>(null);

    // Guard: Only allow if bootstrapping
    $effect(() => {
        if (system.status !== 'bootstrapping') {
            goto('/');
        }
    });

    async function handleSetup(e: SubmitEvent) {
        e.preventDefault();
        if (isLoading) return;

        isLoading = true;
        error = null;

        try {
            const response = await kernelFetch<{ jwt: string }>('/auth/register', {
                method: 'POST',
                body: {
                    username,
                    password,
                    bootstrap_code: bootstrapCode
                }
            });

            // 1. Set auth state
            setAuth(response.jwt);
            
            // 2. Clear bootstrap state and refresh system
            await system.refreshStatus();
            
            toast.success('Admin account created successfully!');
            goto('/');
        } catch (err: any) {
            console.error('Setup failed:', err);
            error = 'Admin setup failed. Check bootstrap code and try again.';
            toast.error(error);
        } finally {
            isLoading = false;
        }
    }
</script>

<svelte:head>
    <title>Admin Setup — Singularity</title>
</svelte:head>

<div class="min-h-screen bg-zinc-950 flex flex-col items-center justify-center p-6">
    <div class="w-full max-w-[400px]">
        <!-- Header -->
        <div class="mb-10 text-center">
            <div class="inline-flex items-center justify-center w-12 h-12 rounded-xl bg-indigo-500/10 border border-indigo-500/20 mb-4">
                <svg xmlns="http://www.w3.org/2000/svg" width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" class="text-indigo-500"><path d="M12 22s8-4 8-10V5l-8-3-8 3v7c0 6 8 10 8 10"/><path d="m9 12 2 2 4-4"/></svg>
            </div>
            <h1 class="text-2xl font-semibold text-zinc-100 tracking-tight">System Initialization</h1>
            <p class="mt-2 text-zinc-500 text-sm leading-relaxed">
                A new Singularity instance has been detected. 
                Configure the primary administrator account to continue.
            </p>
        </div>

        <!-- Setup Form -->
        <form onsubmit={handleSetup} class="space-y-5">
            {#if error}
                <div class="p-3 rounded-lg bg-red-500/10 border border-red-500/20 text-red-500 text-xs font-medium">
                    {error}
                </div>
            {/if}

            <div class="space-y-4">
                <div class="space-y-1.5">
                    <label for="username" class="text-xs font-medium text-zinc-400 ml-1">Admin Username</label>
                    <input
                        id="username"
                        type="text"
                        bind:value={username}
                        placeholder="e.g. admin"
                        required
                        class="w-full bg-zinc-900 border border-zinc-800 rounded-lg px-4 py-2.5 text-sm text-zinc-200 placeholder:text-zinc-600 focus:outline-none focus:ring-2 focus:ring-indigo-500/20 focus:border-indigo-500/40 transition-all"
                    />
                </div>

                <div class="space-y-1.5">
                    <label for="password" class="text-xs font-medium text-zinc-400 ml-1">Password</label>
                    <input
                        id="password"
                        type="password"
                        bind:value={password}
                        placeholder="••••••••"
                        required
                        class="w-full bg-zinc-900 border border-zinc-800 rounded-lg px-4 py-2.5 text-sm text-zinc-200 placeholder:text-zinc-600 focus:outline-none focus:ring-2 focus:ring-indigo-500/20 focus:border-indigo-500/40 transition-all"
                    />
                </div>

                <div class="space-y-1.5 pt-2">
                    <div class="flex items-center justify-between ml-1">
                        <label for="code" class="text-xs font-medium text-zinc-400">Bootstrap Code</label>
                        <span class="text-[10px] text-zinc-500 italic">Check kernel boot logs</span>
                    </div>
                    <input
                        id="code"
                        type="text"
                        bind:value={bootstrapCode}
                        placeholder="XXXX-XXXX"
                        required
                        class="w-full bg-zinc-900 border border-zinc-800 rounded-lg px-4 py-2.5 text-sm text-zinc-200 placeholder:text-zinc-600 font-mono focus:outline-none focus:ring-2 focus:ring-indigo-500/20 focus:border-indigo-500/40 transition-all"
                    />
                </div>
            </div>

            <button
                type="submit"
                disabled={isLoading}
                class="w-full bg-indigo-600 hover:bg-indigo-500 disabled:opacity-50 disabled:cursor-not-allowed text-white font-medium py-3 rounded-lg text-sm transition-all shadow-lg shadow-indigo-600/10 active:scale-[0.98]"
            >
                {#if isLoading}
                    <div class="flex items-center justify-center gap-2">
                        <div class="w-4 h-4 border-2 border-white/30 border-t-white rounded-full animate-spin"></div>
                        <span>Initializing...</span>
                    </div>
                {:else}
                    Complete Setup
                {/if}
            </button>
        </form>

        <div class="mt-8 pt-8 border-t border-zinc-900">
            <p class="text-[11px] text-zinc-600 text-center leading-relaxed">
                This process is only required once per deployment. 
                The bootstrap code is generated in memory and never persisted.
            </p>
        </div>
    </div>
</div>
