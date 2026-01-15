<script lang="ts">
	import { page } from '$app/state';
	import { auth, clearAuth } from '$lib/state/auth.svelte';
	import { goto } from '$app/navigation';

	interface NavItem {
		label: string;
		href: string;
		icon: string;
		adminOnly?: boolean;
	}

	const navItems: NavItem[] = [
		{
			label: 'Dashboard',
			href: '/',
			icon: `<path stroke-linecap="round" stroke-linejoin="round" stroke-width="1.5" d="M3 12l2-2m0 0l7-7 7 7M5 10v10a1 1 0 001 1h3m10-11l2 2m-2-2v10a1 1 0 01-1 1h-3m-6 0a1 1 0 001-1v-4a1 1 0 011-1h2a1 1 0 011 1v4a1 1 0 001 1m-6 0h6" />`
		},
		{
			label: 'Schema',
			href: '/schema',
			icon: `<path stroke-linecap="round" stroke-linejoin="round" stroke-width="1.5" d="M4 7v10c0 2.21 3.582 4 8 4s8-1.79 8-4V7M4 7c0 2.21 3.582 4 8 4s8-1.79 8-4M4 7c0-2.21 3.582-4 8-4s8 1.79 8 4m0 5c0 2.21-3.582 4-8 4s-8-1.79-8-4" />`
			// Schema visible to all authenticated users
		},
		{
			label: 'Data',
			href: '/data',
			icon: `<path stroke-linecap="round" stroke-linejoin="round" stroke-width="1.5" d="M4 6h16M4 10h16M4 14h16M4 18h16" />`
		},
		{
			label: 'Objects',
			href: '/objects',
			icon: `<path stroke-linecap="round" stroke-linejoin="round" stroke-width="1.5" d="M5 19a2 2 0 01-2-2V7a2 2 0 012-2h4l2 2h4a2 2 0 012 2v1M5 19h14a2 2 0 002-2v-5a2 2 0 00-2-2H9a2 2 0 00-2 2v5a2 2 0 01-2 2z" />`
		},
		{
			label: 'Users',
			href: '/users',
			icon: `<path stroke-linecap="round" stroke-linejoin="round" stroke-width="1.5" d="M12 4.354a4 4 0 110 5.292M15 21H3v-1a6 6 0 0112 0v1zm0 0h6v-1a6 6 0 00-9-5.197M13 7a4 4 0 11-8 0 4 4 0 018 0z" />`,
			adminOnly: true
		},
		{
			label: 'Policies',
			href: '/policies',
			icon: `<path stroke-linecap="round" stroke-linejoin="round" stroke-width="1.5" d="M9 12l2 2 4-4m5.618-4.016A11.955 11.955 0 0112 2.944a11.955 11.955 0 01-8.618 3.04A12.02 12.02 0 003 9c0 5.591 3.824 10.29 9 11.622 5.176-1.332 9-6.03 9-11.622 0-1.042-.133-2.052-.382-3.016z" />`,
			adminOnly: true
		},
		{
			label: 'System',
			href: '/system',
			icon: `<path stroke-linecap="round" stroke-linejoin="round" stroke-width="1.5" d="M10.325 4.317c.426-1.756 2.924-1.756 3.35 0a1.724 1.724 0 002.573 1.066c1.543-.94 3.31.826 2.37 2.37a1.724 1.724 0 001.065 2.572c1.756.426 1.756 2.924 0 3.35a1.724 1.724 0 00-1.066 2.573c.94 1.543-.826 3.31-2.37 2.37a1.724 1.724 0 00-2.572 1.065c-.426 1.756-2.924 1.756-3.35 0a1.724 1.724 0 00-2.573-1.066c-1.543.94-3.31-.826-2.37-2.37a1.724 1.724 0 00-1.065-2.572c-1.756-.426-1.756-2.924 0-3.35a1.724 1.724 0 001.066-2.573c-.94-1.543.826-3.31 2.37-2.37.996.608 2.296.07 2.572-1.065z" /><path stroke-linecap="round" stroke-linejoin="round" stroke-width="1.5" d="M15 12a3 3 0 11-6 0 3 3 0 016 0z" />`,
			adminOnly: true
		}
	];

	function isActive(href: string): boolean {
		if (href === '/') {
			return page.url.pathname === '/';
		}
		return page.url.pathname.startsWith(href);
	}

	function handleLogout() {
		clearAuth();
		goto('/login');
	}
