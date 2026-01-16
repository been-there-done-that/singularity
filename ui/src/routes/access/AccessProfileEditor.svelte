<script lang="ts">
	/**
	 * Access Profile Editor
	 * Modal for creating/editing access profiles with safe defaults.
	 */
	import { Button } from '$lib/ui';
	import { createProfile, updateProfile, type AccessProfile, type AccessProfileInput, type AccessProfileUpdate } from '$lib/kernel';

	interface Props {
		profile: AccessProfile | null;  // null = create mode
		onClose: () => void;
	}

	let { profile, onClose }: Props = $props();

	const isCreating = profile === null;

	// Form state
	let model_id = $state(profile?.model_id ?? '');
	let principal_type = $state<'user' | 'group' | 'role'>(profile?.principal_type ?? 'user');
	let principal_id = $state(profile?.principal_id ?? '');
	let allow_query = $state(profile?.allow_query ?? false);
	let allow_insert = $state(profile?.allow_insert ?? false);
	let allow_update = $state(profile?.allow_update ?? false);
	let allow_delete = $state(profile?.allow_delete ?? false);
	let priority = $state(profile?.priority ?? 0);

	// Row scopes (simplified: same scope for all ops)
	let query_scope = $state<string>(profile?.row_scopes?.query ?? 'owner');
	let insert_scope = $state<string>(profile?.row_scopes?.insert ?? 'owner');
	let update_scope = $state<string>(profile?.row_scopes?.update ?? 'owner');
	let delete_scope = $state<string>(profile?.row_scopes?.delete ?? 'owner');

	let saving = $state(false);
	let error = $state<string | null>(null);

	async function handleSubmit(e: Event) {
		e.preventDefault();
		saving = true;
		error = null;

		try {
			const row_scopes: Record<string, string> = {};
			if (allow_query) row_scopes.query = query_scope;
			if (allow_insert) row_scopes.insert = insert_scope;
			if (allow_update) row_scopes.update = update_scope;
			if (allow_delete) row_scopes.delete = delete_scope;

			if (isCreating) {
				const input: AccessProfileInput = {
					model_id,
					principal_type,
					principal_id,
					allow_query,
					allow_insert,
					allow_update,
					allow_delete,
					priority,
					row_scopes
				};
				await createProfile(input);
			} else {
				const update: AccessProfileUpdate = {
					id: profile!.id,
					allow_query,
					allow_insert,
					allow_update,
					allow_delete,
					priority,
					row_scopes
				};
				await updateProfile(update);
			}
			onClose();
		} catch (e: any) {
			error = e.message ?? 'Failed to save profile';
		} finally {
			saving = false;
		}
	}
</script>

