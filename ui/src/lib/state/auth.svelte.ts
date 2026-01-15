/**
 * Authentication State - Svelte 5 Runes
 * 
 * SECURITY RULES:
 * ✅ JWT stored in sessionStorage (cleared when tab closes)
 * ❌ Capabilities are NEVER stored here
 * 
 * The subject is derived from JWT claims but is NOT authoritative.
 * All authorization is enforced by the kernel, not the UI.
 */

import type { Subject } from '../kernel/types';
import { browser } from '$app/environment';

const STORAGE_KEY = 'singularity_jwt';

// Use a class to encapsulate reactive state for module exports
class AuthState {
    jwt = $state<string | null>(null);
    subject = $state<Subject | null>(null);

    constructor() {
        // Restore from sessionStorage on initialization (client-side only)
        if (browser) {
            const stored = sessionStorage.getItem(STORAGE_KEY);
            if (stored) {
                this.restoreFromToken(stored);
            }
        }
    }

    private restoreFromToken(jwt: string): void {
        try {
            // Check if expired first
            const payload = jwt.split('.')[1];
            const decoded = JSON.parse(atob(payload.replace(/-/g, '+').replace(/_/g, '/')));
            const exp = decoded.exp;

            if (exp && Date.now() >= exp * 1000) {
                // Token expired, clear storage
                sessionStorage.removeItem(STORAGE_KEY);
                return;
            }

            this.jwt = jwt;
            this.subject = {
                id: decoded.sub,
                roles: decoded.roles ?? []
            };
        } catch {
            // Invalid token, clear storage
            sessionStorage.removeItem(STORAGE_KEY);
        }
    }

    // Computed admin flag - avoids role checks scattered everywhere
    get isAdmin(): boolean {
        return this.subject?.roles.includes('admin') ?? false;
    }

    // Computed authentication status
    get isAuthenticated(): boolean {
        return this.jwt !== null && this.subject !== null;
    }
}

// Export the singleton auth state
// Access computed values via auth.isAdmin and auth.isAuthenticated
export const auth = new AuthState();

// Export getter functions for components that need direct access
// (Svelte 5 doesn't allow exporting $derived from modules)
export function getIsAdmin(): boolean {
    return auth.isAdmin;
}

export function getIsAuthenticated(): boolean {
    return auth.isAuthenticated;
}

/**
 * Set authentication from a JWT.
 * Decodes the JWT payload to extract subject claims.
 * Persists to sessionStorage for page reload survival.
 * 
 * Note: This is for UI display only. The kernel re-verifies the JWT.
 */
export function setAuth(jwt: string): void {
    try {
        // Decode JWT payload (base64url)
        const payload = jwt.split('.')[1];
        const decoded = JSON.parse(atob(payload.replace(/-/g, '+').replace(/_/g, '/')));

        auth.jwt = jwt;
        auth.subject = {
            id: decoded.sub,
            roles: decoded.roles ?? []
        };

        // Persist to sessionStorage
        if (browser) {
            sessionStorage.setItem(STORAGE_KEY, jwt);
        }
    } catch {
        // Invalid JWT format - clear auth
        clearAuth();
    }
}

/**
 * Clear authentication state.
 * Called on logout or JWT expiration.
 */
export function clearAuth(): void {
    auth.jwt = null;
    auth.subject = null;

    if (browser) {
        sessionStorage.removeItem(STORAGE_KEY);
    }
}

/**
 * Check if JWT is expired.
 * Returns true if expired or invalid.
 */
export function isJwtExpired(): boolean {
    if (!auth.jwt) return true;

    try {
        const payload = auth.jwt.split('.')[1];
        const decoded = JSON.parse(atob(payload.replace(/-/g, '+').replace(/_/g, '/')));
        const exp = decoded.exp;

        if (!exp) return false; // No expiration claim
        return Date.now() >= exp * 1000;
    } catch {
        return true;
    }
}
