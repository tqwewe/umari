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
                script { (maud::PreEscaped("tailwind.config = { darkMode: 'class' }")) }
                script src="https://unpkg.com/htmx.org@2.0.4" {}
                style {
                    (maud::PreEscaped(r#"
  [data-active] { background-color: #eef2ff; color: #3730a3; }
  .dark [data-active] { background-color: rgba(79,70,229,0.15); color: #818cf8; }
  [data-tab-active] { color: #4f46e5; border-color: #4f46e5; }
  .dark [data-tab-active] { color: #818cf8; border-color: #818cf8; }
                    "#))
                }
                script {
                    (maud::PreEscaped(r#"(function(){
  const s = localStorage.getItem('umari-dark');
  const p = window.matchMedia('(prefers-color-scheme: dark)').matches;
  if (s === 'dark' || (!s && p)) document.documentElement.classList.add('dark');
})();"#))
                }
            }
            body class="bg-gray-50 dark:bg-gray-950 text-gray-900 dark:text-gray-100 flex" {
                aside class="w-52 bg-white dark:bg-gray-900 border-r border-gray-200 dark:border-gray-700 fixed h-screen flex flex-col z-10" {
                    div class="h-14 flex items-center px-5 border-b border-gray-200 dark:border-gray-700 shrink-0" {
                        span class="font-bold text-base tracking-tight text-gray-900 dark:text-gray-100" { "Umari" }
                    }
                    nav class="flex-1 p-3 overflow-y-auto" {
                        p class="text-xs font-semibold text-gray-400 dark:text-gray-500 uppercase tracking-wider px-2 mb-2" { "Modules" }
                        a href="/ui/commands"
                            hx-get="/ui/commands"
                            hx-target="#content"
                            hx-push-url="/ui/commands"
                            data-nav="/ui/commands"
                            class="nav-link flex items-center px-3 py-2 rounded-md text-sm font-medium text-gray-600 dark:text-gray-400 hover:bg-gray-50 dark:hover:bg-gray-800 hover:text-gray-900 dark:hover:text-gray-100 transition-colors"
                            { "Commands" }
                        a href="/ui/projectors"
                            hx-get="/ui/projectors"
                            hx-target="#content"
                            hx-push-url="/ui/projectors"
                            data-nav="/ui/projectors"
                            class="nav-link flex items-center px-3 py-2 rounded-md text-sm font-medium text-gray-600 dark:text-gray-400 hover:bg-gray-50 dark:hover:bg-gray-800 hover:text-gray-900 dark:hover:text-gray-100 transition-colors"
                            { "Projectors" }
                        a href="/ui/effects"
                            hx-get="/ui/effects"
                            hx-target="#content"
                            hx-push-url="/ui/effects"
                            data-nav="/ui/effects"
                            class="nav-link flex items-center px-3 py-2 rounded-md text-sm font-medium text-gray-600 dark:text-gray-400 hover:bg-gray-50 dark:hover:bg-gray-800 hover:text-gray-900 dark:hover:text-gray-100 transition-colors"
                            { "Effects" }
                        p class="text-xs font-semibold text-gray-400 dark:text-gray-500 uppercase tracking-wider px-2 mt-4 mb-2" { "Observability" }
                        a href="/ui/events"
                            hx-get="/ui/events"
                            hx-target="#content"
                            hx-push-url="/ui/events"
                            data-nav="/ui/events"
                            class="nav-link flex items-center px-3 py-2 rounded-md text-sm font-medium text-gray-600 dark:text-gray-400 hover:bg-gray-50 dark:hover:bg-gray-800 hover:text-gray-900 dark:hover:text-gray-100 transition-colors"
                            { "Events" }
                    }
                    div class="border-t border-gray-200 dark:border-gray-700 p-3 shrink-0" {
                        button onclick="umariToggleDark()"
                            class="w-full flex items-center gap-2 px-3 py-2 rounded-md text-sm text-gray-500 dark:text-gray-400 hover:bg-gray-50 dark:hover:bg-gray-800 transition-colors"
                        {
                            span id="dark-icon" {}
                            span { "Toggle theme" }
                        }
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
        const active = path === el.dataset.nav || path.startsWith(el.dataset.nav + '/');
        el.toggleAttribute('data-active', active);
        el.classList.toggle('text-gray-600', !active);
    });
}
function umariToggleDark() {
    const dark = document.documentElement.classList.toggle('dark');
    localStorage.setItem('umari-dark', dark ? 'dark' : 'light');
    updateDarkIcon();
}
function updateDarkIcon() {
    const el = document.getElementById('dark-icon');
    if (el) el.textContent = document.documentElement.classList.contains('dark') ? '☀' : '☽';
}
document.addEventListener('click', function(e) {
    if (e.ctrlKey || e.metaKey || e.shiftKey) {
        const el = e.target.closest('a[href][hx-get], a[href][data-hx-get]');
        if (el) {
            e.preventDefault();
            e.stopImmediatePropagation();
            window.open(el.getAttribute('href'), '_blank');
        }
    }
}, true);
document.addEventListener('DOMContentLoaded', function() { updateNav(); updateDarkIcon(); });
document.addEventListener('htmx:pushedIntoHistory', updateNav);
document.addEventListener('htmx:afterSwap', function() {
    const el = document.querySelector('[data-page-title]');
    if (el) document.title = el.dataset.pageTitle + ' - Umari';
});
                    "#))
                }
            }
        }
    }
}