<!-- Backdrop -->
<div class="fixed inset-0 bg-black/60 z-50 flex items-center justify-center p-4">
	<!-- Modal -->
	<div class="bg-zinc-900 border border-zinc-700 rounded-xl w-full max-w-lg shadow-2xl">
		<div class="p-6 border-b border-zinc-800">
			<h2 class="text-xl font-bold text-white">
				{isCreating ? 'Create Access Profile' : 'Edit Access Profile'}
			</h2>
			<p class="text-sm text-zinc-400 mt-1">Configure row-level security permissions</p>
		</div>

		<form onsubmit={handleSubmit} class="p-6 space-y-5">
			{#if error}
				<div class="p-3 bg-red-900/50 border border-red-700 rounded-lg text-red-200 text-sm">
					{error}
				</div>
			{/if}

			<!-- Principal Section -->
			<div class="space-y-4">
				<div>
					<label class="block text-sm font-medium text-zinc-300 mb-1">Model ID</label>
					<input
						type="text"
						bind:value={model_id}
						disabled={!isCreating}
						placeholder="e.g., todos"
						class="w-full px-3 py-2 bg-zinc-800 border border-zinc-700 rounded-lg text-white placeholder-zinc-500 focus:border-blue-500 focus:ring-1 focus:ring-blue-500 disabled:opacity-50"
						required
					/>
				</div>

				<div class="grid grid-cols-2 gap-4">
					<div>
						<label class="block text-sm font-medium text-zinc-300 mb-1">Principal Type</label>
						<select
							bind:value={principal_type}
							disabled={!isCreating}
							class="w-full px-3 py-2 bg-zinc-800 border border-zinc-700 rounded-lg text-white focus:border-blue-500 disabled:opacity-50"
						>
							<option value="user">User</option>
							<option value="group">Group</option>
							<option value="role">Role</option>
						</select>
					</div>
					<div>
						<label class="block text-sm font-medium text-zinc-300 mb-1">Principal ID</label>
						<input
							type="text"
							bind:value={principal_id}
							disabled={!isCreating}
							placeholder="e.g., admin or user-123"
							class="w-full px-3 py-2 bg-zinc-800 border border-zinc-700 rounded-lg text-white placeholder-zinc-500 focus:border-blue-500 disabled:opacity-50"
							required
						/>
					</div>
				</div>

				<div>
					<label class="block text-sm font-medium text-zinc-300 mb-1">Priority</label>
					<input
						type="number"
						bind:value={priority}
						min="0"
						max="1000"
						class="w-24 px-3 py-2 bg-zinc-800 border border-zinc-700 rounded-lg text-white focus:border-blue-500"
					/>
					<p class="text-xs text-zinc-500 mt-1">Higher priority profiles take precedence</p>
				</div>
			</div>

			<!-- Permissions Section -->
			<div class="border-t border-zinc-800 pt-5">
				<h3 class="text-sm font-medium text-zinc-300 mb-3">Permissions & Row Scopes</h3>
				
				<div class="space-y-3">
					<!-- Query -->
					<div class="flex items-center gap-4">
						<label class="flex items-center gap-2 w-28">
							<input type="checkbox" bind:checked={allow_query} class="rounded bg-zinc-800 border-zinc-600 text-blue-500 focus:ring-blue-500" />
							<span class="text-sm text-zinc-300">Query</span>
						</label>
						{#if allow_query}
							<select bind:value={query_scope} class="flex-1 px-3 py-1.5 text-sm bg-zinc-800 border border-zinc-700 rounded text-white">
								<option value="owner">Owner only (owner_id = subject)</option>
								<option value="all">All rows</option>
								<option value="deny">Deny</option>
							</select>
						{/if}
					</div>

					<!-- Insert -->
					<div class="flex items-center gap-4">
						<label class="flex items-center gap-2 w-28">
							<input type="checkbox" bind:checked={allow_insert} class="rounded bg-zinc-800 border-zinc-600 text-blue-500 focus:ring-blue-500" />
							<span class="text-sm text-zinc-300">Insert</span>
						</label>
						{#if allow_insert}
							<select bind:value={insert_scope} class="flex-1 px-3 py-1.5 text-sm bg-zinc-800 border border-zinc-700 rounded text-white">
								<option value="owner">Owner only</option>
								<option value="all">All</option>
								<option value="deny">Deny</option>
							</select>
						{/if}
					</div>

					<!-- Update -->
					<div class="flex items-center gap-4">
						<label class="flex items-center gap-2 w-28">
							<input type="checkbox" bind:checked={allow_update} class="rounded bg-zinc-800 border-zinc-600 text-blue-500 focus:ring-blue-500" />
							<span class="text-sm text-zinc-300">Update</span>
						</label>
						{#if allow_update}
							<select bind:value={update_scope} class="flex-1 px-3 py-1.5 text-sm bg-zinc-800 border border-zinc-700 rounded text-white">
								<option value="owner">Owner only</option>
								<option value="all">All rows</option>
								<option value="deny">Deny</option>
							</select>
						{/if}
					</div>

					<!-- Delete -->
					<div class="flex items-center gap-4">
						<label class="flex items-center gap-2 w-28">
							<input type="checkbox" bind:checked={allow_delete} class="rounded bg-zinc-800 border-zinc-600 text-blue-500 focus:ring-blue-500" />
							<span class="text-sm text-zinc-300">Delete</span>
						</label>
						{#if allow_delete}
							<select bind:value={delete_scope} class="flex-1 px-3 py-1.5 text-sm bg-zinc-800 border border-zinc-700 rounded text-white">
								<option value="owner">Owner only</option>
								<option value="all">All rows</option>
								<option value="deny">Deny</option>
							</select>
						{/if}
					</div>
				</div>

				<p class="text-xs text-zinc-500 mt-3">
					⚠️ "All rows" grants access to ALL data in the model. Use with caution.
				</p>
			</div>
		</form>

		<div class="p-4 border-t border-zinc-800 flex justify-end gap-3">
			<Button onclick={onClose} variant="ghost" disabled={saving}>
				Cancel
			</Button>
			<Button onclick={handleSubmit} variant="primary" disabled={saving}>
				{#if saving}
					Saving...
				{:else}
					{isCreating ? 'Create Profile' : 'Save Changes'}
				{/if}
			</Button>
		</div>
	</div>
</div>
