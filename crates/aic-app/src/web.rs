//! What only the web build does: fetch `data/` from its own site, download files and ask the
//! browser for a file to open.

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::mpsc::{self, Receiver};

use aic_data::Bundle;
use eframe::egui;
use wasm_bindgen::{JsCast as _, JsValue};
use wasm_bindgen_futures::JsFuture;

use crate::files::{Download, Upload};

mod table {
    include!(concat!(env!("OUT_DIR"), "/embedded_data.rs"));
}

/// Where the site serves the repository's `data/` folder, next to the page.
const DATA_URL: &str = "data";

/// The game data on its way from the site: every file `build.rs` found in `data/`, fetched at
/// once.
pub struct DataDownload {
    progress: Rc<RefCell<Progress>>,
    total: usize,
}

#[derive(Default)]
struct Progress {
    files: Vec<(String, Vec<u8>)>,
    /// The first file that failed, and why.
    failed: Option<(String, String)>,
}

/// How far a [`DataDownload`] got.
pub enum Fetched {
    Loading {
        done: usize,
        total: usize,
    },
    Done(Bundle),
    /// A file, and why it didn't come.
    Failed {
        file: String,
        error: String,
    },
}

impl DataDownload {
    pub fn start(ctx: &egui::Context) -> Self {
        let progress = Rc::new(RefCell::new(Progress::default()));
        for &path in table::FILES {
            let progress = Rc::clone(&progress);
            let ctx = ctx.clone();
            wasm_bindgen_futures::spawn_local(async move {
                let fetched = fetch(&format!("{DATA_URL}/{path}")).await;
                let mut progress = progress.borrow_mut();
                match fetched {
                    Ok(bytes) => progress.files.push((path.to_owned(), bytes)),
                    Err(error) => {
                        progress.failed.get_or_insert((path.to_owned(), error));
                    }
                }
                ctx.request_repaint();
            });
        }
        Self {
            progress,
            total: table::FILES.len(),
        }
    }

    /// Where the download is. Once it is `Done`, the files have moved into the bundle.
    pub fn poll(&mut self) -> Fetched {
        let mut progress = self.progress.borrow_mut();
        if let Some((file, error)) = progress.failed.clone() {
            return Fetched::Failed { file, error };
        }
        let done = progress.files.len();
        if done < self.total {
            return Fetched::Loading {
                done,
                total: self.total,
            };
        }
        Fetched::Done(Bundle::from_owned(std::mem::take(&mut progress.files)))
    }
}

/// Fetches `url`, relative to the page.
async fn fetch(url: &str) -> Result<Vec<u8>, String> {
    let window = web_sys::window().ok_or("no window")?;
    let response: web_sys::Response = JsFuture::from(window.fetch_with_str(url))
        .await
        .and_then(JsValue::dyn_into)
        .map_err(describe)?;
    if !response.ok() {
        return Err(format!("HTTP {}", response.status()));
    }
    let buffer = JsFuture::from(response.array_buffer().map_err(describe)?)
        .await
        .map_err(describe)?;
    Ok(js_sys::Uint8Array::new(&buffer).to_vec())
}

/// Has the browser save `file` to the player's downloads.
pub fn download(file: &Download) -> Result<(), String> {
    let document = web_sys::window()
        .and_then(|w| w.document())
        .ok_or("no document")?;
    let parts = js_sys::Array::of1(&js_sys::Uint8Array::from(file.bytes.as_slice()));
    let options = web_sys::BlobPropertyBag::new();
    options.set_type("application/octet-stream");
    let blob = web_sys::Blob::new_with_u8_array_sequence_and_options(&parts, &options)
        .map_err(describe)?;
    let url = web_sys::Url::create_object_url_with_blob(&blob).map_err(describe)?;
    let anchor: web_sys::HtmlAnchorElement = document
        .create_element("a")
        .map_err(describe)?
        .dyn_into()
        .map_err(|element| describe(element.into()))?;
    anchor.set_href(&url);
    anchor.set_download(&file.name);
    anchor.click();
    web_sys::Url::revoke_object_url(&url).map_err(describe)
}

/// Asks the browser for a save file to open. The receiver gets it once it is read, or hangs up if
/// the player cancels.
pub fn pick_file(extension: &str) -> Receiver<Upload> {
    let (sender, receiver) = mpsc::channel();
    let dialog = rfd::AsyncFileDialog::new().add_filter("AIC Modeler", &[extension]);
    wasm_bindgen_futures::spawn_local(async move {
        if let Some(file) = dialog.pick_file().await {
            let upload = Upload {
                name: file.file_name(),
                bytes: file.read().await,
            };
            // The editor may have moved on; nothing to do then.
            let _ = sender.send(upload);
        }
    });
    receiver
}

fn describe(value: JsValue) -> String {
    value
        .as_string()
        .or_else(|| {
            value
                .dyn_ref::<js_sys::Error>()
                .map(|e| String::from(e.message()))
        })
        .unwrap_or_else(|| format!("{value:?}"))
}
