# Kernel Invariants

> **This document is a contract, not documentation.**
> Breaking these invariants is a security bug.

---

## v0 Guarantees

### 1. Pipeline Integrity

Every `data.*` operation passes through:

```
Planner → Policy → Executor
```

No shortcuts. No bypasses. Ever.

**Guarded by:** `contract_data_pipeline_integrity`

---

### 2. Determinism

| Component | Invariant |
|-----------|-----------|
| Planner | Same input → identical LogicalPlan |
| Policy | Same plan + subject → identical PlanGrant |
| Hash | Same plan + grant → identical plan_hash |

**Guarded by:** `test_same_input_produces_identical_plans`, `contract_same_input_produces_identical_grants`, `contract_plan_hash_determinism`

---

### 3. Separation of Concerns

| Layer | Decides | Must NOT |
|-------|---------|----------|
| Planner | "What does this mean?" | Touch DB, evaluate policy |
| Policy | "Yes/No + constraints" | Mint authority, mutate state |
| Executor | "Execute with enforcement" | Re-authorize, interpret policy |

**Guarded by:** Module structure + compile-time dependencies

---

### 4. Row Security

| Subject | Sees |
|---------|------|
| Admin | All rows (`RowPredicate::Always`) |
| User | Only rows where `owner_id = $subject` |
| Unauthenticated | Denied |

**Guarded by:** `contract_admin_sees_all_rows`, `contract_user_sees_only_own_rows`, `contract_unauthenticated_denied`

---

### 5. Bulk Safety

| Rule | Enforcement |
|------|-------------|
| Non-admin UPDATE/DELETE requires WHERE | `WhereRequired` error |
| Bulk limit enforced | `BulkLimitExceeded` error |
| Admin can bypass WHERE | `requires_where: false` |

**Guarded by:** `contract_update_requires_where_for_user`, `contract_delete_requires_where_for_user`, `contract_admin_can_update_without_where`

---

### 6. Query/Count Invariant

```
query(filter).len() == count(filter)
```

Always. No exceptions.

**Guarded by:** `contract_query_count_invariant`, `contract_query_count_invariant_with_user_predicate`

---

### 7. Capability Binding

```rust
Capability.plan_hash == compute_plan_hash(plan, grant)
```

Prevents mismatched plan/grant reuse.

**Guarded by:** `test_verify_plan_hash_mismatch`

---

## v0 Explicit Non-Goals

These are **not bugs** — they are consciously deferred:

| Feature | Status | Hook |
|---------|--------|------|
| Dynamic per-row Cedar evaluation | Deferred | `RowPredicate::Dynamic` exists |
| Arbitrary joins (non-FK) | Deferred | `__relations` table exists |
| Cross-model policies | Deferred | `PlanAuthContext` extensible |
| Streaming queries | Deferred | N/A |

---

## Rules You Must Not Break

### ❌ Never call Executor without Policy

```rust
// WRONG
let result = executor::execute(&plan, &grant, conn, subject);

// RIGHT
let grant = authorizer.authorize(&plan, &ctx)?;
let result = executor::execute(&plan, &grant, conn, subject)?;
```

### ❌ Never skip Planner for data operations

```rust
// WRONG
let sql = format!("SELECT * FROM {} WHERE ...", table);

// RIGHT
let plan = planner::plan_query("items", &input, &schema)?;
```

### ❌ Never widen access in Policy

Policy produces constraints. It never:
- Mints authority
- Adds fields to selection
- Removes predicates
- Ignores bulk limits

### ❌ Never re-authorize in Executor

Executor enforces PlanGrant blindly. It never:
- Inspects roles
- Queries policy rules
- "Fixes" bad grants
- Silently drops forbidden content

---

## Test Coverage Map

| Invariant | Primary Tests |
|-----------|---------------|
| Pipeline integrity | `contract_pipeline_integrity.rs` |
| Row security | `contract_executor.rs` |
| Bulk safety | `contract_executor.rs` |
| Planner contracts | `contract_planner.rs` |
| Policy contracts | `contract_policy_plangrant.rs` |
| Capability binding | `executor::tests` |

---

## When to Update This Document

- Adding a new invariant
- Changing v0 guarantees to v1
- Discovering a missing contract test

Do not update for feature work that doesn't touch kernel invariants.
