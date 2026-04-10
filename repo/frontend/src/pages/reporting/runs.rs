/// Reporting: Report run history with export actions.
///
/// Lists all past report runs (both manual and scheduled), shows their status,
/// and provides export download links for completed runs in PDF or CSV format.
///
/// Export links use `export_run_url_with_watermark` so the backend can stamp
/// the generated file with the viewer's identity and the export timestamp,
/// enabling full audit-trail traceability.
use chrono::Utc;
use wasm_bindgen_futures::spawn_local;
use yew::prelude::*;

use crate::{
    services::reporting_service,
    store::auth_store::AuthContext,
    types::reporting::ReportRun,
};

#[derive(Clone, PartialEq)]
enum PageState { Loading, Loaded(Vec<ReportRun>), Error(String) }

#[function_component(ReportRunsPage)]
pub fn report_runs_page() -> Html {
    let auth       = use_context::<AuthContext>().expect("AuthContext missing");
    let page_state = use_state(|| PageState::Loading);

    {
        let ps = page_state.clone();
        use_effect_with((), move |_| {
            spawn_local(async move {
                match reporting_service::list_runs().await {
                    Ok(runs) => ps.set(PageState::Loaded(runs)),
                    Err(e)   => ps.set(PageState::Error(e)),
                }
            });
            || ()
        });
    }

    let on_refresh = {
        let ps = page_state.clone();
        Callback::from(move |_: MouseEvent| {
            let ps = ps.clone();
            spawn_local(async move {
                ps.set(PageState::Loading);
                match reporting_service::list_runs().await {
                    Ok(runs) => ps.set(PageState::Loaded(runs)),
                    Err(e)   => ps.set(PageState::Error(e)),
                }
            });
        })
    };

    let content = match &*page_state {
        PageState::Loading => html! {
            <div class="loading-state"><div class="spinner"/></div>
        },
        PageState::Error(e) => html! {
            <p class="error-state__message">{ e }</p>
        },
        PageState::Loaded(runs) if runs.is_empty() => html! {
            <div class="empty-state">
                <p>{ "No report runs yet." }</p>
                <p>{ "Trigger a run from the Report Schedules page." }</p>
            </div>
        },
        PageState::Loaded(runs) => {
            // Viewer identity and timestamp are embedded in export URLs so the
            // backend can watermark the generated file for audit traceability.
            let viewer      = auth.session.as_ref()
                .map(|s| s.username.clone())
                .unwrap_or_else(|| "unknown".to_string());
            let exported_at = Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string();
            html! {
                <table class="data-table">
                    <thead>
                        <tr>
                            <th>{ "Started" }</th>
                            <th>{ "Status" }</th>
                            <th>{ "Format" }</th>
                            <th>{ "Period" }</th>
                            <th>{ "Completed" }</th>
                            <th>{ "Export" }</th>
                        </tr>
                    </thead>
                    <tbody>
                        { for runs.iter().map(|r| {
                            let run_id     = r.id;
                            let is_done    = r.is_completed();
                            let status_cls = if r.is_completed() { "badge--published" }
                                           else if r.is_running() { "badge--scheduled" }
                                           else { "badge--draft" };
                            let csv_url  = reporting_service::export_run_url_with_watermark(
                                run_id, "csv", &viewer, &exported_at,
                            );
                            let pdf_url  = reporting_service::export_run_url_with_watermark(
                                run_id, "pdf", &viewer, &exported_at,
                            );
                            html! {
                                <tr key={run_id.to_string()}>
                                    <td>{ r.started_at.format("%Y-%m-%d %H:%M").to_string() }</td>
                                    <td>
                                        <span class={format!("badge {}", status_cls)}>
                                            { &r.status }
                                        </span>
                                    </td>
                                    <td class="mono">{ &r.output_format }</td>
                                    <td>
                                        { format!(
                                            "{} – {}",
                                            r.date_from.format("%Y-%m-%d"),
                                            r.date_to.format("%Y-%m-%d"),
                                        ) }
                                    </td>
                                    <td>
                                        { r.completed_at
                                            .map(|t| t.format("%Y-%m-%d %H:%M").to_string())
                                            .unwrap_or_else(|| "—".to_string()) }
                                    </td>
                                    <td class="action-cell">
                                        if is_done {
                                            <a href={csv_url}
                                               download=""
                                               class="btn btn--small btn--secondary">
                                                { "CSV" }
                                            </a>
                                            { " " }
                                            <a href={pdf_url}
                                               download=""
                                               class="btn btn--small btn--secondary">
                                                { "PDF" }
                                            </a>
                                        }
                                    </td>
                                </tr>
                            }
                        }) }
                    </tbody>
                </table>
            }
        }
    };

    html! {
        <div class="page">
            <header class="page__header">
                <h1 class="page__title">{ "Report Runs" }</h1>
                <div class="page__actions">
                    <button class="btn btn--secondary" onclick={on_refresh}>
                        { "Refresh" }
                    </button>
                </div>
            </header>
            <div class="page__body">
                { content }
            </div>
        </div>
    }
}
