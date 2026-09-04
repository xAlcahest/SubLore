//! What a module may do to the project database, enforced rather than promised.
//!
//! A module declares a lowercase id and owns every table named `m_<id>_*`. On its own that is a
//! contract in a document. SQLite's authorizer makes it a guard: the host installs one for the
//! duration of each module statement and takes it off afterwards.
//!
//! **Denying `Pragma` is the one that matters.** It is what stops a module writing `user_version`
//! and breaking the second ladder from the inside: `user_version` is this crate's forever, and a
//! module that bumped it would make the next free core refuse the whole project. Everything else
//! here costs a module a feature; that one costs a user their project. See docs/module-abi.md 4.7
//! and 6.1, and docs/module-storage-tasks.md S2.

use rusqlite::hooks::{AuthAction, AuthContext, Authorization};
use rusqlite::Connection;

use crate::error::{ProjectError, ProjectErrorKind};

/// The core-owned table a module's own ladder is counted in. Readable and updatable by a module;
/// which row it may touch is not something an authorizer can see, because a `WHERE` is not among
/// its arguments, so that is guarded by the core writing the row itself.
const MODULE_SCHEMA: &str = "module_schema";

/// SQLite's own schema table, under both names it answers to.
///
/// A schema change is not one action. Measured on 2026-09-04: `CREATE TABLE m_fixture_notes` emits
/// an insert, five updates and a read on this table around the `CreateTable` itself, and `DROP
/// TABLE` emits deletes and rootpage updates. Refusing those refuses the schema change, so they are
/// permitted, and they are safe to permit for one reason only: **`Pragma` is denied**, so
/// `writable_schema` can never be turned on, and without it SQLite refuses a direct write to this
/// table whatever an authorizer says. `a_module_cannot_write_the_schema_table_directly` is what
/// holds that second half up.
const SCHEMA_TABLES: [&str; 2] = ["sqlite_master", "sqlite_schema"];

fn is_schema_table(table: &str) -> bool {
    SCHEMA_TABLES.contains(&table.to_ascii_lowercase().as_str())
}

/// The functions a module's statements may call.
///
/// An allowlist, and that is the point: a denylist over a set that grows inside SQLite has a gap on
/// the day SQLite adds one. These are the ones a termbase or a memory needs to match text.
const FUNCTIONS: [&str; 12] = [
    "abs", "coalesce", "count", "length", "lower", "ltrim", "max", "min", "rtrim", "substr",
    "trim", "upper",
];

/// Whether `id` is one a module may declare.
///
/// Lowercase ASCII letters, digits and underscores, and not empty. An allowlist rather than an
/// escape: the id is pasted into a table-name prefix, and the only safe way to put a value into a
/// name is to refuse every value that is not already safe.
pub fn is_module_id(id: &str) -> bool {
    !id.is_empty()
        && id
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_')
}

/// The prefix every table of module `id` starts with.
///
/// The trailing underscore is load-bearing. Without it a module called `a` would own `m_ab_notes`,
/// which belongs to `ab`, and two modules would quietly share tables.
fn prefix_of(id: &str) -> String {
    format!("m_{id}_")
}

/// Whether a table belongs to the module, by name.
fn owns(prefix: &str, table: &str) -> bool {
    // The comparison is on the name SQLite hands over, which is the name as written in the schema.
    // SQLite table names are case-insensitive, so a module could reach its own table as `M_ID_X`;
    // it is still its own table, and folding here is what keeps that true.
    table.to_ascii_lowercase().starts_with(prefix)
}

