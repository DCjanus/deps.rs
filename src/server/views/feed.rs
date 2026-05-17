use actix_web::{
    HttpRequest, HttpResponse,
    http::{
        StatusCode,
        header::{ETag, EntityTag, IF_NONE_MATCH},
    },
};
use rss::{CategoryBuilder, ChannelBuilder, GuidBuilder, Item, ItemBuilder};
use sha1::{Digest, Sha1};

use crate::{
    engine::AnalyzeDependenciesOutcome,
    models::{
        SubjectPath,
        crates::{AnalyzedDependencies, AnalyzedDependency, CrateName},
    },
};

const FEED_TTL_MINUTES: &str = "60";

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum DependencyKind {
    Main,
    Dev,
    Build,
}

impl DependencyKind {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            DependencyKind::Main => "main",
            DependencyKind::Dev => "dev",
            DependencyKind::Build => "build",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum FeedIssueKind {
    Outdated,
    Insecure,
}

impl FeedIssueKind {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            FeedIssueKind::Outdated => "outdated",
            FeedIssueKind::Insecure => "insecure",
        }
    }
}

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct FeedItem {
    pub(crate) package_name: String,
    pub(crate) dependency_kind: DependencyKind,
    pub(crate) dependency_name: String,
    pub(crate) issue_kind: FeedIssueKind,
    pub(crate) title: String,
    pub(crate) description: String,
    pub(crate) guid: String,
}

pub(crate) fn response(
    request: &HttpRequest,
    analysis_outcome: &AnalyzeDependenciesOutcome,
    subject_path: &SubjectPath,
    repo_path: Option<&str>,
    html_url: &str,
) -> HttpResponse {
    let body = render(analysis_outcome, subject_path, repo_path, html_url);
    let etag_value = format!("{:x}", Sha1::digest(body.as_bytes()));
    let etag = EntityTag::new_strong(etag_value);

    if request
        .headers()
        .get(IF_NONE_MATCH)
        .and_then(|value| value.to_str().ok())
        .is_some_and(|value| etag_matches(value, &etag))
    {
        return HttpResponse::build(StatusCode::NOT_MODIFIED)
            .insert_header(ETag(etag))
            .finish();
    }

    HttpResponse::Ok()
        .insert_header(("Content-Type", "application/rss+xml; charset=utf-8"))
        .insert_header(ETag(etag))
        .body(body)
}

pub(crate) fn render(
    analysis_outcome: &AnalyzeDependenciesOutcome,
    subject_path: &SubjectPath,
    repo_path: Option<&str>,
    html_url: &str,
) -> String {
    let items = feed_items(analysis_outcome, subject_path, repo_path)
        .into_iter()
        .map(|item| rss_item(item, html_url))
        .collect::<Vec<_>>();

    channel(subject_path, html_url, items).to_string()
}

fn channel(subject_path: &SubjectPath, html_url: &str, items: Vec<Item>) -> rss::Channel {
    ChannelBuilder::default()
        .title(channel_title(subject_path))
        .link(html_url)
        .description("Outdated and insecure dependency status reported by deps.rs.")
        .ttl(Some(FEED_TTL_MINUTES.to_string()))
        .items(items)
        .build()
}

fn etag_matches(if_none_match: &str, etag: &EntityTag) -> bool {
    if_none_match.split(',').map(str::trim).any(|candidate| {
        candidate == "*"
            || candidate
                .parse::<EntityTag>()
                .is_ok_and(|candidate| candidate.weak_eq(etag))
    })
}

pub(crate) fn channel_title(subject_path: &SubjectPath) -> String {
    match subject_path {
        SubjectPath::Repo(repo_path) => {
            format!(
                "deps.rs: {}/{}/{} dependency status",
                repo_path.site,
                repo_path.qual.as_ref(),
                repo_path.name.as_ref()
            )
        }
        SubjectPath::Crate(crate_path) => {
            format!(
                "deps.rs: {} {} dependency status",
                crate_path.name.as_ref(),
                crate_path.version
            )
        }
    }
}

