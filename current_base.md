# Singularity System Documentation - Current Base

**Date:** 2026-01-16
**Status:** v0 Implementation Complete (MVP)

## 1. System Overview

Singularity is a **capability-based data access kernel** that decouples identity from execution. It enforces secure data access through a standardized **Plan-Authorize-Plan (PAP)** pipeline, ensuring that every operation is cryptographically authorized before execution.

### Core Philosophy
- **Identity != Authority**: Identity (Who you are) is converted into Capabilities (What you can do) via Policy.
- **Zero-Trust Execution**: The execution layer only accepts cryptographically verified Capability Tokens.
- **Transport Agnostic**: Logic resides in the kernel, accessible via HTTP or internal gRPC.
- **Policy-Aware**: Fine-grained access control (Row-level, Field-level) is baked into the planning phase.

---

## 2. Architecture Components

### 2.1 The PAP Pipeline (`src/transport/pipeline.rs`)

The heart of the system is the **Plan-Authorize-Plan** loop:

1.  **Request**: User sends an `OpRequest` (e.g., `data.query`) + JWT.
2.  **Planning (Stage 1)**: The `Planner` converts the raw input into a **Logical Plan**, validating structure and resolving schema references (`src/planner/`).
3.  **Authorization**: The `Policy Engine` evaluates the Logical Plan against Rhai policies (`src/policy/`).
    -   *Input*: Subject (User), Plan, Context.
    -   *Output*: `PlanGrant` (Row predicates, Field masks, Limits) or Denial.
4.  **Minting**: A **Capability Token** is signed (Ed25519), embedding the `PlanGrant` and a hash of the Plan.
5.  **Execution**: The client returns the Token + Original Payload to `OpExecute`.
6.  **Verification (Stage 2)**: The Executor verifies the Token signature and re-computes the Plan hash to ensure integrity.
7.  **Final Execution**: The `StateBackedExecutor` executes the plan against the storage backend.

### 2.2 Core Modules

-   **`src/protocol/`**: Defines the wire format (Opcodes, Requests, Grants, Dsl).
    -   **Opcodes**: `data.insert`, `data.query`, `data.update`, `data.delete`, `data.count`, `schema.*`, `model.*`.
    -   **DSL**: Strictly typed query language (`QueryInput`, `MutationInput`, `FilterOp`).

-   **`src/planner/`**: Query Compiler.
    -   **Validation**: Checks for unknown fields, invalid types.
    -   **Resolution**: Maps names to internal IDs.
    -   **Output**: `LogicalPlan` (Safe, deterministically hashable).

-   **`src/policy/`**: Access Control Logic.
    -   **Engine**: Embedded Rhai scripting engine.
    -   **Enforcement**: Generates `RowPredicate` (SQL `WHERE` clauses) and `FieldMask` (Select/Update allowlists).

-   **`src/execution/`**: Operation Runners.
    -   **`StateBackedExecutor`**: The main entry point for `OpExecute`.
    -   **`SqliteState`**: The reference state implementation.
    -   **Handlers**: Logic for each opcode (`DATA_INSERT`, `DATA_QUERY`, etc.).

-   **`src/state/sqlite.rs`**: Storage Engine.
    -   Manages SQLite connection.
    -   Handles Schema (Migration, Introspection).
    -   Handles Data (Dynamic SQL generation via `sql/builder.rs`).
    -   **System Fields**: Enforces `id`, `created_at`, `updated_at`, `owner_id`.

-   **`src/object/`**: Object Storage.
    -   **`LocalFsStore`**: Local filesystem implementation for file uploads/downloads.
    -   **Presigned URLs**: Decouples data transfer from kernel.

---

## 3. Implemented Features

### 3.1 Data Operations (CRUD)
Fully supported via specific Opcodes:
-   **`data.insert`**: Create new records.
    -   *Auto-injection*: `id` (UUID), `created_at`, `updated_at`, `owner_id`.
-   **`data.query`**: Retrieve records with filtering, sorting, pagination.
    -   *Security*: Automatically applies Row Predicates (e.g., `WHERE owner_id = ?`) and Field Masks.
-   **`data.update`**: Modify existing records by ID.
    -   *Auto-injection*: Updates `updated_at` timestamp.
-   **`data.delete`**: Remove records by ID.
-   **`data.count`**: Efficient counting matching query criteria.

### 3.2 Security Features
-   **JWT Authentication**: Verifies identity providers.
-   **Role-Based Access Control (RBAC)**: Policies can check `subject.roles`.
-   **Ownership**: Automatic `owner_id` tracking and enforcement.
-   **Field-Level Security**: Policies can mask specific fields (e.g., `read_only` or hidden).
-   **Plan Hashing**: Ensures the details of an operation cannot change between Authorization and Execution.

### 3.3 Bootstrap & Meta
-   **Bootstrap Admin**: Auto-generates an Admin code on first launch to claim root access.
-   **Schema Management**: Dynamic model creation and field definition via API.

---

## 4. User Interface (SvelteKit)

The UI providing a visual interface for the kernel: `ui/`

### 4.1 Data Explorer (`/data`)
-   **Visual Grid**: Powered by `ResultsTable.svelte`.
-   **CRUD Actions**:
    -   **Insert**: `RecordDialog.svelte` for creating records.
    -   **Edit**: Update existing records via pre-filled dialog.
    -   **Delete**: One-click deletion with confirmation.
-   **Filter Builder**:
    -   `FilterBuilder.svelte`: Visual UI to construct complex filters (`=`, `!=`, `like`, `>`).
    -   **Live DSL**: Toggling "Show DSL" reveals the generated JSON query.

### 4.2 Schema Editor (`/schema`)
-   View and manage Models and Fields.

### 4.3 Architecture
-   **Client-Side Rendering**: Svelte 5 (Runes).
-   **Kernel Bridge**: `lib/kernel/pipeline.ts` interacts directly with the Rust backend via HTTP.

---

## 5. Directory Map

```text
/
├── src/
│   ├── main.rs            # Entry point (HTTP/gRPC/Sqlite setup)
│   ├── protocol/          # Wire types (Request/Response/DSL)
│   ├── planner/           # Query validation & planning
│   ├── policy/            # Authz logic (Rhai integration)
│   ├── execution/         # Executor & State bindings
│   ├── state/             # SQLite backend implementation
│   ├── transport/         # API Pipeline (HTTP/gRPC)
│   └── object/            # File storage logic
├── ui/
│   ├── src/
│   │   ├── lib/
│   │   │   ├── kernel/    # Client SDK
│   │   │   ├── explorer/  # UI Components (Table, Dialog, Filter)
│   │   │   └── dsl/       # Types
│   │   └── routes/
│   │       ├── data/      # Data Explorer Page
│   │       └── schema/    # Schema Manager
└── tests/                 # E2E & Contract tests
```