</script>

<aside
	class="fixed left-0 top-0 h-screen w-64 bg-zinc-950 border-r border-zinc-800 flex flex-col z-40"
>
	<!-- Logo -->
	<div class="px-6 py-5 border-b border-zinc-800">
		<div class="flex items-center gap-3">
			<div
				class="w-8 h-8 rounded-lg bg-gradient-to-br from-indigo-500 to-purple-600 flex items-center justify-center"
			>
				<svg class="w-5 h-5 text-white" fill="currentColor" viewBox="0 0 24 24">
					<path
						d="M12 2C6.48 2 2 6.48 2 12s4.48 10 10 10 10-4.48 10-10S17.52 2 12 2zm-2 15l-5-5 1.41-1.41L10 14.17l7.59-7.59L19 8l-9 9z"
					/>
				</svg>
			</div>
			<span class="font-bold text-lg text-white">Singularity</span>
		</div>
	</div>

	<!-- Navigation -->
	<nav class="flex-1 px-3 py-4 overflow-y-auto">
		<ul class="space-y-1">
			{#each navItems as item}
			{#if !item.adminOnly || auth.isAdmin}
					<li>
						<a
							href={item.href}
							class="flex items-center gap-3 px-3 py-2 rounded-lg text-sm font-medium transition-colors
								{isActive(item.href)
								? 'bg-indigo-500/10 text-indigo-400'
								: 'text-zinc-400 hover:text-white hover:bg-zinc-800'}"
						>
							<svg class="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
								{@html item.icon}
							</svg>
							{item.label}
						</a>
					</li>
				{/if}
			{/each}
		</ul>
	</nav>

	<!-- User Menu (bottom) -->
	{#if auth.isAuthenticated}
		<div class="px-3 py-4 border-t border-zinc-800">
			<div class="flex items-center gap-3 px-3 py-2">
				<div class="w-8 h-8 rounded-full bg-zinc-700 flex items-center justify-center">
					<svg class="w-4 h-4 text-zinc-400" fill="currentColor" viewBox="0 0 24 24">
						<path
							d="M12 12c2.21 0 4-1.79 4-4s-1.79-4-4-4-4 1.79-4 4 1.79 4 4 4zm0 2c-2.67 0-8 1.34-8 4v2h16v-2c0-2.66-5.33-4-8-4z"
						/>
					</svg>
				</div>
				<div class="flex-1 min-w-0">
					<p class="text-sm font-medium text-white truncate">
						{auth.isAdmin ? 'Admin' : 'User'}
					</p>
					<p class="text-xs text-zinc-500 truncate">{auth.subject?.id?.slice(0, 20)}...</p>
				</div>
			</div>
			<!-- Logout Button -->
			<button
				onclick={handleLogout}
				class="mt-2 w-full flex items-center gap-3 px-3 py-2 rounded-lg text-sm font-medium
					   text-zinc-400 hover:text-red-400 hover:bg-red-500/10 transition-colors"
			>
				<svg class="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
					<path stroke-linecap="round" stroke-linejoin="round" stroke-width="1.5" 
						d="M17 16l4-4m0 0l-4-4m4 4H7m6 4v1a3 3 0 01-3 3H6a3 3 0 01-3-3V7a3 3 0 013-3h4a3 3 0 013 3v1" />
				</svg>
				Logout
			</button>
		</div>
	{/if}
</aside>
