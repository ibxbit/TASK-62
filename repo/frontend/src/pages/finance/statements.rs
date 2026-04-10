/// Finance: Statement import page.
///
/// Lists all imported bank/payment statements.  Finance analysts select a
/// local CSV file, which is read in-browser, base64-encoded, and POSTed to
/// the API.  Validation (extension, empty file, no selection) is performed
/// before submission.  The submit button is disabled while the upload is
/// in-flight to prevent duplicate submissions.
use wasm_bindgen_futures::{spawn_local, JsFuture};
use web_sys::HtmlInputElement;
use yew::prelude::*;

use crate::{
    services::finance_service,
    types::finance::{StatementImport, UploadStatementRequest},
};

#[derive(Clone, PartialEq)]
enum PageState { Loading, Loaded(Vec<StatementImport>), Error(String) }

#[derive(Clone, PartialEq)]
enum UploadState { Idle, Working, Done(String), Failed(String) }

// ── Inline base64 encoder (no external crate needed) ─────────────────────────

fn to_base64(data: &[u8]) -> String {
    const TABLE: &[u8; 64] =
        b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity((data.len() + 2) / 3 * 4);
    for chunk in data.chunks(3) {
        let n = (chunk[0] as u32) << 16
            | (if chunk.len() > 1 { chunk[1] as u32 } else { 0 }) << 8
            | (if chunk.len() > 2 { chunk[2] as u32 } else { 0 });
        out.push(TABLE[((n >> 18) & 63) as usize] as char);
        out.push(TABLE[((n >> 12) & 63) as usize] as char);
        out.push(if chunk.len() > 1 { TABLE[((n >> 6) & 63) as usize] as char } else { '=' });
        out.push(if chunk.len() > 2 { TABLE[(n & 63) as usize] as char } else { '=' });
    }
    out
}

// ── Component ─────────────────────────────────────────────────────────────────

