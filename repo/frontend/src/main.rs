mod components;
mod pages;
mod services;
mod store;
mod types;

use yew::prelude::*;
use yew_router::prelude::*;

use components::nav::NavSidebar;
use pages::{switch, Route};
use store::{
    auth_store::AuthProvider,
    notification_store::InboxContextProvider,
};

/// Root application component.
///
/// Provides the three context layers required by all descendant components:
///   1. `AuthProvider`         — session token + role (persisted to localStorage)
///   2. `InboxContextProvider` — unread notification badge state
///   3. `BrowserRouter`        — yew-router client-side routing
///
/// The shell renders `NavSidebar` (role-aware) alongside the current route's
/// page component; the sidebar is hidden on the `/login` route.
#[function_component(App)]
fn app() -> Html {
    html! {
        <AuthProvider>
            <InboxContextProvider>
                <BrowserRouter>
                    <AppShell />
                </BrowserRouter>
            </InboxContextProvider>
        </AuthProvider>
    }
}

/// Inner shell that has access to the router context.
///
/// Shows the sidebar navigation for all routes except `/login` and `/404`.
#[function_component(AppShell)]
fn app_shell() -> Html {
    let route = use_route::<Route>();
    let show_nav = !matches!(route, Some(Route::Login) | Some(Route::NotFound) | None);

    html! {
        <div class="app-layout">
            if show_nav {
                <NavSidebar />
            }
            <main class={if show_nav { "app-layout__content" } else { "app-layout__content app-layout__content--full" }}>
                <Switch<Route> render={switch} />
            </main>
        </div>
    }
}

fn main() {
    console_log::init_with_level(log::Level::Debug)
        .expect("failed to init console_log");
    yew::Renderer::<App>::new().render();
}
