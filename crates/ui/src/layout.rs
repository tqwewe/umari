use maud::{DOCTYPE, Markup, html};

pub fn page(title: &str, content: Markup) -> Markup {
    page_inner(title, content, false)
}

pub fn wide_page(title: &str, content: Markup) -> Markup {
    page_inner(title, content, true)
}

pub fn width_wrapper(content: Markup, wide: bool) -> Markup {
    if wide {
        html! { div class="max-w-7xl mx-auto" { (content) } }
    } else {
        html! { div class="max-w-4xl mx-auto" { (content) } }
    }
}

fn page_inner(title: &str, content: Markup, wide: bool) -> Markup {
    html! {
        (DOCTYPE)
        html lang="en" {
            head {
                meta charset="utf-8";
                meta name="viewport" content="width=device-width, initial-scale=1";
                title { (title) " - Umari" }
                script src="https://cdn.tailwindcss.com" {}
                script src="https://unpkg.com/htmx.org@2.0.4" {}
            }
            body class="bg-gray-50 text-gray-900 flex" {
                aside class="w-52 bg-white border-r border-gray-200 fixed h-screen flex flex-col z-10" {
                    div class="h-14 flex items-center px-5 border-b border-gray-200 shrink-0" {
                        span class="font-bold text-base tracking-tight" { "Umari" }
                    }
                    nav class="flex-1 p-3 overflow-y-auto" {
                        p class="text-xs font-semibold text-gray-400 uppercase tracking-wider px-2 mb-2" { "Modules" }
                        a href="/ui/commands"
                            hx-get="/ui/commands"
                            hx-target="#content"
                            hx-push-url="/ui/commands"
                            data-nav="/ui/commands"
                            class="nav-link flex items-center px-3 py-2 rounded-md text-sm font-medium text-gray-600 hover:bg-gray-50 hover:text-gray-900 transition-colors"
                            { "Commands" }
                        a href="/ui/projectors"
                            hx-get="/ui/projectors"
                            hx-target="#content"
                            hx-push-url="/ui/projectors"
                            data-nav="/ui/projectors"
                            class="nav-link flex items-center px-3 py-2 rounded-md text-sm font-medium text-gray-600 hover:bg-gray-50 hover:text-gray-900 transition-colors"
                            { "Projectors" }
                        a href="/ui/policies"
                            hx-get="/ui/policies"
                            hx-target="#content"
                            hx-push-url="/ui/policies"
                            data-nav="/ui/policies"
                            class="nav-link flex items-center px-3 py-2 rounded-md text-sm font-medium text-gray-600 hover:bg-gray-50 hover:text-gray-900 transition-colors"
                            { "Policies" }
                        a href="/ui/effects"
                            hx-get="/ui/effects"
                            hx-target="#content"
                            hx-push-url="/ui/effects"
                            data-nav="/ui/effects"
                            class="nav-link flex items-center px-3 py-2 rounded-md text-sm font-medium text-gray-600 hover:bg-gray-50 hover:text-gray-900 transition-colors"
                            { "Effects" }
                        p class="text-xs font-semibold text-gray-400 uppercase tracking-wider px-2 mt-4 mb-2" { "Observability" }
                        a href="/ui/events"
                            hx-get="/ui/events"
                            hx-target="#content"
                            hx-push-url="/ui/events"
                            data-nav="/ui/events"
                            class="nav-link flex items-center px-3 py-2 rounded-md text-sm font-medium text-gray-600 hover:bg-gray-50 hover:text-gray-900 transition-colors"
                            { "Events" }
                    }
                }
                div class="ml-52 flex-1 min-h-screen" {
                    main class="px-8 py-8" {
                        div #content {
                            (width_wrapper(content, wide))
                        }
                    }
                }
                script {
                    (maud::PreEscaped(r#"
                        function updateNav() {
                            const path = window.location.pathname;
                            document.querySelectorAll('[data-nav]').forEach(el => {
                                const navPath = el.dataset.nav;
                                const active = path === navPath || path.startsWith(navPath + '/');
                                el.classList.toggle('bg-indigo-50', active);
                                el.classList.toggle('text-indigo-700', active);
                                el.classList.toggle('text-gray-600', !active);
                            });
                        }
                        document.addEventListener('DOMContentLoaded', updateNav);
                        document.addEventListener('htmx:pushedIntoHistory', updateNav);
                    "#))
                }
            }
        }
    }
}