#[function_component(StatementsPage)]
pub fn statements_page() -> Html {
    let page_state    = use_state(|| PageState::Loading);
    let upload_state  = use_state(|| UploadState::Idle);
    let source_input  = use_state(|| "bank_csv".to_string());
    // Selected file stored as web_sys::File for in-browser reading
    let selected_file = use_state(|| Option::<web_sys::File>::None);
    let file_error    = use_state(|| Option::<String>::None);

    let reload = {
        let ps = page_state.clone();
        move || {
            let ps = ps.clone();
            spawn_local(async move {
                match finance_service::list_statements().await {
                    Ok(items) => ps.set(PageState::Loaded(items)),
                    Err(e)    => ps.set(PageState::Error(e)),
                }
            });
        }
    };

    { let r = reload.clone(); use_effect_with((), move |_| { r(); || () }); }

    // ── File selection handler ────────────────────────────────────────────────

    let on_file_change = {
        let selected_file = selected_file.clone();
        let file_error    = file_error.clone();
        Callback::from(move |e: Event| {
            let input: HtmlInputElement = e.target_unchecked_into();
            let files = input.files();
            if let Some(file_list) = files {
                if let Some(file) = file_list.get(0) {
                    let name = file.name();
                    // Validate extension
                    if !name.to_lowercase().ends_with(".csv") {
                        file_error.set(Some("Only CSV files are accepted.".to_string()));
                        selected_file.set(None);
                        return;
                    }
                    // Validate non-empty
                    if file.size() == 0.0 {
                        file_error.set(Some("The selected file is empty.".to_string()));
                        selected_file.set(None);
                        return;
                    }
                    file_error.set(None);
                    selected_file.set(Some(file));
                } else {
                    selected_file.set(None);
                    file_error.set(None);
                }
            }
        })
    };

    // ── Upload / submit handler ───────────────────────────────────────────────

    let on_upload = {
        let us            = upload_state.clone();
        let src           = source_input.clone();
        let reload        = reload.clone();
        let selected_file = selected_file.clone();
        let file_error    = file_error.clone();
        Callback::from(move |e: SubmitEvent| {
            e.prevent_default();

            // Must have a file selected
            let file = match (*selected_file).clone() {
                Some(f) => f,
                None    => {
                    file_error.set(Some("Please select a CSV file before importing.".to_string()));
                    return;
                }
            };

            let us2       = us.clone();
            let reload    = reload.clone();
            let src_val   = (*src).clone();
            let file_name = file.name();

            spawn_local(async move {
                us2.set(UploadState::Working);

                // Read file text content via Blob.text() → JsFuture
                let text_result = JsFuture::from(file.text()).await;
                let text = match text_result {
                    Ok(v)  => v.as_string().unwrap_or_default(),
                    Err(_) => {
                        us2.set(UploadState::Failed("Failed to read file content.".to_string()));
                        return;
                    }
                };

                if text.is_empty() {
                    us2.set(UploadState::Failed("File content is empty after reading.".to_string()));
                    return;
                }

                let content_base64 = to_base64(text.as_bytes());

                let body = UploadStatementRequest {
                    filename:             file_name,
                    source:               src_val,
                    content_base64,
                    expected_fingerprint: None,
                };

                match finance_service::upload_statement(&body).await {
                    Ok(resp) => {
                        us2.set(UploadState::Done(format!(
                            "Imported {} records (valid: {})",
                            resp.record_count, resp.is_valid
                        )));
                        reload();
                    }
                    Err(e) => us2.set(UploadState::Failed(e)),
                }
            });
        })
    };

    // ── Statement list ────────────────────────────────────────────────────────

    let content = match &*page_state {
        PageState::Loading => html! { <div class="loading-state"><div class="spinner"/></div> },
        PageState::Error(e) => html! { <p class="error-state__message">{ e }</p> },
        PageState::Loaded(items) if items.is_empty() => html! {
            <div class="empty-state"><p>{ "No statements imported yet." }</p></div>
        },
        PageState::Loaded(items) => html! {
            <table class="data-table">
                <thead>
                    <tr>
                        <th>{ "Filename" }</th>
                        <th>{ "Source" }</th>
                        <th>{ "Records" }</th>
                        <th>{ "Import Date" }</th>
                        <th>{ "Status" }</th>
                    </tr>
                </thead>
                <tbody>
                    { for items.iter().map(|s| html! {
                        <tr key={s.id.to_string()}>
                            <td>{ &s.filename }</td>
                            <td>{ &s.source }</td>
                            <td>{ s.total_records }</td>
                            <td>{ s.import_date.to_string() }</td>
                            <td>
                                <span class={format!("badge badge--{}", s.status)}>{ &s.status }</span>
                            </td>
                        </tr>
                    }) }
                </tbody>
            </table>
        },
    };

    let is_working = matches!(*upload_state, UploadState::Working);

    let feedback = match &*upload_state {
        UploadState::Working   => html! {
            <div class="action-feedback action-feedback--working">{ "Uploading…" }</div>
        },
        UploadState::Done(msg) => html! {
            <div class="action-feedback action-feedback--success">{ msg }</div>
        },
        UploadState::Failed(e) => html! {
            <div class="action-feedback action-feedback--error">{ e }</div>
        },
        UploadState::Idle      => html! {},
    };

    // Display selected file name or prompt
    let file_label = match &*selected_file {
        Some(f) => f.name(),
        None    => "No file selected".to_string(),
    };

    html! {
        <div class="page">
            <header class="page__header">
                <h1 class="page__title">{ "Statement Imports" }</h1>
            </header>
            <div class="page__body">
                <form onsubmit={on_upload} class="inline-form">
                    <label class="form-field form-field--inline">
                        <span>{ "Source" }</span>
                        <select class="form-field__input"
                                disabled={is_working}
                                onchange={{
                                    let s = source_input.clone();
                                    Callback::from(move |e: Event| {
                                        let el: web_sys::HtmlSelectElement = e.target_unchecked_into();
                                        s.set(el.value());
                                    })
                                }}>
                            <option value="bank_csv">{ "Bank CSV" }</option>
                            <option value="stripe">{ "Stripe" }</option>
                            <option value="sumup">{ "SumUp" }</option>
                        </select>
                    </label>
                    <label class="form-field form-field--inline">
                        <span>{ "CSV File" }</span>
                        <div class="file-input-wrapper">
                            <input
                                type="file"
                                accept=".csv"
                                disabled={is_working}
                                onchange={on_file_change}
                                class="form-field__file-input"
                            />
                            <span class="form-field__file-label">{ &file_label }</span>
                        </div>
                    </label>
                    if let Some(fe) = &*file_error {
                        <p class="form-field__error">{ fe }</p>
                    }
                    <button type="submit"
                            class="btn btn--primary"
                            disabled={is_working || selected_file.is_none()}>
                        { if is_working { "Uploading…" } else { "Import Statement" } }
                    </button>
                </form>
                { feedback }
                { content }
            </div>
        </div>
    }
}
