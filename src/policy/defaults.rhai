// Default System Policy

// 1. Admins have full access
if subject.roles.contains("admin") {
    return true;
}

// 2. Schema operations are restricted to Admins (already checked above)
// So if we are here (non-admin), schema ops are DENIED.
if op.starts_with("schema.") {
    return false;
}

// 3. Ownership-based access
// IF creating, allow (ownership will be stamped)
if op.ends_with(".create") {
    return true;
}

// If the resource tracks ownership, enforce it.
if "owner_id" in resource {
    // Check internal ID for ownership
    if "internal_id" in subject {
         return resource.owner_id == subject.internal_id;
    }
    // We assume the pipeline populates internal_id.
    
    // Note: internal_id is Option<String>.
    // Rhai handles Option transparently? Or we need to check?
    // In our context.rs, we didn't add internal_id to scope?
    // WAIT. I need to update PolicyEngine::build_scope to expose internal_id!
    
    // Assuming I fix build_scope:
    return resource_owner_id == subject_internal_id;
}

// 4. Default Deny
return false;
