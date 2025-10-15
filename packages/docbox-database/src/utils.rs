use sqlx::error::DatabaseError;

use crate::DbErr;

/// Database error extension helper to determine common types of database
/// errors that can be safely caught
pub trait DatabaseErrorExt {
    fn is_error_code(&self, error_code: &str) -> bool;

    fn is_database_does_not_exist(&self) -> bool;

    fn is_database_exists(&self) -> bool;

    fn is_table_does_not_exist(&self) -> bool;

    fn is_duplicate_record(&self) -> bool;
}

impl DatabaseErrorExt for &dyn DatabaseError {
    fn is_error_code(&self, error_code: &str) -> bool {
        self.code()
            .is_some_and(|code| code.to_string().eq(error_code))
    }

    fn is_database_does_not_exist(&self) -> bool {
        self.is_error_code("3D000" /* Database does not exist */)
    }

    fn is_database_exists(&self) -> bool {
        self.is_error_code("42P04" /* Duplicate database */)
    }

    fn is_table_does_not_exist(&self) -> bool {
        self.is_error_code("42P01" /* Table does not exist */)
    }

    fn is_duplicate_record(&self) -> bool {
        self.is_unique_violation()
    }
}

impl DatabaseErrorExt for DbErr {
    fn is_error_code(&self, error_code: &str) -> bool {
        self.as_database_error()
            .is_some_and(|error| error.is_error_code(error_code))
    }

    fn is_database_does_not_exist(&self) -> bool {
        self.as_database_error()
            .is_some_and(|error| error.is_database_does_not_exist())
    }

    fn is_database_exists(&self) -> bool {
        self.as_database_error()
            .is_some_and(|error| error.is_database_exists())
    }

    fn is_table_does_not_exist(&self) -> bool {
        self.as_database_error()
            .is_some_and(|error| error.is_table_does_not_exist())
    }

    fn is_duplicate_record(&self) -> bool {
        self.as_database_error()
            .is_some_and(|error| error.is_duplicate_record())
    }
}
