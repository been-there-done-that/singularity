/**
 * Kernel Protocol Types
 * 
 * These types mirror the kernel's protocol layer.
 * See: src/protocol/request.rs, src/protocol/grant.rs, src/protocol/execute.rs
 */

// Resource identifier for capability scoping
export interface Resource {
    resource_type: string;
    resource_id: string | null; // null for collection operations
}

// Phase 1: Request capability (intent declaration)
// This is what callers provide (request_id and timestamp are auto-generated)
export interface OpRequestInput {
    op: string;
    resource: Resource;
    input: unknown;
}

// Full OpRequest sent to kernel (includes auto-generated fields)
export interface OpRequest {
    request_id: string;
    op: string;
    resource: Resource;
    input: unknown;
    timestamp: number;
}

// Capability grant response from kernel
export interface CapGrant {
    token: string;
    expires_at: number;
    scope: string[];
}

// Phase 2: Execute with capability
export interface OpExecute {
    token: string;
    payload: unknown;
}

// Execution result wrapper
export interface ExecutionResult<T = unknown> {
    success: boolean;
    data?: T;
    error?: KernelError;
}

// Kernel error shape
export interface KernelError {
    code: string;
    message: string;
    details?: Record<string, unknown>;
}

// Subject from JWT claims
export interface Subject {
    id: string;
    roles: string[];
}
