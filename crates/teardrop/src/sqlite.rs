//! Read-only SQLite through the copy Windows already ships (`winsqlite3.dll`,
//! Windows 10 1809+), loaded on demand. Bundling our own SQLite would add well
//! over a megabyte to the exe for one small query.

use std::ffi::{c_char, c_int, c_void, CStr, CString};
use std::path::Path;

type Db = *mut c_void;
type Stmt = *mut c_void;

struct Api {
    open_v2: unsafe extern "C" fn(*const c_char, *mut Db, c_int, *const c_char) -> c_int,
    close: unsafe extern "C" fn(Db) -> c_int,
    busy_timeout: unsafe extern "C" fn(Db, c_int) -> c_int,
    prepare_v2: unsafe extern "C" fn(Db, *const c_char, c_int, *mut Stmt, *mut *const c_char) -> c_int,
    bind_int64: unsafe extern "C" fn(Stmt, c_int, i64) -> c_int,
    step: unsafe extern "C" fn(Stmt) -> c_int,
    column_count: unsafe extern "C" fn(Stmt) -> c_int,
    column_text: unsafe extern "C" fn(Stmt, c_int) -> *const u8,
    finalize: unsafe extern "C" fn(Stmt) -> c_int,
}

const SQLITE_ROW: c_int = 100;
const SQLITE_OPEN_READONLY: c_int = 0x1;
const SQLITE_OPEN_NOMUTEX: c_int = 0x8000;

/// Look up an export and view it as the function-pointer type `F`.
///
/// # Safety
/// `F` must be an `extern` function pointer with the signature the DLL exports.
#[cfg(windows)]
unsafe fn load<F: Copy>(lib: windows::Win32::Foundation::HMODULE, name: windows::core::PCSTR) -> Option<F> {
    let p = windows::Win32::System::LibraryLoader::GetProcAddress(lib, name)?;
    debug_assert_eq!(std::mem::size_of::<F>(), std::mem::size_of_val(&p));
    Some(std::mem::transmute_copy::<_, F>(&p))
}

#[cfg(windows)]
fn api() -> Option<&'static Api> {
    use std::sync::OnceLock;
    use windows::core::{s, w};
    use windows::Win32::System::LibraryLoader::LoadLibraryW;
    static API: OnceLock<Option<Api>> = OnceLock::new();
    API.get_or_init(|| unsafe {
        let lib = LoadLibraryW(w!("winsqlite3.dll")).ok()?;
        Some(Api {
            open_v2: load(lib, s!("sqlite3_open_v2"))?,
            close: load(lib, s!("sqlite3_close"))?,
            busy_timeout: load(lib, s!("sqlite3_busy_timeout"))?,
            prepare_v2: load(lib, s!("sqlite3_prepare_v2"))?,
            bind_int64: load(lib, s!("sqlite3_bind_int64"))?,
            step: load(lib, s!("sqlite3_step"))?,
            column_count: load(lib, s!("sqlite3_column_count"))?,
            column_text: load(lib, s!("sqlite3_column_text"))?,
            finalize: load(lib, s!("sqlite3_finalize"))?,
        })
    })
    .as_ref()
}

#[cfg(not(windows))]
fn api() -> Option<&'static Api> {
    None
}

#[derive(Debug, PartialEq)]
pub enum Error {
    /// `winsqlite3.dll` isn't there (older Windows).
    Unavailable,
    Open,
    Query,
}

/// A read-only connection.
pub struct Conn {
    api: &'static Api,
    db: Db,
}

impl Conn {
    pub fn open_readonly(path: &Path) -> Result<Conn, Error> {
        let api = api().ok_or(Error::Unavailable)?;
        let c = CString::new(path.to_string_lossy().as_bytes()).map_err(|_| Error::Open)?;
        let mut db: Db = std::ptr::null_mut();
        let rc = unsafe { (api.open_v2)(c.as_ptr(), &mut db, SQLITE_OPEN_READONLY | SQLITE_OPEN_NOMUTEX, std::ptr::null()) };
        if rc != 0 || db.is_null() {
            if !db.is_null() {
                unsafe { (api.close)(db) };
            }
            return Err(Error::Open);
        }
        unsafe { (api.busy_timeout)(db, 1500) };
        Ok(Conn { api, db })
    }

    /// Run `sql` with one integer parameter (`?1`). Every column comes back as
    /// text (SQLite converts numbers), so callers parse what they need.
    pub fn query_i64(&self, sql: &str, param: i64) -> Result<Vec<Vec<String>>, Error> {
        let c = CString::new(sql).map_err(|_| Error::Query)?;
        let mut stmt: Stmt = std::ptr::null_mut();
        unsafe {
            if (self.api.prepare_v2)(self.db, c.as_ptr(), -1, &mut stmt, std::ptr::null_mut()) != 0 || stmt.is_null() {
                return Err(Error::Query);
            }
            (self.api.bind_int64)(stmt, 1, param);
            let cols = (self.api.column_count)(stmt);
            let mut rows = Vec::new();
            while (self.api.step)(stmt) == SQLITE_ROW {
                rows.push(
                    (0..cols)
                        .map(|i| {
                            let t = (self.api.column_text)(stmt, i);
                            if t.is_null() {
                                String::new()
                            } else {
                                CStr::from_ptr(t as *const c_char).to_string_lossy().into_owned()
                            }
                        })
                        .collect(),
                );
            }
            (self.api.finalize)(stmt);
            Ok(rows)
        }
    }
}

impl Drop for Conn {
    fn drop(&mut self) {
        unsafe { (self.api.close)(self.db) };
    }
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;

    #[test]
    fn winsqlite_is_available_and_reads_json() {
        // A throwaway database file with a JSON column, read back through the API.
        let dir = std::env::temp_dir().join(format!("teardrop-sqlite-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("t.db");
        // Build the file with the same system library (read-write for the test only).
        let api = api().expect("winsqlite3.dll");
        unsafe {
            let c = CString::new(path.to_string_lossy().as_bytes()).unwrap();
            let mut db: Db = std::ptr::null_mut();
            assert_eq!((api.open_v2)(c.as_ptr(), &mut db, 0x2 | 0x4, std::ptr::null()), 0);
            let exec: unsafe extern "C" fn(Db, *const c_char, *const c_void, *const c_void, *mut *mut c_char) -> c_int = {
                use windows::core::{s, w};
                use windows::Win32::System::LibraryLoader::LoadLibraryW;
                let lib = LoadLibraryW(w!("winsqlite3.dll")).unwrap();
                load(lib, s!("sqlite3_exec")).unwrap()
            };
            let sql = CString::new("CREATE TABLE m(t INTEGER, data TEXT); INSERT INTO m VALUES (10, '{\"cost\":0.5,\"tokens\":{\"total\":42}}'), (99, '{\"cost\":1.5,\"tokens\":{\"total\":8}}');").unwrap();
            assert_eq!(exec(db, sql.as_ptr(), std::ptr::null(), std::ptr::null(), std::ptr::null_mut()), 0);
            (api.close)(db);
        }
        let conn = Conn::open_readonly(&path).unwrap();
        let rows = conn.query_i64("SELECT json_extract(data,'$.tokens.total'), json_extract(data,'$.cost') FROM m WHERE t >= ?1", 50).unwrap();
        assert_eq!(rows, vec![vec!["8".to_string(), "1.5".to_string()]]);
        assert_eq!(Conn::open_readonly(&dir.join("missing.db")).err(), Some(Error::Open));
        drop(conn);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
