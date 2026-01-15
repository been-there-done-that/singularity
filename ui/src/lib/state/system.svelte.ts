/**
 * System State - Svelte 5 Runes
 * 
 * Reflects the kernel's health and readiness state.
 * Specifically handles the "bootstrapping" state for one-time admin setup.
 */

import { checkHealth } from '../kernel/http';
import type { SystemStatus } from '../kernel/types';

class SystemStore {
    status = $state<SystemStatus | null>(null);
    isChecking = $state(false);
    error = $state<string | null>(null);

    /**
     * Refresh system status from kernel.
     * Should be called on app mount and after critical transitions (like setup).
     */
    async refreshStatus() {
        this.isChecking = true;
        this.error = null;
        try {
            const health = await checkHealth();
            this.status = health.status;
        } catch (e) {
            console.error('System health check failed:', e);
            this.status = 'degraded';
            this.error = e instanceof Error ? e.message : 'Unknown error';
        } finally {
            this.isChecking = false;
        }
    }
}

export const system = new SystemStore();
