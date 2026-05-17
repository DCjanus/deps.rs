use actix_web::{HttpResponse, http::header::ContentType};
use maud::{Markup, html};

use crate::{
    engine::AnalyzeDependenciesOutcome,
    models::SubjectPath,
    server::views::feed::{self, FeedItem},
};

pub(crate) fn response(
    analysis_outcome: AnalyzeDependenciesOutcome,
    subject_path: SubjectPath,
    html_url: &str,
    feed_xml_url: &str,
) -> HttpResponse {
    let title = feed::channel_title(&subject_path);
    let duration = analysis_outcome.duration;
    let items = feed::feed_items(&analysis_outcome, &subject_path);
    let html = super::render_html_with_feed(
        &title,
        render_body(&title, html_url, feed_xml_url, items, duration),
        Some(feed_xml_url),
    );

    HttpResponse::Ok()
        .insert_header(ContentType::html())
        .body(html.0)
}

fn render_body(
    title: &str,
    html_url: &str,
    feed_xml_url: &str,
    items: Vec<FeedItem>,
    duration: std::time::Duration,
) -> Markup {
    html! {
        section class="hero is-light" {
            div class="hero-head" { (super::render_navbar()) }
            div class="hero-body" {
                div class="container" {
                    h1 class="title is-1" { (title) }
                    p class="subtitle" { "Outdated and insecure dependency status feed" }
                    p {
                        a class="button is-small" href=(html_url) { "Status page" }
                        " "
                        a class="button is-small" href=(feed_xml_url) { "RSS XML" }
                    }
                }
            }
        }
        section class="section" {
            div class="container" {
                @if items.is_empty() {
                    div class="notification is-success" {
                        "No outdated or insecure dependencies are currently reported."
                    }
                } @else {
                    table class="table is-fullwidth is-striped is-hoverable" {
                        thead {
                            tr {
                                th { "Dependency" }
                                th { "Package" }
                                th { "Scope" }
                                th { "Issue" }
                                th { "Details" }
                            }
                        }
                        tbody {
                            @for item in items {
                                (render_item(item, html_url))
                            }
                        }
                    }
                }
            }
        }
        (super::render_footer(Some(duration)))
    }
}

fn render_item(item: FeedItem, html_url: &str) -> Markup {
    let issue_class = match item.issue_kind.as_str() {
        "insecure" => "tag is-danger",
        "outdated" => "tag is-warning",
        _ => "tag",
    };

    html! {
        tr {
            td {
                a href=(html_url) { (item.dependency_name) }
            }
            td { code { (item.package_name) } }
            td { (item.dependency_kind.as_str()) }
            td { span class=(issue_class) { (item.issue_kind.as_str()) } }
            td { (item.description) }
        }
    }
}
