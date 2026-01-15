/**
 * Kernel Pipeline
 * 
 * Two-phase capability-based execution:
 * 1. requestCapability - declare intent, receive short-lived token
 * 2. execute - use token to perform operation
 * 
 * This mirrors the kernel's pipeline exactly.
 * See: src/transport/pipeline.rs
 */

import { kernelFetch } from './http';
import type { OpRequestInput, OpRequest, CapGrant } from './types';
import { auth } from '../state/auth.svelte';

// Generate unique request ID
function generateRequestId(): string {
    return `req-${Date.now()}-${Math.random().toString(36).slice(2, 9)}`;
}

/**
 * Phase 1: Request a capability token for an operation.
 * 
 * The caller provides an OpRequestInput (no request_id/timestamp).
 * This function adds the required fields before sending to kernel.
 * 
 * The kernel will:
 * 1. Verify the JWT
 * 2. Evaluate policy against the intent
 * 3. Return a signed, short-lived capability token
 * 
 * @throws KernelRequestError if policy denies the request
 */
export async function requestCapability(input: OpRequestInput): Promise<CapGrant> {
    // Build full OpRequest with auto-generated fields
    const req: OpRequest = {
        request_id: generateRequestId(),
        op: input.op,
        resource: input.resource,
        input: input.input,
        timestamp: Math.floor(Date.now() / 1000), // Unix seconds
    };

    return kernelFetch<CapGrant>('/v1/op/request', {
        method: 'POST',
        body: req,
        jwt: auth.jwt
    });
}

// Generate unique execute ID
function generateExecuteId(): string {
    return `exec-${Date.now()}-${Math.random().toString(36).slice(2, 9)}`;
}

/**
 * Phase 2: Execute an operation with a valid capability token.
 * 
 * The kernel will:
 * 1. Verify the capability token signature
 * 2. Check token expiration
 * 3. Execute within the token's scope
 * 
 * The kernel returns the result directly (not wrapped):
 * - Read ops: the data value (object, array, etc.)
 * - Write ops: { rows_affected: number }
 * - No-op: { status: "no-op" }
 * 
 * @param token - The capability token from requestCapability
 * @param payload - Operation-specific payload
 */
export async function execute<T>(token: string, payload: unknown): Promise<T> {
    return kernelFetch<T>('/v1/op/execute', {
        method: 'POST',
        body: {
            execute_id: generateExecuteId(),
            token,
            payload,
            timestamp: Math.floor(Date.now() / 1000)
        },
        jwt: auth.jwt
    });
}

