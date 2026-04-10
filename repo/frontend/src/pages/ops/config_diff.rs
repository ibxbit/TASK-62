/// Config version diff view.
///
/// Allows the user to select two version IDs and see a structural diff.
/// Renders added/removed/changed keys side by side.
use uuid::Uuid;
use wasm_bindgen_futures::spawn_local;
use yew::prelude::*;

use crate::{
    services::ops_service,
    types::ops::VersionDiff,
};

#[derive(Clone, PartialEq)]
enum DiffState {
    Idle,
    Loading,
    Loaded(VersionDiff),
    Error(String),
}

#[function_component(ConfigDiffPage)]
pub fn config_diff_page() -> Html {
    let template_id = use_state(|| String::new());
    let v1          = use_state(|| String::new());
    let v2          = use_state(|| String::new());
    let diff_state  = use_state(|| DiffState::Idle);

    let on_template = { let t = template_id.clone(); Callback::from(move |e: InputEvent| {
        let i: web_sys::HtmlInputElement = e.target_unchecked_into(); t.set(i.value());
    })};
    let on_v1 = { let v = v1.clone(); Callback::from(move |e: InputEvent| {
        let i: web_sys::HtmlInputElement = e.target_unchecked_into(); v.set(i.value());
    })};
    let on_v2 = { let v = v2.clone(); Callback::from(move |e: InputEvent| {
        let i: web_sys::HtmlInputElement = e.target_unchecked_into(); v.set(i.value());
    })};

    let on_diff = {
        let template_id = template_id.clone();
        let v1          = v1.clone();
        let v2          = v2.clone();
        let ds          = diff_state.clone();
        Callback::from(move |e: SubmitEvent| {
            e.prevent_default();
            let tid = (*template_id).parse::<Uuid>();
            let id1 = (*v1).parse::<Uuid>();
            let id2 = (*v2).parse::<Uuid>();
            match (tid, id1, id2) {
                (Ok(tid), Ok(id1), Ok(id2)) => {
                    let ds = ds.clone();
                    ds.set(DiffState::Loading);
                    spawn_local(async move {
                        match ops_service::diff_versions(tid, id1, id2).await {
                            Ok(diff) => ds.set(DiffState::Loaded(diff)),
                            Err(e)   => ds.set(DiffState::Error(e)),
                        }
                    });
                }
                _ => ds.set(DiffState::Error("Please enter valid UUIDs".to_string())),
            }
        })
    };

    let diff_content = match &*diff_state {
        DiffState::Idle    => html! { <p class="hint">{ "Enter two version IDs above to compare them." }</p> },
        DiffState::Loading => html! { <div class="spinner" /> },
        DiffState::Error(e) => html! { <p class="error-state__message">{ e }</p> },
        DiffState::Loaded(diff) => html! {
            <div class="diff-view">
                <div class="diff-view__header">
                    { format!("v{} → v{}", diff.old_version, diff.new_version) }
                </div>
                if !diff.added.is_empty() {
                    <section class="diff-view__section diff-view__section--added">
                        <h3>{ "Added" }</h3>
                        <ul>{ for diff.added.iter().map(|k| html! { <li>{ k }</li> }) }</ul>
                    </section>
                }
                if !diff.removed.is_empty() {
                    <section class="diff-view__section diff-view__section--removed">
                        <h3>{ "Removed" }</h3>
                        <ul>{ for diff.removed.iter().map(|k| html! { <li>{ k }</li> }) }</ul>
                    </section>
                }
                if !diff.changed.is_empty() {
                    <section class="diff-view__section diff-view__section--changed">
                        <h3>{ "Changed" }</h3>
                        <table class="data-table">
                            <thead><tr><th>{ "Key" }</th><th>{ "Old" }</th><th>{ "New" }</th></tr></thead>
                            <tbody>
                                { for diff.changed.iter().map(|c| html! {
                                    <tr>
                                        <td>{ &c.key }</td>
                                        <td class="diff-cell--old">{ c.old_value.to_string() }</td>
                                        <td class="diff-cell--new">{ c.new_value.to_string() }</td>
                                    </tr>
                                }) }
                            </tbody>
                        </table>
                    </section>
                }
                if diff.added.is_empty() && diff.removed.is_empty() && diff.changed.is_empty() {
                    <p>{ "No differences found — these versions are identical." }</p>
                }
            </div>
        },
    };

    html! {
        <div class="page">
            <header class="page__header">
                <h1 class="page__title">{ "Config Version Diff" }</h1>
            </header>
            <div class="page__body">
                <form onsubmit={on_diff} class="inline-form">
                    <label class="form-field form-field--inline">
                        <span>{ "Template ID" }</span>
                        <input type="text" placeholder="uuid" oninput={on_template} class="form-field__input" />
                    </label>
                    <label class="form-field form-field--inline">
                        <span>{ "Version A (older)" }</span>
                        <input type="text" placeholder="uuid" oninput={on_v1} class="form-field__input" />
                    </label>
                    <label class="form-field form-field--inline">
                        <span>{ "Version B (newer)" }</span>
                        <input type="text" placeholder="uuid" oninput={on_v2} class="form-field__input" />
                    </label>
                    <button type="submit" class="btn btn--primary">{ "Compare" }</button>
                </form>
                { diff_content }
            </div>
        </div>
    }
}
