//! Hand files to Windows' share sheet so they can go to Phone Link.
//!
//! Phone Link has no public send API (and reports transfer progress only in its
//! own window), but it registers as a Windows share target for any file type.
//! So: resolve the dropped paths to storage files on a worker thread, then show
//! the system share window on the UI thread with those files preloaded.

use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;
use windows::core::{implement, Interface, Result, HSTRING};
use windows::ApplicationModel::DataTransfer::{DataRequestedEventArgs, DataTransferManager};
use windows::Foundation::Collections::{IIterable, IIterable_Impl, IIterator, IIterator_Impl};
use windows::Foundation::TypedEventHandler;
use windows::Storage::{IStorageItem, StorageFile};
use windows::Win32::Foundation::HWND;
use windows::Win32::UI::Shell::IDataTransferManagerInterop;

/// Storage files are agile (free-threaded) WinRT objects, so moving them to
/// the UI thread is fine; the wrapper just says so to the compiler.
pub struct Items(Vec<IStorageItem>);
unsafe impl Send for Items {}

impl Items {
    /// A method (not field access) so closures capture the whole wrapper.
    fn list(&self) -> &[IStorageItem] {
        &self.0
    }
}

pub enum Payload {
    Files(Items),
    /// A URL: Phone Link opens it directly instead of shipping a .url file.
    Link(String),
    /// Plain text (notes, snippets).
    Text(String),
}

static PENDING: Mutex<Option<Payload>> = Mutex::new(None);

#[implement(IIterator<IStorageItem>)]
struct ItemIter {
    items: Vec<IStorageItem>,
    pos: AtomicUsize,
}

impl IIterator_Impl<IStorageItem> for ItemIter_Impl {
    fn Current(&self) -> Result<IStorageItem> {
        self.items.get(self.pos.load(Ordering::SeqCst)).cloned().ok_or_else(|| windows::core::Error::from(windows::Win32::Foundation::E_BOUNDS))
    }

    fn HasCurrent(&self) -> Result<bool> {
        Ok(self.pos.load(Ordering::SeqCst) < self.items.len())
    }

    fn MoveNext(&self) -> Result<bool> {
        let next = (self.pos.load(Ordering::SeqCst) + 1).min(self.items.len());
        self.pos.store(next, Ordering::SeqCst);
        Ok(next < self.items.len())
    }

    fn GetMany(&self, items: &mut [Option<IStorageItem>]) -> Result<u32> {
        let start = self.pos.load(Ordering::SeqCst);
        let mut n = 0;
        for (slot, item) in items.iter_mut().zip(self.items.iter().skip(start)) {
            *slot = Some(item.clone());
            n += 1;
        }
        self.pos.store(start + n, Ordering::SeqCst);
        Ok(n as u32)
    }
}

#[implement(IIterable<IStorageItem>)]
struct ItemList(Vec<IStorageItem>);

impl IIterable_Impl<IStorageItem> for ItemList_Impl {
    fn First(&self) -> Result<IIterator<IStorageItem>> {
        Ok(ItemIter { items: self.0.clone(), pos: AtomicUsize::new(0) }.into())
    }
}

/// Resolve `paths` to storage files (worker thread). Returns how many resolved;
/// the files wait in `PENDING` until `show` runs on the UI thread.
pub fn prepare(paths: &[String]) -> usize {
    let items: Vec<IStorageItem> = paths
        .iter()
        .filter(|p| Path::new(p).is_file())
        .filter_map(|p| StorageFile::GetFileFromPathAsync(&HSTRING::from(p.as_str())).ok()?.get().ok())
        .filter_map(|f| f.cast::<IStorageItem>().ok())
        .collect();
    let n = items.len();
    if let Ok(mut slot) = PENDING.lock() {
        *slot = (n > 0).then_some(Payload::Files(Items(items)));
    }
    n
}

/// Queue a URL for `show` — the phone gets a real link, not a file.
pub fn prepare_link(url: String) {
    if let Ok(mut slot) = PENDING.lock() {
        *slot = Some(Payload::Link(url));
    }
}

