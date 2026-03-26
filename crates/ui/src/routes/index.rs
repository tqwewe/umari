use axum::response::Redirect;

pub async fn index() -> Redirect {
    Redirect::to("/ui/commands")
}
