/**
 * Capability Guard - CORE SECURITY BOUNDARY
 * 
 * This is the SINGLE MOST IMPORTANT security pattern in the UI.
 * 
 * INVARIANTS (DO NOT VIOLATE):
 * - Capability tokens NEVER escape the closure scope
 * - Capability tokens NEVER touch $state or $derived
 * - Capability tokens NEVER pass through props
 * - Capability tokens NEVER appear in event details
 * 
 * Usage:
 * ```ts
 * await withCapability(
 *   { op: 'schema.list', resource: { ... }, input: {} },
 *   async (token) => {
 *     const result = await execute(token, {});
 *     return result;
 *   }
 * );
 * ```
 */

import { requestCapability, execute } from './pipeline';
import type { OpRequestInput } from './types';

/**
 * Execute an operation within a scoped capability.
 * 
 * The capability token is:
 * - Requested from the kernel
 * - Passed to the executor function
 * - Dropped when the function completes
 * 
 * ❌ This function MUST NOT return the token
 * ❌ The executor MUST NOT store the token
 * 
 * @param req - The operation request (intent declaration)
 * @param executor - Function that uses the token to execute the operation
 * @returns The result from the executor, NOT the token
 */
export async function withCapability<T>(
    req: OpRequestInput,
    executor: (token: string) => Promise<T>
): Promise<T> {
    // Phase 1: Request capability
    const grant = await requestCapability(req);

    try {
        // Phase 2: Execute with scoped token
        return await executor(grant.token);
    } finally {
        // Token is dropped here - no cleanup needed
        // The token variable goes out of scope and is garbage collected
    }
}

/**
 * Convenience wrapper that handles the common execute pattern.
 * 
 * For simple operations where you just need to call execute with a payload.
 * Returns the kernel response directly (not wrapped).
 */
export async function executeWithCapability<T>(
    req: OpRequestInput,
    payload: unknown = {}
): Promise<T> {
    return withCapability(req, async (token) => {
        return execute<T>(token, payload);
    });
}

