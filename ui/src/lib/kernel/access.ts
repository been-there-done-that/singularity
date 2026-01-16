/**
 * Access Profile API
 * 
 * Control-plane operations for managing row-level security (RLS) profiles.
 * These operations return direct results (no capability/execute phase).
 */

import { kernelFetch } from './http';
import { auth } from '../state/auth.svelte';

// Types
export interface AccessProfile {
    id: string;
    model_id: string;
    principal_type: 'user' | 'group' | 'role';
    principal_id: string;
    allow_query: boolean;
    allow_insert: boolean;
    allow_update: boolean;
    allow_delete: boolean;
    priority: number;
    row_scopes: Record<string, { type: string }>;
    created_at: number;
    updated_at: number;
}

export interface AccessProfileInput {
    model_id: string;
    principal_type: 'user' | 'group' | 'role';
    principal_id: string;
    allow_query?: boolean;
    allow_insert?: boolean;
    allow_update?: boolean;
    allow_delete?: boolean;
    priority?: number;
    row_scopes?: Record<string, { type: string }>;
}

export interface AccessProfileUpdate {
    id: string;
    allow_query?: boolean;
    allow_insert?: boolean;
    allow_update?: boolean;
    allow_delete?: boolean;
    priority?: number;
    row_scopes?: Record<string, { type: string }>;
}

export interface AccessProfileListResponse {
    total: number;
    profiles: AccessProfile[];
}

// Helper to make access op request (direct result, no execute phase)
async function accessOp<T>(op: string, input: unknown): Promise<T> {
    const req = {
        request_id: `req-${Date.now()}-${Math.random().toString(36).slice(2, 9)}`,
        op,
        resource: { resource_type: '__access_profiles', resource_id: null },
        input,
        timestamp: Math.floor(Date.now() / 1000)
    };

    const grant = await kernelFetch<{ result?: T }>('/v1/op/request', {
        method: 'POST',
        body: req,
        jwt: auth.jwt
    });

    return grant.result as T;
}

/**
 * Create an access profile.
 */
export async function createProfile(input: AccessProfileInput): Promise<{ id: string; created: boolean }> {
    return accessOp('access.create_profile', input);
}

/**
 * List access profiles for a model (or all if model_id is empty).
 */
export async function listProfiles(model_id: string = ''): Promise<AccessProfileListResponse> {
    return accessOp('access.list_profiles', { model_id });
}

/**
 * Get a specific access profile.
 */
export async function getProfile(id: string): Promise<AccessProfile> {
    return accessOp('access.get_profile', { id });
}

/**
 * Update an access profile.
 */
export async function updateProfile(update: AccessProfileUpdate): Promise<{ id: string; updated: boolean }> {
    return accessOp('access.update_profile', update);
}

/**
 * Delete an access profile.
 */
export async function deleteProfile(id: string): Promise<{ id: string; deleted: boolean }> {
    return accessOp('access.delete_profile', { id });
}
