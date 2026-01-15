/**
 * PAP DSL Types
 * 
 * These types mirror the kernel's data DSL (src/protocol/data).
 * They act as the source of truth for the Data Explorer UI.
 */

export interface QueryInput {
    select: string[];
    where: FilterOp | null;
    joins: JoinSpec[];
    order_by: OrderSpec[];
    limit: number | null;
    offset: number | null;
}

export type FilterValue = string | number | boolean | null | "$subject";

export type FilterOp =
    | { and: FilterOp[] }
    | { or: FilterOp[] }
    | { not: FilterOp }
    | { eq: [string, FilterValue] }
    | { ne: [string, FilterValue] }
    | { gt: [string, FilterValue] }
    | { lt: [string, FilterValue] }
    | { gte: [string, FilterValue] }
    | { lte: [string, FilterValue] }
    | { in: [string, FilterValue[]] }
    | { like: [string, string] }
    | { is_null: string }
    | { is_not_null: string };

export interface JoinSpec {
    relation: string;
    as: string;
    type: "left" | "inner";
    select: string[];
    where: FilterOp | null;
}

export interface OrderSpec {
    field: string;
    dir: "asc" | "desc";
}

// Minimal row type for rendering
export type Row = Record<string, unknown>;

// Execution mode from kernel observability
export type ExecutionMode = "FAST" | "SLOW";