fn rss_item(item: FeedItem, html_url: &str) -> Item {
    ItemBuilder::default()
        .title(item.title)
        .link(Some(html_url.to_string()))
        .description(item.description)
        .guid(
            GuidBuilder::default()
                .value(item.guid)
                .permalink(false)
                .build(),
        )
        .category(category(item.issue_kind.as_str()))
        .category(category(item.dependency_kind.as_str()))
        .build()
}

fn category(name: &str) -> rss::Category {
    CategoryBuilder::default().name(name).build()
}

pub(crate) fn feed_items(
    analysis_outcome: &AnalyzeDependenciesOutcome,
    subject_path: &SubjectPath,
    repo_path: Option<&str>,
) -> Vec<FeedItem> {
    let mut items = Vec::new();
    let subject_id = subject_id(subject_path, repo_path);

    for (package_name, deps) in &analysis_outcome.crates {
        collect_dependency_items(&mut items, &subject_id, package_name, deps);
    }

    items.sort();
    items
}

fn collect_dependency_items(
    items: &mut Vec<FeedItem>,
    subject_id: &str,
    package_name: &CrateName,
    deps: &AnalyzedDependencies,
) {
    for (dep_name, dep) in &deps.main {
        collect_dependency_item(
            items,
            subject_id,
            package_name,
            DependencyKind::Main,
            dep_name,
            dep,
        );
    }

    for (dep_name, dep) in &deps.dev {
        collect_dependency_item(
            items,
            subject_id,
            package_name,
            DependencyKind::Dev,
            dep_name,
            dep,
        );
    }

    for (dep_name, dep) in &deps.build {
        collect_dependency_item(
            items,
            subject_id,
            package_name,
            DependencyKind::Build,
            dep_name,
            dep,
        );
    }
}

fn collect_dependency_item(
    items: &mut Vec<FeedItem>,
    subject_id: &str,
    package_name: &CrateName,
    dependency_kind: DependencyKind,
    dependency_name: &CrateName,
    dep: &AnalyzedDependency,
) {
    if dep.is_outdated() {
        items.push(build_item(
            subject_id,
            package_name,
            dependency_kind,
            dependency_name,
            dep,
            FeedIssueKind::Outdated,
        ));
    }

    if dep.is_insecure() {
        items.push(build_item(
            subject_id,
            package_name,
            dependency_kind,
            dependency_name,
            dep,
            FeedIssueKind::Insecure,
        ));
    }
}

fn build_item(
    subject_id: &str,
    package_name: &CrateName,
    dependency_kind: DependencyKind,
    dependency_name: &CrateName,
    dep: &AnalyzedDependency,
    issue_kind: FeedIssueKind,
) -> FeedItem {
    let dependency_name = dependency_name.as_ref().to_string();
    let package_name = package_name.as_ref().to_string();
    let required = dep.required.to_string();
    let guid = item_guid(
        subject_id,
        &package_name,
        dependency_kind,
        &dependency_name,
        issue_kind,
        &required,
    );

    FeedItem {
        title: format!(
            "{package_name}: {dependency_name} is {}",
            issue_kind.as_str()
        ),
        description: item_description(dep),
        package_name,
        dependency_kind,
        dependency_name,
        issue_kind,
        guid,
    }
}

fn item_description(dep: &AnalyzedDependency) -> String {
    let latest_that_matches = dep
        .latest_that_matches
        .as_ref()
        .map_or_else(|| "none".to_string(), ToString::to_string);
    let latest = dep
        .latest
        .as_ref()
        .map_or_else(|| "none".to_string(), ToString::to_string);

    format!(
        "Required: {}. Latest matching: {latest_that_matches}. Latest available: {latest}.",
        dep.required
    )
}

fn item_guid(
    subject_id: &str,
    package_name: &str,
    dependency_kind: DependencyKind,
    dependency_name: &str,
    issue_kind: FeedIssueKind,
    required: &str,
) -> String {
    let declaration = [
        subject_id,
        package_name,
        dependency_kind.as_str(),
        dependency_name,
        issue_kind.as_str(),
        required,
    ]
    .join("\0");
    let digest = Sha1::digest(declaration.as_bytes());

    format!(
        "deps.rs:{subject_id}:{}:{dependency_name}:{}:decl={digest:x}",
        dependency_kind.as_str(),
        issue_kind.as_str()
    )
}

