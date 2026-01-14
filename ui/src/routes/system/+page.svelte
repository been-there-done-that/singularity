<script lang="ts">
	/**
	 * System - Admin Only
	 */
	import { EmptyState } from '$lib/ui';
</script>

<div class="max-w-6xl mx-auto">
	<div class="mb-8">
		<h1 class="text-3xl font-bold text-white">System</h1>
		<p class="mt-2 text-zinc-400">Database and health status</p>
	</div>

	<div class="bg-zinc-900 border border-zinc-800 rounded-xl">
		<EmptyState
			title="Coming soon"
			description="System health, migration status, and diagnostics."
			icon="info"
		/>
	</div>
</div>