/// What one action is, for a module that owns `prefix`.
///
/// Written as a total match over the actions this build knows, so a variant added by a newer
/// rusqlite arrives as `Unknown` and is denied rather than falling through a wildcard into
/// permission.
fn decide(prefix: &str, action: &AuthAction<'_>) -> Authorization {
    let allow = match action {
        // Carry no table name of their own; the actions they lead to are checked separately.
        AuthAction::Select | AuthAction::Transaction { .. } | AuthAction::Savepoint { .. } => true,

        AuthAction::Read {
            table_name,
            column_name: _,
        }
        | AuthAction::Update {
            table_name,
            column_name: _,
        } => {
            owns(prefix, table_name) || *table_name == MODULE_SCHEMA || is_schema_table(table_name)
        }

        AuthAction::Insert { table_name } | AuthAction::Delete { table_name } => {
            owns(prefix, table_name) || is_schema_table(table_name)
        }
        AuthAction::CreateTable { table_name }
        | AuthAction::DropTable { table_name }
        | AuthAction::Analyze { table_name } => owns(prefix, table_name),
        AuthAction::AlterTable {
            database_name: _,
            table_name,
        } => owns(prefix, table_name),
        // Both names, because an index is as much the module's as the table under it and a module
        // that named one after a core table would be reaching into the core's own structures.
        AuthAction::CreateIndex {
            index_name,
            table_name,
        }
        | AuthAction::DropIndex {
            index_name,
            table_name,
        } => owns(prefix, index_name) && owns(prefix, table_name),
        // Emitted by `CREATE INDEX` on the index it just made. Held to the same prefix, so a module
        // cannot ask SQLite to rebuild an index belonging to the core.
        AuthAction::Reindex { index_name } => owns(prefix, index_name),

        AuthAction::Function { function_name } => {
            FUNCTIONS.contains(&function_name.to_ascii_lowercase().as_str())
        }

        // Denied outright, and the first is the one that protects the user's project rather than
        // the module's own tables.
        AuthAction::Pragma { .. }
        | AuthAction::Attach { .. }
        | AuthAction::Detach { .. }
        | AuthAction::CreateVtable { .. }
        | AuthAction::DropVtable { .. } => false,

        // Everything else: views, triggers, temp objects, reindex, recursion, and any action a
        // newer SQLite grows. A module has no use for one and this build cannot reason about it.
        _ => false,
    };
    if allow {
        Authorization::Allow
    } else {
        Authorization::Deny
    }
}

/// Install the guard for module `id` on `conn`.
///
/// Every statement prepared while it is installed is checked. The caller takes it off with
/// [`clear`] the moment the module's call returns, because the core's own statements must not be
/// held to a module's rules.
pub fn guard(conn: &Connection, id: &str) -> Result<(), ProjectError> {
    if !is_module_id(id) {
        return Err(ProjectError::new(
            ProjectErrorKind::MigrationFailed,
            std::path::Path::new(""),
            format!("{id:?} is not a module id: lowercase letters, digits and underscores only"),
        ));
    }
    let prefix = prefix_of(id);
    conn.authorizer(Some(move |context: AuthContext<'_>| {
        decide(&prefix, &context.action)
    }))
    .map_err(|error| {
        ProjectError::new(
            ProjectErrorKind::MigrationFailed,
            std::path::Path::new(""),
            format!("the module guard could not be installed: {error}"),
        )
    })
}

/// Take the guard off, whatever happened while it was on.
pub fn clear(conn: &Connection) -> Result<(), ProjectError> {
    conn.authorizer::<fn(AuthContext<'_>) -> Authorization>(None)
        .map_err(|error| {
            ProjectError::new(
                ProjectErrorKind::MigrationFailed,
                std::path::Path::new(""),
                format!("the module guard could not be removed: {error}"),
            )
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_id_is_lowercase_ascii_and_nothing_else() {
        assert!(is_module_id("fixture"));
        assert!(is_module_id("term_base_2"));
        assert!(!is_module_id(""));
        assert!(!is_module_id("Fixture"), "an uppercase letter is refused");
        assert!(!is_module_id("term-base"), "a hyphen is refused");
        assert!(!is_module_id("a b"), "a space is refused");
        assert!(!is_module_id("a'b"), "a quote is refused");
        assert!(
            !is_module_id("m_a_x; DROP TABLE series"),
            "so is a statement"
        );
    }

    #[test]
    fn a_prefix_ends_at_the_underscore_so_two_modules_cannot_share_a_table() {
        let a = prefix_of("a");
        assert!(owns(&a, "m_a_notes"));
        assert!(
            !owns(&a, "m_ab_notes"),
            "m_ab_notes belongs to the module called ab"
        );
        assert!(!owns(&a, "series"));
        // SQLite folds table names, so a module reaching its own table in another case still owns
        // it: what must not fold is the boundary between one module's prefix and another's.
        assert!(owns(&a, "M_A_NOTES"));
    }
}
