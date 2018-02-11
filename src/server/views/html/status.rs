use std::collections::BTreeMap;

use base64::display::Base64Display;
use hyper::Response;
use maud::{Markup, html};

use ::engine::AnalyzeDependenciesOutcome;
use ::models::crates::{CrateName, AnalyzedDependency, AnalyzedDependencies};
use ::models::repo::RepoPath;
use ::server::assets;

fn dependency_tables(crate_name: CrateName, deps: AnalyzedDependencies) -> Markup {
    html! {
        h2 class="title is-3" {
            "Crate "
            code (crate_name.as_ref())
        }

        @if deps.main.is_empty() && deps.dev.is_empty() && deps.build.is_empty() {
            p class="notification has-text-centered" "No dependencies! 🎉"
        }

        @if !deps.main.is_empty() {
            (dependency_table("Dependencies", deps.main))
        }

        @if !deps.dev.is_empty() {
            (dependency_table("Dev dependencies", deps.dev))
        }

        @if !deps.build.is_empty() {
            (dependency_table("Build dependencies", deps.build))
        }
    }
}

fn dependency_table(title: &str, deps: BTreeMap<CrateName, AnalyzedDependency>) -> Markup {
    let count_total = deps.len();
    let count_outdated = deps.iter().filter(|&(_, dep)| dep.is_outdated()).count();

    html! {
        h3 class="title is-4" (title)
        p class="subtitle is-5" {
            @if count_outdated > 0 {
                (format!(" ({} total, {} up-to-date, {} outdated)", count_total, count_total - count_outdated, count_outdated))
            } @else {
                (format!(" ({} total, all up-to-date)", count_total))
            }
        }

        table class="table is-fullwidth is-striped is-hoverable" {
            thead {
                tr {
                    th "Crate"
                    th class="has-text-right" "Required"
                    th class="has-text-right" "Latest"
                    th class="has-text-right" "Status"
                }
            }
            tbody {
                @for (name, dep) in deps {
                    tr {
                        td {
                            a href=(format!("https://crates.io/crates/{}", name.as_ref())) (name.as_ref())
                        }
                        td class="has-text-right" code (dep.required.to_string())
                        td class="has-text-right" {
                            @if let Some(ref latest) = dep.latest {
                                code (latest.to_string())
                            } @else {
                                "N/A"
                            }
                        }
                        td class="has-text-right" {
                            @if dep.is_outdated() {
                                span class="tag is-warning" "out of date"
                            } @else {
                                span class="tag is-success" "up to date"
                            }
                        }
                    }
                }
            }
        }
    }
}

pub fn render(analysis_outcome: AnalyzeDependenciesOutcome, repo_path: RepoPath) -> Response {
    let self_path = format!("repo/{}/{}/{}", repo_path.site.as_ref(), repo_path.qual.as_ref(), repo_path.name.as_ref());
    let status_base_url = format!("{}/{}", &super::SELF_BASE_URL as &str, self_path);
    let title = format!("{} / {}", repo_path.qual.as_ref(), repo_path.name.as_ref());

    let (hero_class, status_asset) = if analysis_outcome.any_outdated() {
        ("is-warning", assets::BADGE_OUTDATED_SVG.as_ref())
    } else {
        ("is-success", assets::BADGE_UPTODATE_SVG.as_ref())
    };

    let status_data_url = format!("data:image/svg+xml;base64,{}", Base64Display::standard(status_asset));

    super::render_html(&title, html! {
        section class=(format!("hero {}", hero_class)) {
            div class="hero-head" (super::render_navbar())
            div class="hero-body" {
                div class="container" {
                    h1 class="title is-1" {
                        a href=(format!("{}/{}/{}", repo_path.site.to_base_uri(), repo_path.qual.as_ref(), repo_path.name.as_ref())) {
                            i class="fa fa-github" ""
                            (format!(" {} / {}", repo_path.qual.as_ref(), repo_path.name.as_ref()))
                        }
                    }

                    img src=(status_data_url);
                }
            }
            div class="hero-footer" {
                div class="container" {
                    pre class="is-size-7" {
                        (format!("[![dependency status]({}/status.svg)]({})", status_base_url, status_base_url))
                    }
                }
            }
        }
        section class="section" {
            div class="container" {
                @for (crate_name, deps) in analysis_outcome.crates {
                    (dependency_tables(crate_name, deps))
                }
            }
        }
    })
}
