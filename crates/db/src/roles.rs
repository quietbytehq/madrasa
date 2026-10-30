use crate::audit::{self, snapshot, Action};
use crate::models::{PermissionGrant, Role, UserRole};
use crate::{map_unique, Actor, DbError, DbResult};
use madrasa_core::edition::Edition;
use madrasa_core::hlc::Hlc;
use madrasa_core::ids::new_id;
use madrasa_core::permissions::{Permission, RoleCode};
use sqlx::PgConnection;
use uuid::Uuid;

pub async fn create(
    conn: &mut PgConnection,
    hlc: &Hlc,
    actor: Actor,
    school_id: Uuid,
    code: &str,
    name: &str,
    is_system: bool,
) -> DbResult<Role> {
    let id = new_id();
    let role = sqlx::query_as::<_, Role>(
        "INSERT INTO roles (id, school_id, code, name, is_system, updated_by, hlc)
         VALUES ($1, $2, $3, $4, $5, $6, $7) RETURNING *",
    )
    .bind(id)
    .bind(school_id)
    .bind(code)
    .bind(name)
    .bind(is_system)
    .bind(actor.user_id)
    .bind(hlc.now().to_string())
    .fetch_one(&mut *conn)
    .await
    .map_err(|e| map_unique(e, "a role with this code"))?;
    audit::record(
        conn,
        hlc,
        actor,
        Action::Create,
        "role",
        Some(id),
        None,
        Some(snapshot(&role)),
    )
    .await?;
    Ok(role)
}

pub async fn grant(
    conn: &mut PgConnection,
    hlc: &Hlc,
    actor: Actor,
    role: &Role,
    permission: Permission,
) -> DbResult<PermissionGrant> {
    let id = new_id();
    let grant = sqlx::query_as::<_, PermissionGrant>(
        "INSERT INTO permissions (id, school_id, role_id, code, updated_by, hlc)
         VALUES ($1, $2, $3, $4, $5, $6)
         ON CONFLICT (role_id, code) DO UPDATE SET deleted_at = NULL, updated_at = now(), hlc = EXCLUDED.hlc
         RETURNING id, school_id, role_id, code, hlc",
    )
    .bind(id)
    .bind(role.school_id)
    .bind(role.id)
    .bind(permission.code())
    .bind(actor.user_id)
    .bind(hlc.now().to_string())
    .fetch_one(&mut *conn)
    .await?;
    audit::record(
        conn,
        hlc,
        actor,
        Action::Create,
        "permission",
        Some(grant.id),
        None,
        Some(snapshot(&grant)),
    )
    .await?;
    Ok(grant)
}

/// Create the built-in roles with their default grants for a new school.
pub async fn create_defaults(
    conn: &mut PgConnection,
    hlc: &Hlc,
    actor: Actor,
    school_id: Uuid,
    edition: Edition,
) -> DbResult<Vec<Role>> {
    let mut roles = Vec::new();
    for code in RoleCode::defaults_for(edition) {
        let role = create(
            &mut *conn,
            hlc,
            actor,
            school_id,
            code.code(),
            code.display_name(),
            true,
        )
        .await?;
        for permission in code.default_permissions() {
            grant(&mut *conn, hlc, actor, &role, permission).await?;
        }
        roles.push(role);
    }
    Ok(roles)
}

pub async fn list(conn: &mut PgConnection, school_id: Uuid) -> DbResult<Vec<Role>> {
    Ok(sqlx::query_as::<_, Role>(
        "SELECT * FROM roles WHERE school_id = $1 AND deleted_at IS NULL ORDER BY code",
    )
    .bind(school_id)
    .fetch_all(conn)
    .await?)
}

pub async fn find_by_code(conn: &mut PgConnection, school_id: Uuid, code: &str) -> DbResult<Role> {
    sqlx::query_as::<_, Role>(
        "SELECT * FROM roles WHERE school_id = $1 AND code = $2 AND deleted_at IS NULL",
    )
    .bind(school_id)
    .bind(code)
    .fetch_optional(conn)
    .await?
    .ok_or_else(|| DbError::NotFound(format!("role {code}")))
}

pub async fn assign(
    conn: &mut PgConnection,
    hlc: &Hlc,
    actor: Actor,
    user_id: Uuid,
    role: &Role,
) -> DbResult<UserRole> {
    let id = new_id();
    let ur = sqlx::query_as::<_, UserRole>(
        "INSERT INTO user_roles (id, school_id, user_id, role_id, updated_by, hlc)
         VALUES ($1, $2, $3, $4, $5, $6)
         ON CONFLICT (user_id, role_id) DO UPDATE SET deleted_at = NULL, updated_at = now(), hlc = EXCLUDED.hlc
         RETURNING id, school_id, user_id, role_id, hlc",
    )
    .bind(id)
    .bind(role.school_id)
    .bind(user_id)
    .bind(role.id)
    .bind(actor.user_id)
    .bind(hlc.now().to_string())
    .fetch_one(&mut *conn)
    .await?;
    audit::record(
        conn,
        hlc,
        actor,
        Action::Create,
        "user_role",
        Some(ur.id),
        None,
        Some(snapshot(&ur)),
    )
    .await?;
    Ok(ur)
}

/// Role codes of a user.
pub async fn codes_for_user(conn: &mut PgConnection, user_id: Uuid) -> DbResult<Vec<String>> {
    Ok(sqlx::query_scalar(
        "SELECT r.code FROM user_roles ur JOIN roles r ON r.id = ur.role_id
         WHERE ur.user_id = $1 AND ur.deleted_at IS NULL AND r.deleted_at IS NULL ORDER BY r.code",
    )
    .bind(user_id)
    .fetch_all(conn)
    .await?)
}

/// Effective permission codes of a user (union over roles).
pub async fn permission_codes_for_user(
    conn: &mut PgConnection,
    user_id: Uuid,
) -> DbResult<Vec<String>> {
    Ok(sqlx::query_scalar(
        "SELECT DISTINCT p.code FROM user_roles ur
         JOIN roles r ON r.id = ur.role_id AND r.deleted_at IS NULL
         JOIN permissions p ON p.role_id = r.id AND p.deleted_at IS NULL
         WHERE ur.user_id = $1 AND ur.deleted_at IS NULL ORDER BY p.code",
    )
    .bind(user_id)
    .fetch_all(conn)
    .await?)
}
