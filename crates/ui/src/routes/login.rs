use axum::{
    Form,
    extract::State,
    http::{StatusCode, header},
    response::{IntoResponse, Redirect, Response},
};
use maud::{DOCTYPE, Markup, PreEscaped, html};
use serde::Deserialize;

use crate::UiState;

pub const SESSION_COOKIE: &str = "umari_session";

fn login_markup(error: bool) -> Markup {
    html! {
        (DOCTYPE)
        html lang="en" {
            head {
                meta charset="utf-8";
                meta name="viewport" content="width=device-width, initial-scale=1";
                title { "Sign In - Umari" }
                script src="https://cdn.tailwindcss.com" {}
                script { (PreEscaped("tailwind.config = { darkMode: 'class' }")) }
                script {
                    (PreEscaped(r#"(function(){
  const s = localStorage.getItem('umari-dark');
  const p = window.matchMedia('(prefers-color-scheme: dark)').matches;
  if (s === 'dark' || (!s && p)) document.documentElement.classList.add('dark');
})();"#))
                }
            }
            body class="bg-gray-50 dark:bg-gray-950 text-gray-900 dark:text-gray-100 min-h-screen flex items-center justify-center" {
                div class="w-full max-w-sm px-4" {
                    div class="bg-white dark:bg-gray-900 rounded-xl border border-gray-200 dark:border-gray-700 shadow-sm p-8" {
                        div class="mb-6 text-center" {
                            span class="font-bold text-xl tracking-tight" { "Umari" }
                            p class="text-sm text-gray-500 dark:text-gray-400 mt-1" { "Enter your API key to continue" }
                        }
                        @if error {
                            div class="mb-4 text-sm text-red-600 dark:text-red-400 bg-red-50 dark:bg-red-900/20 border border-red-200 dark:border-red-800 rounded-lg px-3 py-2" {
                                "Invalid API key."
                            }
                        }
                        form method="post" action="/ui/login" class="space-y-4" {
                            div {
                                label for="key" class="block text-sm font-medium text-gray-700 dark:text-gray-300 mb-1" { "API Key" }
                                input
                                    type="password"
                                    id="key"
                                    name="key"
                                    required
                                    autofocus
                                    class="w-full px-3 py-2 rounded-lg border border-gray-300 dark:border-gray-600 bg-white dark:bg-gray-800 text-sm focus:outline-none focus:ring-2 focus:ring-indigo-500 dark:focus:ring-indigo-400";
                            }
                            button
                                type="submit"
                                class="w-full py-2 px-4 bg-indigo-600 hover:bg-indigo-700 text-white text-sm font-medium rounded-lg transition-colors"
                            { "Sign in" }
                        }
                    }
                }
            }
        }
    }
}

pub async fn login_get() -> Markup {
    login_markup(false)
}

#[derive(Deserialize)]
pub struct LoginForm {
    key: String,
}

pub async fn login_post(State(state): State<UiState>, Form(form): Form<LoginForm>) -> Response {
    let valid = state
        .api_key
        .as_ref()
        .map(|k| k.as_ref() == form.key.as_str())
        .unwrap_or(true);

    if !valid {
        return (StatusCode::UNAUTHORIZED, login_markup(true)).into_response();
    }

    let cookie = format!("{SESSION_COOKIE}={}; HttpOnly; SameSite=Strict; Path=/", form.key);
    ([(header::SET_COOKIE, cookie)], Redirect::to("/")).into_response()
}

pub async fn logout() -> impl IntoResponse {
    let cookie = format!("{SESSION_COOKIE}=; HttpOnly; SameSite=Strict; Path=/; Max-Age=0");
    ([(header::SET_COOKIE, cookie)], Redirect::to("/ui/login"))
}
