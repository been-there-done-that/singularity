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
import type { OpRequest, CapGrant, ExecutionResult } from './types';
import { auth } from '../state/auth.svelte';

/**
 * Phase 1: Request a capability token for an operation.
 * 
 * The kernel will:
 * 1. Verify the JWT
 * 2. Evaluate policy against the intent
 * 3. Return a signed, short-lived capability token
 * 
 * @throws KernelRequestError if policy denies the request
 */
export async function requestCapability(req: OpRequest): Promise<CapGrant> {
    return kernelFetch<CapGrant>('/v1/op/request', {
        method: 'POST',
        body: req,
        jwt: auth.jwt
    });
}

/**
 * Phase 2: Execute an operation with a valid capability token.
 * 
 * The kernel will:
 * 1. Verify the capability token signature
 * 2. Check token expiration
 * 3. Execute within the token's scope
 * 
 * @param token - The capability token from requestCapability
 * @param payload - Operation-specific payload
 */
export async function execute<T>(token: string, payload: unknown): Promise<ExecutionResult<T>> {
    return kernelFetch<ExecutionResult<T>>('/v1/op/execute', {
        method: 'POST',
        body: { token, payload },
        jwt: auth.jwt
    });
}