/// Queue plain text for `show`.
pub fn prepare_text(text: String) {
    if let Ok(mut slot) = PENDING.lock() {
        *slot = Some(Payload::Text(text));
    }
}

thread_local! {
    static TOKEN: std::cell::Cell<Option<i64>> = const { std::cell::Cell::new(None) };
}

/// Show the Windows share window for the prepared files (UI thread only).
pub fn show(hwnd: HWND) -> Result<()> {
    let items = PENDING.lock().ok().and_then(|mut p| p.take()).ok_or_else(|| windows::core::Error::from(windows::Win32::Foundation::E_FAIL))?;
    let interop = windows::core::factory::<DataTransferManager, IDataTransferManagerInterop>()?;
    let dtm: DataTransferManager = unsafe { interop.GetForWindow(hwnd)? };
    // One handler at a time: drop the one from the previous share.
    if let Some(t) = TOKEN.take() {
        let _ = dtm.RemoveDataRequested(windows::Foundation::EventRegistrationToken { Value: t });
    }
    let token = dtm.DataRequested(&TypedEventHandler::new(move |_, args: &Option<DataRequestedEventArgs>| {
        if let Some(args) = args {
            let data = args.Request()?.Data()?;
            let props = data.Properties()?;
            props.SetDescription(&HSTRING::from("Choose Phone Link to send to your phone"))?;
            match &items {
                Payload::Files(items) => {
                    props.SetTitle(&HSTRING::from(if items.list().len() == 1 { "Send 1 file" } else { "Send files" }))?;
                    let list: IIterable<IStorageItem> = ItemList(items.list().to_vec()).into();
                    data.SetStorageItems(&list, true)?;
                }
                Payload::Link(url) => {
                    props.SetTitle(&HSTRING::from("Send link"))?;
                    data.SetWebLink(&windows::Foundation::Uri::CreateUri(&HSTRING::from(url.as_str()))?)?;
                }
                Payload::Text(t) => {
                    props.SetTitle(&HSTRING::from("Send text"))?;
                    data.SetText(&HSTRING::from(t.as_str()))?;
                }
            }
        }
        Ok(())
    }))?;
    TOKEN.set(Some(token.Value));
    unsafe { interop.ShowShareUIForWindow(hwnd) }
}

#[cfg(test)]
mod tests {
    use super::*;
    use windows::ApplicationModel::DataTransfer::DataPackage;
    use windows::Win32::System::WinRT::{RoInitialize, RO_INIT_MULTITHREADED};

    #[test]
    fn dropped_files_travel_through_a_data_package() {
        unsafe {
            let _ = RoInitialize(RO_INIT_MULTITHREADED);
        }
        let dir = std::env::temp_dir().join(format!("teardrop-share-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let files: Vec<String> = ["a.txt", "b.png"]
            .iter()
            .map(|n| {
                let p = dir.join(n);
                std::fs::write(&p, b"x").unwrap();
                p.to_string_lossy().into_owned()
            })
            .collect();
        let mut paths = files.clone();
        paths.push(dir.to_string_lossy().into_owned()); // a folder: skipped
        paths.push(dir.join("missing.bin").to_string_lossy().into_owned()); // gone: skipped
        assert_eq!(prepare(&paths), 2);

        let items = match PENDING.lock().unwrap().take().unwrap() { Payload::Files(i) => i, _ => panic!() };
        let list: IIterable<IStorageItem> = ItemList(items.list().to_vec()).into();
        // Our IIterable enumerates correctly...
        let names: Vec<String> = list.clone().into_iter().map(|i| i.Name().unwrap().to_string()).collect();
        assert_eq!(names, ["a.txt", "b.png"]);
        // ...and Windows accepts it as a data package's file list.
        let pkg = DataPackage::new().unwrap();
        pkg.SetStorageItems(&list, true).unwrap();
        let back = pkg.GetView().unwrap().GetStorageItemsAsync().unwrap().get().unwrap();
        assert_eq!(back.Size().unwrap(), 2);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
