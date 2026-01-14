/**
 * Kernel HTTP Client
 * 
 * Low-level fetch wrapper for kernel communication.
 * Base URL is configurable via VITE_KERNEL_URL environment variable.
 */

import type { KernelError } from './types';

// Single config point for kernel URL
const KERNEL_BASE_URL = import.meta.env.VITE_KERNEL_URL ?? 'http://localhost:3000';

export class KernelRequestError extends Error {
    constructor(
        public readonly code: string,
        message: string,
        public readonly details?: Record<string, unknown>
    ) {
        super(message);
        this.name = 'KernelRequestError';
    }

    static fromKernelError(err: KernelError): KernelRequestError {
        return new KernelRequestError(err.code, err.message, err.details);
    }
}

/**
 * Typed fetch wrapper for kernel endpoints.
 * 
 * - Automatically includes JWT from auth state
 * - Handles CBOR/JSON content negotiation
 * - Throws KernelRequestError on failures
 */
export async function kernelFetch<T>(
    path: string,
    options: {
        method: 'GET' | 'POST' | 'PUT' | 'DELETE';
        body?: unknown;
        jwt?: string | null;
    }
): Promise<T> {
    const url = `${KERNEL_BASE_URL}${path}`;

    const headers: Record<string, string> = {
        'Content-Type': 'application/json',
        Accept: 'application/json'
    };

    if (options.jwt) {
        headers['Authorization'] = `Bearer ${options.jwt}`;
    }

    const response = await fetch(url, {
        method: options.method,
        headers,
        body: options.body ? JSON.stringify(options.body) : undefined
    });

    if (!response.ok) {
        let errorBody: KernelError;
        try {
            errorBody = await response.json();
        } catch {
            errorBody = {
                code: 'NETWORK_ERROR',
                message: `Request failed: ${response.status} ${response.statusText}`
            };
        }
        throw KernelRequestError.fromKernelError(errorBody);
    }

    // Handle empty responses (204 No Content)
    if (response.status === 204) {
        return undefined as T;
    }

    return response.json();
}
