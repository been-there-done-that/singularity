<script lang="ts">
	/**
	 * Access Profile Card
	 * Displays a single access profile with edit/delete actions.
	 */
	import { Button } from '$lib/ui';
	import type { AccessProfile } from '$lib/kernel';

	interface Props {
		profile: AccessProfile;
		onEdit: () => void;
		onDelete: () => void;
	}

	let { profile, onEdit, onDelete }: Props = $props();

	const scopeLabels: Record<string, string> = {
		owner: 'Owner only',
		all: 'All rows',
		deny: 'Denied'
	};

	function getScopeLabel(scope: {type: string} | string): string {
		const scopeType = typeof scope === 'string' ? scope : scope?.type ?? 'unknown';
		return scopeLabels[scopeType] ?? scopeType;
	}

	function formatDate(ts: number): string {
		return new Date(ts * 1000).toLocaleDateString();
	}
</script>

<div class="bg-zinc-900 border border-zinc-800 rounded-xl p-5 hover:border-zinc-700 transition-colors">
	<div class="flex items-start justify-between">
		<div class="flex-1">
			<div class="flex items-center gap-3 mb-2">
				<span class="px-2 py-0.5 text-xs font-medium rounded bg-blue-900/50 text-blue-300 uppercase">
					{profile.principal_type}
				</span>
				<h3 class="text-lg font-semibold text-white">{profile.principal_id}</h3>
				{#if profile.priority > 0}
					<span class="text-xs text-zinc-500">Priority: {profile.priority}</span>
				{/if}
			</div>
			<p class="text-sm text-zinc-400 mb-3">Model: <code class="text-zinc-300">{profile.model_id}</code></p>

			<!-- Permissions -->
			<div class="flex flex-wrap gap-2 mb-3">
				{#if profile.allow_query}
					<span class="px-2 py-1 text-xs rounded bg-green-900/30 text-green-400 border border-green-800">Query</span>
				{/if}
				{#if profile.allow_insert}
					<span class="px-2 py-1 text-xs rounded bg-green-900/30 text-green-400 border border-green-800">Insert</span>
				{/if}
				{#if profile.allow_update}
					<span class="px-2 py-1 text-xs rounded bg-green-900/30 text-green-400 border border-green-800">Update</span>
				{/if}
				{#if profile.allow_delete}
					<span class="px-2 py-1 text-xs rounded bg-green-900/30 text-green-400 border border-green-800">Delete</span>
				{/if}
				{#if !profile.allow_query && !profile.allow_insert && !profile.allow_update && !profile.allow_delete}
					<span class="px-2 py-1 text-xs rounded bg-red-900/30 text-red-400 border border-red-800">No Permissions</span>
				{/if}
			</div>

			<!-- Row Scopes -->
			{#if Object.keys(profile.row_scopes).length > 0}
				<div class="flex flex-wrap gap-2">
					{#each Object.entries(profile.row_scopes) as [opcode, scope]}
						<span class="px-2 py-1 text-xs rounded bg-zinc-800 text-zinc-300">
							{opcode}: <span class="text-zinc-400">{getScopeLabel(scope)}</span>
						</span>
					{/each}
				</div>
			{/if}
		</div>

		<!-- Actions -->
		<div class="flex gap-2 ml-4">
			<button
				onclick={onEdit}
				class="p-2 rounded-lg hover:bg-zinc-800 text-zinc-400 hover:text-white transition-colors"
				title="Edit"
			>
				<svg class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
					<path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M11 5H6a2 2 0 00-2 2v11a2 2 0 002 2h11a2 2 0 002-2v-5m-1.414-9.414a2 2 0 112.828 2.828L11.828 15H9v-2.828l8.586-8.586z" />
				</svg>
			</button>
			<button
				onclick={onDelete}
				class="p-2 rounded-lg hover:bg-red-900/50 text-zinc-400 hover:text-red-400 transition-colors"
				title="Delete"
			>
				<svg class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
					<path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M19 7l-.867 12.142A2 2 0 0116.138 21H7.862a2 2 0 01-1.995-1.858L5 7m5 4v6m4-6v6m1-10V4a1 1 0 00-1-1h-4a1 1 0 00-1 1v3M4 7h16" />
				</svg>
			</button>
		</div>
	</div>

	<div class="mt-3 pt-3 border-t border-zinc-800 text-xs text-zinc-500">
		Created {formatDate(profile.created_at)} · Updated {formatDate(profile.updated_at)}
	</div>
</div>
