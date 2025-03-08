use rusqlite::{
    types::{FromSql, FromSqlError, FromSqlResult, ToSqlOutput, Value, ValueRef},
    Error, ToSql,
};

use crate::{AnnouncementMethod, Status, UserName};

impl FromSql for UserName {
    fn column_result(value: ValueRef) -> FromSqlResult<UserName> {
        let s = String::column_result(value)?;
        Ok(UserName::new(s))
    }
}
impl ToSql for UserName {
    fn to_sql(&self) -> Result<ToSqlOutput<'_>, rusqlite::Error> {
        self.as_str().to_sql()
    }
}

impl FromSql for Status {
    fn column_result(value: ValueRef) -> FromSqlResult<Status> {
        match FromSql::column_result(value) {
            Ok(i) => match i {
                0 => Ok(Status::Closed),
                1 => Ok(Status::Private),
                2 => Ok(Status::Public),
                _ => Err(FromSqlError::Other("unknown Status".into())),
            },
            Err(e) => Err(e),
        }
    }
}
impl ToSql for Status {
    fn to_sql(&self) -> Result<ToSqlOutput, Error> {
        let i = match self {
            Status::Public => 2,
            Status::Private => 1,
            Status::Closed => 0,
        };
        Ok(ToSqlOutput::Owned(Value::Integer(i)))
    }
}

impl FromSql for AnnouncementMethod {
    fn column_result(value: ValueRef) -> FromSqlResult<AnnouncementMethod> {
        match FromSql::column_result(value) {
            Result::Ok(i) => match i {
                0 => Result::Ok(AnnouncementMethod::New),
                1 => Result::Ok(AnnouncementMethod::Mod),
                2 => Result::Ok(AnnouncementMethod::Del),
                _ => Result::Err(FromSqlError::Other("unknown AnnouncementMethod".into())),
            },
            Result::Err(e) => Result::Err(e),
        }
    }
}
impl ToSql for AnnouncementMethod {
    fn to_sql(&self) -> Result<ToSqlOutput, Error> {
        let i = match self {
            AnnouncementMethod::New => 0,
            AnnouncementMethod::Mod => 1,
            AnnouncementMethod::Del => 2,
        };
        Ok(ToSqlOutput::Owned(Value::Integer(i)))
    }
}
