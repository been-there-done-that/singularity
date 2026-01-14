/**
 * Authentication State - Svelte 5 Runes
 * 
 * SECURITY RULES:
 * ✅ JWT is allowed in $state (identity only)
 * ❌ Capabilities are NEVER stored here
 * 
 * The subject is derived from JWT claims but is NOT authoritative.
 * All authorization is enforced by the kernel, not the UI.
 */

import type { Subject } from '../kernel/types';

// Use a class to encapsulate reactive state for module exports
class AuthState {
    jwt = $state<string | null>(null);
    subject = $state<Subject | null>(null);

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
