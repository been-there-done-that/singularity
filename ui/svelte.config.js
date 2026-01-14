import adapter from '@sveltejs/adapter-static';
import { vitePreprocess } from '@sveltejs/vite-plugin-svelte';

/** @type {import('@sveltejs/kit').Config} */
const config = {
	preprocess: vitePreprocess(),

	kit: {
		adapter: adapter({
			// SPA mode: single fallback page for client-side routing
			fallback: 'index.html',
			strict: false
		})
	}
};

export default config;