fn normalized_repo_path(path: Option<&str>) -> Option<String> {
    path.map(str::trim)
        .map(|path| path.trim_matches('/'))
        .filter(|path| !path.is_empty())
        .map(ToOwned::to_owned)
}

fn subject_id(subject_path: &SubjectPath, path: Option<&str>) -> String {
    match subject_path {
        SubjectPath::Repo(repo_path) => {
            let mut subject_id = format!(
                "repo:{}/{}/{}",
                repo_path.site,
                repo_path.qual.as_ref(),
                repo_path.name.as_ref()
            );

            if let Some(path) = normalized_repo_path(path) {
                subject_id.push_str(":path=");
                subject_id.push_str(&path);
            }

            subject_id
        }
        SubjectPath::Crate(crate_path) => {
            format!("crate:{}/{}", crate_path.name.as_ref(), crate_path.version)
        }
    }
}

#[cfg(test)]
mod tests {
    use indexmap::IndexMap;
    use semver::{Version, VersionReq};

    use super::*;
    use crate::models::{
        crates::{AnalyzedDependency, CratePath},
        repo::RepoPath,
    };

    fn dep(
        required: &str,
        latest_that_matches: Option<&str>,
        latest: Option<&str>,
    ) -> AnalyzedDependency {
        AnalyzedDependency {
            required: VersionReq::parse(required).unwrap(),
            latest_that_matches: latest_that_matches
                .map(|version| Version::parse(version).unwrap()),
            latest: latest.map(|version| Version::parse(version).unwrap()),
            vulnerabilities: Vec::new(),
        }
    }

    fn outcome(dependency: AnalyzedDependency) -> AnalyzeDependenciesOutcome {
        let mut main = IndexMap::new();
        main.insert("tokio".parse().unwrap(), dependency);

        AnalyzeDependenciesOutcome {
            crates: vec![(
                "demo".parse().unwrap(),
                AnalyzedDependencies {
                    main,
                    dev: IndexMap::new(),
                    build: IndexMap::new(),
                },
            )],
            duration: std::time::Duration::from_millis(12),
        }
    }

    fn workspace_outcome() -> AnalyzeDependenciesOutcome {
        let mut api_main = IndexMap::new();
        api_main.insert(
            "tokio".parse().unwrap(),
            dep("~1.32", Some("1.32.9"), Some("1.33.0")),
        );

        let mut worker_main = IndexMap::new();
        worker_main.insert(
            "tokio".parse().unwrap(),
            dep("~1.32", Some("1.32.9"), Some("1.33.0")),
        );

        AnalyzeDependenciesOutcome {
            crates: vec![
                (
                    "api".parse().unwrap(),
                    AnalyzedDependencies {
                        main: api_main,
                        dev: IndexMap::new(),
                        build: IndexMap::new(),
                    },
                ),
                (
                    "worker".parse().unwrap(),
                    AnalyzedDependencies {
                        main: worker_main,
                        dev: IndexMap::new(),
                        build: IndexMap::new(),
                    },
                ),
            ],
            duration: std::time::Duration::from_millis(12),
        }
    }

    fn subject() -> SubjectPath {
        SubjectPath::Crate(CratePath::from_parts("demo", "1.0.0").unwrap())
    }

    fn repo_subject() -> SubjectPath {
        SubjectPath::Repo(RepoPath::from_parts("github", "deps-rs", "deps.rs").unwrap())
    }

    #[test]
    fn renders_stable_xml_without_request_timing() {
        let outcome = outcome(dep("~1.32", Some("1.32.9"), Some("1.33.0")));

        let first = render(
            &outcome,
            &subject(),
            None,
            "https://deps.rs/crate/demo/1.0.0",
        );
        let second = render(
            &outcome,
            &subject(),
            None,
            "https://deps.rs/crate/demo/1.0.0",
        );

        assert_eq!(first, second);
        assert!(first.starts_with("<?xml version=\"1.0\" encoding=\"utf-8\"?>"));
        assert!(first.contains("<ttl>60</ttl>"));
        assert!(!first.contains("lastBuildDate"));
        assert!(!first.contains("pubDate"));
    }

