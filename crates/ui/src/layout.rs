use maud::{DOCTYPE, Markup, html};

pub fn page(title: &str, content: Markup) -> Markup {
    html! {
        (DOCTYPE)
        html lang="en" {
            head {
                meta charset="utf-8";
                meta name="viewport" content="width=device-width, initial-scale=1";
                title { (title) " - Umari" }
                link rel="stylesheet" href="https://cdn.jsdelivr.net/npm/water.css@2/out/water.css";
                script src="https://unpkg.com/htmx.org@2.0.4" {}
            }
            body {
                header {
                    h1 { "Umari" }
                    nav {
                        a href="/"
                            hx-get="/ui/commands"
                            hx-target="#content"
                            hx-push-url="/"
                            { "Commands" }
                        " · "
                        a href="/ui/projections"
                            hx-get="/ui/projections"
                            hx-target="#content"
                            hx-push-url="/ui/projections"
                            { "Projections" }
                        " · "
                        a href="/ui/active"
                            hx-get="/ui/active"
                            hx-target="#content"
                            hx-push-url="/ui/active"
                            { "Active Modules" }
                    }
                }
                main {
                    div #content {
                        (content)
                    }
                }
            }
        }
    }
}
