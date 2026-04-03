use std::collections::{HashMap, HashSet};

// =============================================================================
// RBAC — Role-Based Access Control
//
//   Users → Roles → Permissions
//
//   Alice → admin  → [read, write, delete, manage_users]
//   Bob   → editor → [read, write]
//   Eve   → viewer → [read]
//
//   "Can Bob delete a post?" → Bob is editor → editor has [read, write] → NO
//
//   In practice:
//   - Roles stored in DB (user_roles, role_permissions tables)
//   - Cached in JWT claims or Redis for fast lookup
//   - Checked at the API gateway or within each service
// =============================================================================

#[derive(Debug, Clone)]
struct Permission {
    resource: String, // "posts", "users", "settings"
    action: String,   // "read", "write", "delete"
}

impl Permission {
    fn new(resource: &str, action: &str) -> Self {
        Self {
            resource: resource.to_string(),
            action: action.to_string(),
        }
    }

    fn key(&self) -> String {
        format!("{}:{}", self.resource, self.action)
    }
}

struct RbacSystem {
    // role_name → set of "resource:action" permission keys
    roles: HashMap<String, HashSet<String>>,
    // user_id → list of role names
    user_roles: HashMap<String, Vec<String>>,
}

impl RbacSystem {
    fn new() -> Self {
        Self {
            roles: HashMap::new(),
            user_roles: HashMap::new(),
        }
    }

    fn define_role(&mut self, role: &str, permissions: Vec<Permission>) {
        let perms: HashSet<String> = permissions.iter().map(|p| p.key()).collect();
        self.roles.insert(role.to_string(), perms);
    }

    fn assign_role(&mut self, user_id: &str, role: &str) {
        self.user_roles
            .entry(user_id.to_string())
            .or_default()
            .push(role.to_string());
    }

    /// Check: "Can this user perform this action on this resource?"
    fn is_authorized(&self, user_id: &str, resource: &str, action: &str) -> bool {
        let key = format!("{resource}:{action}");

        let roles = match self.user_roles.get(user_id) {
            Some(r) => r,
            None => return false,
        };

        for role in roles {
            if let Some(perms) = self.roles.get(role) {
                if perms.contains(&key) {
                    return true;
                }
            }
        }
        false
    }

    fn get_user_permissions(&self, user_id: &str) -> Vec<String> {
        let mut all_perms = HashSet::new();

        if let Some(roles) = self.user_roles.get(user_id) {
            for role in roles {
                if let Some(perms) = self.roles.get(role) {
                    all_perms.extend(perms.iter().cloned());
                }
            }
        }

        let mut sorted: Vec<String> = all_perms.into_iter().collect();
        sorted.sort();
        sorted
    }
}

pub fn demo() {
    println!("\n  ═══ RBAC (Role-Based Access Control) ═══\n");

    let mut rbac = RbacSystem::new();

    // Define roles with permissions
    println!("    Defining roles:\n");

    rbac.define_role("admin", vec![
        Permission::new("posts", "read"),
        Permission::new("posts", "write"),
        Permission::new("posts", "delete"),
        Permission::new("users", "read"),
        Permission::new("users", "write"),
        Permission::new("users", "delete"),
        Permission::new("settings", "read"),
        Permission::new("settings", "write"),
    ]);
    println!("    admin  → [posts:*, users:*, settings:read/write]");

    rbac.define_role("editor", vec![
        Permission::new("posts", "read"),
        Permission::new("posts", "write"),
        Permission::new("users", "read"),
    ]);
    println!("    editor → [posts:read/write, users:read]");

    rbac.define_role("viewer", vec![
        Permission::new("posts", "read"),
    ]);
    println!("    viewer → [posts:read]");

    // Assign roles to users
    println!("\n    Assigning roles:\n");
    rbac.assign_role("alice", "admin");
    rbac.assign_role("bob", "editor");
    rbac.assign_role("eve", "viewer");
    rbac.assign_role("charlie", "editor");
    rbac.assign_role("charlie", "viewer"); // user can have multiple roles

    println!("    alice   → admin");
    println!("    bob     → editor");
    println!("    eve     → viewer");
    println!("    charlie → editor + viewer");

    // Check permissions
    println!("\n    Authorization checks:\n");

    let checks = [
        ("alice", "posts", "delete", true),
        ("bob", "posts", "write", true),
        ("bob", "posts", "delete", false),
        ("eve", "posts", "read", true),
        ("eve", "posts", "write", false),
        ("charlie", "users", "read", true),
        ("charlie", "settings", "write", false),
        ("unknown", "posts", "read", false),
    ];

    for (user, resource, action, expected) in &checks {
        let allowed = rbac.is_authorized(user, resource, action);
        let icon = if allowed { "✓" } else { "✗" };
        let match_icon = if allowed == *expected { "" } else { " ⚠ UNEXPECTED" };
        println!(
            "    Can {user:8} {action:6} {resource:10}? {icon} {allowed}{match_icon}"
        );
    }

    // Show effective permissions
    println!("\n    Effective permissions for charlie (editor + viewer):");
    for perm in rbac.get_user_permissions("charlie") {
        println!("      {perm}");
    }

    println!("\n    In production: store in DB → cache in JWT claims or Redis.");
    println!("    Check at gateway (coarse) + service level (fine-grained).\n");
}