    #[test]
    fn if_none_match_uses_weak_etag_comparison() {
        let etag = EntityTag::new_strong("feed-hash".to_string());

        assert!(etag_matches("\"feed-hash\"", &etag));
        assert!(etag_matches("W/\"feed-hash\"", &etag));
        assert!(etag_matches("\"other\", W/\"feed-hash\"", &etag));
        assert!(!etag_matches("W/\"other\"", &etag));
    }

    #[test]
    fn latest_version_change_does_not_change_guid() {
        let earlier = feed_items(
            &outcome(dep("~1.32", Some("1.32.9"), Some("1.33.0"))),
            &subject(),
            None,
        );
        let later = feed_items(
            &outcome(dep("~1.32", Some("1.32.9"), Some("1.34.0"))),
            &subject(),
            None,
        );

        assert_eq!(earlier[0].guid, later[0].guid);
        assert_ne!(earlier[0].description, later[0].description);
    }

    #[test]
    fn requirement_change_changes_guid() {
        let earlier = feed_items(
            &outcome(dep("~1.32", Some("1.32.9"), Some("1.33.0"))),
            &subject(),
            None,
        );
        let later = feed_items(
            &outcome(dep("~1.33", Some("1.33.5"), Some("1.34.0"))),
            &subject(),
            None,
        );

        assert_ne!(earlier[0].guid, later[0].guid);
    }

    #[test]
    fn item_title_includes_package_name() {
        let items = feed_items(&workspace_outcome(), &repo_subject(), None);

        let titles = items
            .iter()
            .map(|item| item.title.as_str())
            .collect::<Vec<_>>();

        assert_eq!(
            titles,
            ["api: tokio is outdated", "worker: tokio is outdated"]
        );
    }

    #[test]
    fn omits_items_when_dependencies_are_current() {
        let items = feed_items(
            &outcome(dep("~1.38", Some("1.38.0"), Some("1.38.0"))),
            &subject(),
            None,
        );

        assert!(items.is_empty());
    }

    #[test]
    fn serializes_xml_special_characters_safely() {
        let outcome = outcome(dep(">=1.0, <2.0", Some("1.5.0"), Some("2.0.0")));

        let xml = render(
            &outcome,
            &subject(),
            None,
            "https://deps.rs/crate/demo/1.0.0?path=a&b=c",
        );

        assert!(xml.contains("<![CDATA[Required: >=1.0, <2.0."));
        assert!(xml.contains("https://deps.rs/crate/demo/1.0.0?path=a&amp;b=c"));
    }

    #[test]
    fn repo_path_changes_guid() {
        let root = feed_items(
            &outcome(dep("~1.32", Some("1.32.9"), Some("1.33.0"))),
            &repo_subject(),
            None,
        );
        let service_a = feed_items(
            &outcome(dep("~1.32", Some("1.32.9"), Some("1.33.0"))),
            &repo_subject(),
            Some("service-a"),
        );
        let service_b = feed_items(
            &outcome(dep("~1.32", Some("1.32.9"), Some("1.33.0"))),
            &repo_subject(),
            Some("service-b"),
        );

        assert_ne!(root[0].guid, service_a[0].guid);
        assert_ne!(service_a[0].guid, service_b[0].guid);
    }

    #[test]
    fn repo_path_normalizes_guid() {
        let untrimmed = feed_items(
            &outcome(dep("~1.32", Some("1.32.9"), Some("1.33.0"))),
            &repo_subject(),
            Some("/service-a/"),
        );
        let normalized = feed_items(
            &outcome(dep("~1.32", Some("1.32.9"), Some("1.33.0"))),
            &repo_subject(),
            Some("service-a"),
        );

        assert_eq!(untrimmed[0].guid, normalized[0].guid);
    }
}
