<script lang="ts">
	/**
	 * Access Profiles - Admin Only
	 * 
	 * Row-Level Security (RLS) profile management.
	 * CRUD for access profiles with safe defaults.
	 */
	import { onMount } from 'svelte';
	import { EmptyState, Button } from '$lib/ui';
	import { listProfiles, createProfile, deleteProfile, type AccessProfile } from '$lib/kernel';
	import AccessProfileCard from './AccessProfileCard.svelte';
	import AccessProfileEditor from './AccessProfileEditor.svelte';

	let profiles = $state<AccessProfile[]>([]);
	let loading = $state(true);
	let error = $state<string | null>(null);
	let editingProfile = $state<AccessProfile | null>(null);
	let isCreating = $state(false);

	async function loadProfiles() {
		loading = true;
		error = null;
		try {
			const result = await listProfiles();
			profiles = result.profiles;
		} catch (e: any) {
			error = e.message ?? 'Failed to load profiles';
		} finally {
			loading = false;
		}
	}

	async function handleDelete(id: string) {
		if (!confirm('Delete this access profile? This cannot be undone.')) return;
		try {
			await deleteProfile(id);
			profiles = profiles.filter((p) => p.id !== id);
		} catch (e: any) {
			error = e.message ?? 'Failed to delete profile';
		}
	}

	function handleEdit(profile: AccessProfile) {
		editingProfile = profile;
	}

	function handleCreate() {
		isCreating = true;
	}

	function handleEditorClose() {
		editingProfile = null;
		isCreating = false;
		loadProfiles();
	}

	onMount(loadProfiles);
</script>

<div class="max-w-6xl mx-auto">
	<div class="mb-8 flex items-center justify-between">
		<div>
			<h1 class="text-3xl font-bold text-white">Access Profiles</h1>
			<p class="mt-2 text-zinc-400">Configure row-level security (RLS) for models</p>
		</div>
		<Button onclick={handleCreate} variant="primary">
			<svg class="w-4 h-4 mr-2" fill="none" stroke="currentColor" viewBox="0 0 24 24">
				<path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M12 4v16m8-8H4" />
			</svg>
			New Profile
		</Button>
	</div>

	{#if error}
		<div class="mb-4 p-4 bg-red-900/50 border border-red-700 rounded-lg text-red-200">
			{error}
		</div>
	{/if}

	{#if loading}
		<div class="flex items-center justify-center py-12">
			<div class="animate-spin rounded-full h-8 w-8 border-b-2 border-blue-500"></div>
		</div>
	{:else if profiles.length === 0}
		<div class="bg-zinc-900 border border-zinc-800 rounded-xl">
			<EmptyState
				title="No access profiles"
				description="Create your first access profile to configure row-level security."
				icon="lock"
			/>
		</div>
	{:else}
		<div class="grid gap-4">
			{#each profiles as profile (profile.id)}
				<AccessProfileCard
					{profile}
					onEdit={() => handleEdit(profile)}
					onDelete={() => handleDelete(profile.id)}
				/>
			{/each}
		</div>
	{/if}
</div>

{#if editingProfile || isCreating}
	<AccessProfileEditor
		profile={editingProfile}
		onClose={handleEditorClose}
	/>
{/if}

<style>
	/* Custom styles for access page */
</style>
