//! What only the web build does: take the icon sheets the page fetched, take the loading page
//! away, download files and ask the browser for a file to open.

use std::sync::mpsc::{self, Receiver};

use wasm_bindgen::{JsCast as _, JsValue};

use crate::files::{Download, Upload};
use crate::icons::SheetPixels;

/// The icon sheets, as the loading page in `index.html` fetched and decoded them
/// (`window.aicLoading.sheets`, a promise of one `{ width, height, rgba }` a sheet, `rgba` being
/// the browser's decoding of the AVIF, unmultiplied). An error when a sheet didn't come or didn't
/// decode: the page shows which.
pub async fn icon_sheets() -> Result<Vec<SheetPixels>, String> {
    let window = web_sys::window().ok_or("no window")?;
    let loading = get(&window, "aicLoading")?;
    let promise = get(&loading, "sheets")?
        .dyn_into::<js_sys::Promise>()
        .map_err(|_| "the page fetched no icon sheets".to_owned())?;
    let sheets = wasm_bindgen_futures::JsFuture::from(promise)
        .await
        .map_err(describe)?;
    js_sys::Array::from(&sheets)
        .iter()
        .map(|sheet| {
            let side = |name| {
                get(&sheet, name)?
                    .as_f64()
                    .map(|side| side as usize)
                    .ok_or_else(|| format!("an icon sheet's {name} is not a number"))
            };
            let size = [side("width")?, side("height")?];
            let rgba = get(&sheet, "rgba")?
                .dyn_into::<js_sys::Uint8Array>()
                .map_err(|_| "an icon sheet's pixels are not a Uint8Array".to_owned())?
                .to_vec();
            Ok(SheetPixels { size, rgba })
        })
        .collect()
}

fn get(object: &JsValue, key: &str) -> Result<JsValue, String> {
    js_sys::Reflect::get(object, &JsValue::from_str(key)).map_err(describe)
}

/// Takes the loading page away, once the editor has drawn its first frame, and marks the moment
/// in the page's performance timeline (`aic-editor-shown`), for measuring a load.
pub fn remove_loading_page() {
    let Some(window) = web_sys::window() else {
        return;
    };
    if let Some(page) = window
        .document()
        .and_then(|document| document.get_element_by_id(LOADING_PAGE))
    {
        page.remove();
    }
    let mark = js_sys::Reflect::get(&window, &JsValue::from_str("performance")).and_then(|p| {
        let mark = js_sys::Reflect::get(&p, &JsValue::from_str("mark"))?;
        mark.dyn_into::<js_sys::Function>()?
            .call1(&p, &JsValue::from_str("aic-editor-shown"))
    });
    // Only a measurement: a browser without it loses nothing.
    let _ = mark;
}

/// The loading page's element in `index.html`.
const LOADING_PAGE: &str = "loading";

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
