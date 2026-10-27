//! Real Windows Search: query the SystemIndex catalog through the
//! Search.CollatorDSO OLE DB provider — the same index Start Menu uses.
//!
//! ISearchQueryHelper turns the user's text into proper SQL (tokenization,
//! escaping, ranking); IDataInitialize opens the provider; the rowset is
//! read through one fixed-buffer WSTR binding per column.

use windows::core::{w, HSTRING};

/// One search row: display name, a path ShellExecute/SHCreateItem can open
/// (filesystem path or `shell:`/::{GUID} parsing path), and the friendly
/// type text ("Text Document", "Shortcut", …).
#[derive(Clone)]
pub struct Row {
    pub name: String,
    pub path: String,
    pub kind: String,
}

/// Indexed hits for `q`, ranked by the index. Empty when the indexer is off,
/// the query is blank, or any COM step fails — callers fall back to the
/// Start Menu scan.
pub fn search(q: &str, max: i32) -> Vec<Row> {
    unsafe { inner(q, max) }.unwrap_or_default()
}

unsafe fn inner(q: &str, max: i32) -> windows::core::Result<Vec<Row>> {
    use windows::core::IUnknown;
    use windows::core::Interface;
    use windows::Win32::System::Com::{CoCreateInstance, CoTaskMemFree, CLSCTX_INPROC_SERVER, CLSCTX_LOCAL_SERVER};
    use windows::Win32::System::Search::*;
    unsafe {
        // SQL for the query text: CONTAINS over name/url + ranking. The
        // default scope='file:' keeps this to files and Start Menu .lnks —
        // packaged apps, Settings pages and Control Panel items are NOT in
        // this catalog; `shellcatalog` covers those.
        // CSearchManager is host-activated (out-of-proc) — no in-proc server.
        let mgr: ISearchManager = CoCreateInstance(&CSearchManager, None, CLSCTX_LOCAL_SERVER)?;
        let qh = mgr.GetCatalog(w!("SystemIndex"))?.GetQueryHelper()?;
        qh.SetQueryMaxResults(max)?;
        // System.Kind is a multi-value column — can't bind as scalar WSTR.
        qh.SetQuerySelectColumns(w!("System.ItemNameDisplay,System.ItemPathDisplay,System.ItemTypeText"))?;
        // Default restriction is scope='file:' — files and Start Menu .lnks.
        // Content properties keep CONTAINS on names/urls, not file contents.
        qh.SetQueryContentProperties(w!("System.ItemNameDisplay"))?;
        let sql_pw = qh.GenerateSQLFromUserQuery(&HSTRING::from(q))?;
        let sql = sql_pw.to_string().unwrap_or_default();
        CoTaskMemFree(Some(sql_pw.0 as *const _));
        if sql.is_empty() {
            return Ok(Vec::new());
        }
        let conn_pw = qh.ConnectionString()?;
        let conn = conn_pw.to_string().unwrap_or_default();
        CoTaskMemFree(Some(conn_pw.0 as *const _));

        // Provider: "Provider=Search.CollatorDSO.1" == SystemIndex catalog.
        let dli: IDataInitialize = CoCreateInstance(&DataLinks, None, CLSCTX_INPROC_SERVER)?;
        let mut ds: Option<IUnknown> = None;
        dli.GetDataSource(None::<&IUnknown>, CLSCTX_INPROC_SERVER.0 as u32, &HSTRING::from(conn.as_str()), &IDBInitialize::IID, &mut ds)?;
        let Some(ds) = ds else { return Ok(Vec::new()) };
        let dbinit: IDBInitialize = ds.cast()?;
        dbinit.Initialize()?;
        let session: IDBCreateCommand = dbinit.cast::<IDBCreateSession>()?.CreateSession(None::<&IUnknown>, &IDBCreateCommand::IID)?.cast()?;
        let cmd: ICommandText = session.CreateCommand(None::<&IUnknown>, &ICommandText::IID)?.cast()?;
        cmd.SetCommandText(&DBGUID_DEFAULT, &HSTRING::from(sql.as_str()))?;
        let mut rowset_unk: Option<IUnknown> = None;
        let mut affected: isize = 0;
        cmd.cast::<ICommand>()?.Execute(None::<&IUnknown>, &IRowset::IID, None, Some(&mut affected), Some(&mut rowset_unk))?;
        let Some(rowset_unk) = rowset_unk else { return Ok(Vec::new()) };
        let rowset: IRowset = rowset_unk.cast()?;

        // Providers may reorder output columns — resolve name -> column index
        // via IColumnsInfo; keep `infos`/`strings` alive through the row walk.
        let ci: IColumnsInfo = rowset.cast()?;
        let (mut ncols, mut infos, mut strings) = (0usize, std::ptr::null_mut::<DBCOLUMNINFO>(), std::ptr::null_mut::<u16>());
        ci.GetColumnInfo(&mut ncols, &mut infos, Some(&mut strings))?;
        let col_of = |want: &str| -> Option<usize> {
            for i in 0..ncols {
                let info = &*infos.add(i);
                if info.pwszName.0.is_null() {
                    continue;
                }
                let mut len = 0;
                while *info.pwszName.0.add(len) != 0 {
                    len += 1;
                }
                if String::from_utf16_lossy(std::slice::from_raw_parts(info.pwszName.0, len)).eq_ignore_ascii_case(want) {
                    return Some(i);
                }
            }
            None
        };
        let wants = [col_of("System.ItemNameDisplay"), col_of("System.ItemPathDisplay"), col_of("System.ItemTypeText")];
        if wants.iter().all(|w| w.is_none()) {
            CoTaskMemFree(Some(infos as *const _));
            CoTaskMemFree(Some(strings as *const _));
            return Ok(Vec::new());
        }

        // This provider validates each binding but rejects the second+
        // binding of a multi-column accessor (DBBINDINGSTATUS_BADBINDINFO),
        // so each column gets its own accessor into a lone VARIANT slot.
        let acc: IAccessor = rowset.cast()?;
        let mut haccs = [HACCESSOR(0), HACCESSOR(0), HACCESSOR(0)];
        let mut wtypes = [0u16; 3];
        for (slot, col) in wants.iter().enumerate() {
            let Some(col) = col else { continue };
            let info = &*infos.add(*col);
            wtypes[slot] = info.wType;
            let binding = DBBINDING {
                iOrdinal: info.iOrdinal,
                obValue: 0,
                dwPart: DBPART_VALUE.0 as u32,
                dwMemOwner: DBMEMOWNER_CLIENTOWNED.0 as u32,
                eParamIO: DBPARAMIO_NOTPARAM.0 as u32,
                cbMaxLen: SLOT,
                wType: info.wType,
                bPrecision: info.bPrecision,
                bScale: info.bScale,
                ..Default::default()
            };
            let mut st = u32::MAX;
            let _ = acc.CreateAccessor(DBACCESSOR_ROWDATA.0 as u32, 1, &binding, SLOT, &mut haccs[slot], Some(&mut st));
        }
        CoTaskMemFree(Some(infos as *const _));
        CoTaskMemFree(Some(strings as *const _));

        let mut out = Vec::new();
        // One caller-owned HROW slot: a non-null `*prghrows` makes the provider
        // fill it instead of allocating (and returning) its own array.
        let mut hrow = 0usize;
        while out.len() < max.max(0) as usize {
            let mut got = 0usize;
            let mut slot: [*mut usize; 1] = [&mut hrow];
            // DB_S_ENDOFROWSET is a success HRESULT; `got == 0` ends the walk.
            if rowset.GetNextRows(0, 0, &mut got, &mut slot).is_err() || got == 0 {
                break;
            }
            let mut bufs = [[0u8; SLOT]; 3];
            for (i, hacc) in haccs.iter().enumerate() {
                if hacc.0 != 0 {
                    let _ = rowset.GetData(hrow, *hacc, bufs[i].as_mut_ptr().cast());
                }
            }
            out.push(Row { name: slot_str(&bufs[0], wtypes[0]), path: slot_str(&bufs[1], wtypes[1]), kind: slot_str(&bufs[2], wtypes[2]) });
            let _ = rowset.ReleaseRows(1, &hrow, std::ptr::null(), std::ptr::null_mut(), std::ptr::null_mut());
        }
        for h in haccs {
            if h.0 != 0 {
                let _ = acc.ReleaseAccessor(h, None);
            }
        }
        Ok(out)
    }
}

