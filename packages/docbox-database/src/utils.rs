use sqlx::error::DatabaseError;

use crate::DbErr;

/// Database error extension helper to determine common types of database
/// errors that can be safely caught
pub trait DatabaseErrorExt {
    fn is_database_does_not_exist(&self) -> bool;

    fn is_table_does_not_exist(&self) -> bool;
}

impl DatabaseErrorExt for &dyn DatabaseError {
    fn is_database_does_not_exist(&self) -> bool {
        self.code().is_some_and(|code| {
            code.to_string().eq("3D000" /* Database does not exist */)
        })
    }

    fn is_table_does_not_exist(&self) -> bool {
        self.code().is_some_and(|code| {
            code.to_string().eq("42P01" /* Table does not exist */)
        })
    }
}

impl DatabaseErrorExt for DbErr {
    fn is_database_does_not_exist(&self) -> bool {
        self.as_database_error()
            .is_some_and(|error| error.is_database_does_not_exist())
    }

    fn is_table_does_not_exist(&self) -> bool {
        self.as_database_error()
            .is_some_and(|error| error.is_table_does_not_exist())
    }
}
