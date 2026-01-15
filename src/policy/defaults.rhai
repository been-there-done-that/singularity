// Default System Policy
//
// This policy enforces the core security model of Singularity:
// 1. Administrators have bypass authority.
// 2. Schema introspection is allowed for authenticated users (filtered by execution).
// 3. Creation is open to authenticated users.
// 4. Resource access is strictly governed by OWNERSHIP.
//
// The 'owner' keyword is injected by the PolicyEngine and guarantees:
// resource.owner_id == subject.internal_id (Stable Identity Match)

// 1. Admins have full access
if subject.roles.contains("admin") {
    return true;
}

// 2. Schema List (Allow everyone, Execution filters based on ownership)
if op == "schema.list_models" {
    return true;
}

// 3. Create (Allowed for authenticated users)
if op.ends_with(".create") {
    return true;
}

// 4. Ownership Check
// The 'owner' keyword is a system-verified boolean (resource.owner_id == subject.internal_id)
if owner {
    return true;
}

// 5. Default Deny
return false;