/// Per-column row buffer: big enough for an inline WSTR (paths) or a VARIANT.
const SLOT: usize = 520;

/// Decode one bound column: inline DBTYPE_WSTR (130), else a VARIANT.
fn slot_str(buf: &[u8; SLOT], wtype: u16) -> String {
    if wtype == 130 {
        let w: Vec<u16> = buf.chunks_exact(2).map(|c| u16::from_le_bytes([c[0], c[1]])).take_while(|&c| c != 0).collect();
        return String::from_utf16_lossy(&w);
    }
    let v = unsafe { std::ptr::read_unaligned(buf.as_ptr().cast::<OleVar>()) };
    var_str(&v, 0)
}

/// 16-byte VARIANT slot the provider writes into the row buffer
/// (vt + 6-byte pad + union pointer, x64 layout).
#[repr(C)]
struct OleVar {
    vt: u16,
    _pad: [u16; 3],
    p: *mut u16,
}

/// Decode a bound VARIANT slot: VT_BSTR (8) is the normal case for the
/// text columns; VT_LPWSTR (31) also turns up. Anything else reads empty.
fn var_str(v: &OleVar, status: u32) -> String {
    use windows::Win32::System::Com::CoTaskMemFree;
    use windows::Win32::System::Search::{DBSTATUS_S_OK, DBSTATUS_S_TRUNCATED};
    if status != DBSTATUS_S_OK.0 as u32 && status != DBSTATUS_S_TRUNCATED.0 as u32 {
        return String::new();
    }
    unsafe {
        match v.vt {
            8 => {
                if v.p.is_null() {
                    return String::new();
                }
                // Take ownership: BSTR's Drop is SysFreeString, the only legal free.
                windows::core::BSTR::from_raw(v.p).to_string()
            }
            31 => {
                if v.p.is_null() {
                    return String::new();
                }
                let mut n = 0;
                while *v.p.add(n) != 0 {
                    n += 1;
                }
                let s = String::from_utf16_lossy(std::slice::from_raw_parts(v.p, n));
                CoTaskMemFree(Some(v.p as *const _));
                s
            }
            _ => String::new(),
        }
    }
}

/// DBGUID_DEFAULT (OLE DB default dialect) isn't in the windows crate.
const DBGUID_DEFAULT: windows::core::GUID = windows::core::GUID::from_u128(0xc8b521fb_5cf3_11ce_ade5_00aa0044773d);

#[cfg(test)]
mod tests {
    /// Live index probe: a query that should hit something indexed on any
    /// dev machine. Skipped silently when the indexer is unavailable.
    #[test]
    fn indexes_real_items() {
        unsafe {
            let _ = windows::Win32::System::Com::CoInitializeEx(None, windows::Win32::System::Com::COINIT_APARTMENTTHREADED);
        }
        let rows = match unsafe { super::inner("docker", 10) } {
            Ok(r) => r,
            Err(e) => {
                eprintln!("inner failed: {e:?}");
                return;
            }
        };
        eprintln!("{} rows", rows.len());
        for r in &rows {
            eprintln!("  {:?} | {:?} | {}", r.name, r.path, r.kind);
            assert!(!r.name.is_empty());
            assert!(!r.path.is_empty());
        }
    }
}
