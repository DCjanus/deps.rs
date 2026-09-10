use std::cmp::Ordering;

use actix_web::{
    HttpRequest, HttpResponse,
    http::{
        StatusCode,
        header::{ETag, EntityTag, IF_NONE_MATCH},
    },
};
use rss::{
    CategoryBuilder, Channel, ChannelBuilder, GuidBuilder, Item, ItemBuilder,
    extension::atom::{AtomExtensionBuilder, Link},
};
use url::Url;

use crate::{
    engine::AnalyzeDependenciesOutcome,
    models::{
        crates::{AnalyzedDependency, CrateName, CratePath},
        repo::RepoPath,
    },
    server::{SELF_BASE_URL, advisory_anchor, dependency_anchor},
};

const FEED_TTL_MINUTES: &str = "60";
const FEED_CACHE_CONTROL: &str = "public, max-age=300, stale-while-revalidate=60";
const GUID_PREFIX: &str = "urn:deps.rs:dependency-issue:v1:";
const GUID_HASH_CONTEXT: &str = "deps.rs dependency issue GUID v1";
const ETAG_HASH_CONTEXT: &str = "deps.rs dependency feed ETag v1";

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum DependencyKind {
    Main,
    Dev,
    Build,
}

impl DependencyKind {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Main => "main",
            Self::Dev => "dev",
            Self::Build => "build",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum IssueKind {
    Insecure,
    MaybeInsecure,
    Outdated,
}

impl IssueKind {
    fn as_str(self) -> &'static str {
        match self {
            Self::Insecure => "insecure",
            Self::MaybeInsecure => "maybe-insecure",
            Self::Outdated => "outdated",
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::Insecure => "insecure",
            Self::MaybeInsecure => "maybe insecure",
            Self::Outdated => "outdated",
        }
    }
}

/// The logical page represented by a feed. For latest-crate feeds the resolved
/// version is display data, while the stable identity remains `latest`.
#[derive(Debug, Clone)]
pub(crate) enum FeedSubject {
    Repo {
        repo: RepoPath,
        path: Option<String>,
    },
    CrateLatest {
        crate_path: CratePath,
    },
    CratePinned {
        crate_path: CratePath,
    },
}

impl FeedSubject {
    pub(crate) fn repo(repo: RepoPath, path: Option<&str>) -> Self {
        Self::Repo {
            repo,
            path: normalized_repo_path(path),
        }
    }

    pub(crate) fn crate_latest(crate_path: CratePath) -> Self {
        Self::CrateLatest { crate_path }
    }

    pub(crate) fn crate_pinned(crate_path: CratePath) -> Self {
        Self::CratePinned { crate_path }
    }

    pub(crate) fn status_url(&self) -> Url {
        self.base_url(false)
    }

    pub(crate) fn feed_url(&self) -> Url {
        self.base_url(true)
    }

    fn base_url(&self, feed: bool) -> Url {
        let mut url =
            Url::parse(SELF_BASE_URL.as_str()).expect("BASE_URL must be a valid absolute URL");

        {
            let mut segments = url
                .path_segments_mut()
                .expect("BASE_URL must support path segments");
            match self {
                Self::Repo { repo, .. } => {
                    segments.push("repo");
                    let site = repo.site.to_string();
                    segments.extend(site.split('/'));
                    segments.extend([repo.qual.as_ref(), repo.name.as_ref()]);
                }
                Self::CrateLatest { crate_path } => {
                    segments.extend(["crate", crate_path.name.as_ref(), "latest"]);
                }
                Self::CratePinned { crate_path } => {
                    let version = crate_path.version.to_string();
                    segments.extend(["crate", crate_path.name.as_ref(), version.as_str()]);
                }
            }
            if feed {
                segments.push("feed.xml");
            }
        }

        if let Self::Repo {
            path: Some(path), ..
        } = self
        {
            url.query_pairs_mut().append_pair("path", path);
        }

        url
    }

    fn title(&self) -> String {
        match self {
            Self::Repo { repo, path } => {
                let mut name = format!(
                    "{}/{}/{}",
                    repo.site,
                    repo.qual.as_ref(),
                    repo.name.as_ref()
                );
                if let Some(path) = path {
                    name.push('/');
                    name.push_str(path);
                }
                format!("deps.rs: {name} dependency status")
            }
            Self::CrateLatest { crate_path } => format!(
                "deps.rs: {} latest ({}) dependency status",
                crate_path.name.as_ref(),
                crate_path.version
            ),
            Self::CratePinned { crate_path } => format!(
                "deps.rs: {} {} dependency status",
                crate_path.name.as_ref(),
                crate_path.version
            ),
        }
    }

    fn identity_fields(&self) -> Vec<String> {
        match self {
            Self::Repo { repo, path } => vec![
                "repo".to_owned(),
                repo.site.to_string(),
                repo.qual.as_ref().to_owned(),
                repo.name.as_ref().to_owned(),
                path.clone().unwrap_or_default(),
            ],
            Self::CrateLatest { crate_path } => vec![
                "crate".to_owned(),
                crate_path.name.as_ref().to_owned(),
                "latest".to_owned(),
            ],
            Self::CratePinned { crate_path } => vec![
                "crate".to_owned(),
                crate_path.name.as_ref().to_owned(),
                "pinned".to_owned(),
                crate_path.version.to_string(),
            ],
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
struct FeedItem {
    issue_kind: IssueKind,
    package_name: String,
    dependency_kind: DependencyKind,
    dependency_name: String,
    advisory_id: Option<String>,
    required: String,
    latest_that_matches: Option<String>,
    latest: Option<String>,
    guid: String,
}

impl Ord for FeedItem {
    fn cmp(&self, other: &Self) -> Ordering {
        (
            self.issue_kind,
            &self.package_name,
            self.dependency_kind,
            &self.dependency_name,
            &self.advisory_id,
            &self.guid,
        )
            .cmp(&(
                other.issue_kind,
                &other.package_name,
                other.dependency_kind,
                &other.dependency_name,
                &other.advisory_id,
                &other.guid,
            ))
    }
}

impl PartialOrd for FeedItem {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

pub(crate) fn response(
    request: &HttpRequest,
    analysis: &AnalyzeDependenciesOutcome,
    subject: &FeedSubject,
) -> HttpResponse {
    let body = render(analysis, subject);
    let etag = EntityTag::new_strong(representation_hash(body.as_bytes()));

    if request
        .headers()
        .get(IF_NONE_MATCH)
        .and_then(|value| value.to_str().ok())
        .is_some_and(|value| etag_matches(value, &etag))
    {
        return HttpResponse::build(StatusCode::NOT_MODIFIED)
            .insert_header(ETag(etag))
            .insert_header(("Cache-Control", FEED_CACHE_CONTROL))
            .finish();
    }

    HttpResponse::Ok()
        .insert_header(("Content-Type", "application/rss+xml; charset=utf-8"))
        .insert_header(("Cache-Control", FEED_CACHE_CONTROL))
        .insert_header(ETag(etag))
        .body(body)
}

pub(crate) fn render(analysis: &AnalyzeDependenciesOutcome, subject: &FeedSubject) -> String {
    let status_url = subject.status_url();
    let feed_url = subject.feed_url();
    let items = feed_items(analysis, subject)
        .into_iter()
        .map(|item| rss_item(item, &status_url))
        .collect::<Vec<_>>();

    channel(subject, &status_url, &feed_url, items).to_string()
}

fn channel(subject: &FeedSubject, status_url: &Url, feed_url: &Url, items: Vec<Item>) -> Channel {
    let atom_link = Link {
        href: feed_url.to_string(),
        rel: "self".to_owned(),
        mime_type: Some("application/rss+xml".to_owned()),
        ..Link::default()
    };
    let atom = AtomExtensionBuilder::default().link(atom_link).build();
    let mut channel = ChannelBuilder::default()
        .title(subject.title())
        .link(status_url.to_string())
        .description("Current outdated and insecure dependency status reported by deps.rs.")
        .generator(Some("deps.rs".to_owned()))
        .ttl(Some(FEED_TTL_MINUTES.to_owned()))
        .items(items)
        .build();
    channel.set_atom_ext(atom);
    channel
}

fn rss_item(item: FeedItem, status_url: &Url) -> Item {
    let mut item_url = status_url.clone();
    let fragment = match &item.advisory_id {
        Some(advisory_id) => advisory_anchor(advisory_id),
        None => dependency_anchor(
            &item.package_name,
            item.dependency_kind.as_str(),
            &item.dependency_name,
        ),
    };
    item_url.set_fragment(Some(&fragment));

    let title = match &item.advisory_id {
        Some(advisory_id) => format!(
            "{}: {} is {} ({advisory_id})",
            item.package_name,
            item.dependency_name,
            item.issue_kind.label()
        ),
        None => format!(
            "{}: {} is {}",
            item.package_name,
            item.dependency_name,
            item.issue_kind.label()
        ),
    };

    ItemBuilder::default()
        .title(Some(title))
        .link(Some(item_url.to_string()))
        .description(Some(item_description(&item)))
        .guid(Some(
            GuidBuilder::default()
                .value(item.guid)
                .permalink(false)
                .build(),
        ))
        .category(category(item.issue_kind.as_str()))
        .category(category(item.dependency_kind.as_str()))
        .build()
}

fn item_description(item: &FeedItem) -> String {
    let latest_that_matches = item.latest_that_matches.as_deref().unwrap_or("none");
    let latest = item.latest.as_deref().unwrap_or("none");
    let crates_url = format!("https://crates.io/crates/{}", item.dependency_name);
    let advisory_url = item
        .advisory_id
        .as_ref()
        .map(|id| format!("https://rustsec.org/advisories/{id}.html"));

    maud::html! {
        p {
            "Dependency kind: " code { (item.dependency_kind.as_str()) } ". "
            "Required: " code { (&item.required) } ". "
            "Latest matching: " code { (latest_that_matches) } ". "
            "Latest available: " code { (latest) } "."
        }
        p {
            a href=(crates_url) { "View crate" }
            @if let (Some(advisory_id), Some(advisory_url)) = (&item.advisory_id, advisory_url) {
                " · " a href=(advisory_url) { "View " (advisory_id) }
            }
        }
    }
    .into_string()
}

fn category(name: &str) -> rss::Category {
    CategoryBuilder::default().name(name).build()
}

fn feed_items(analysis: &AnalyzeDependenciesOutcome, subject: &FeedSubject) -> Vec<FeedItem> {
    let mut items = Vec::new();
    for (package_name, dependencies) in &analysis.crates {
        collect_dependencies(
            &mut items,
            subject,
            package_name,
            DependencyKind::Main,
            &dependencies.main,
        );
        collect_dependencies(
            &mut items,
            subject,
            package_name,
            DependencyKind::Dev,
            &dependencies.dev,
        );
        collect_dependencies(
            &mut items,
            subject,
            package_name,
            DependencyKind::Build,
            &dependencies.build,
        );
    }
    items.sort();
    items.dedup();
    items
}

fn collect_dependencies(
    items: &mut Vec<FeedItem>,
    subject: &FeedSubject,
    package_name: &CrateName,
    dependency_kind: DependencyKind,
    dependencies: &indexmap::IndexMap<CrateName, AnalyzedDependency>,
) {
    for (dependency_name, dependency) in dependencies {
        if dependency.is_insecure() {
            for advisory in &dependency.vulnerabilities {
                let issue_kind = match dependency.latest_that_matches.as_ref() {
                    Some(version) if !advisory.versions.is_vulnerable(version) => {
                        IssueKind::MaybeInsecure
                    }
                    Some(_) | None => IssueKind::Insecure,
                };
                items.push(build_item(
                    subject,
                    package_name,
                    dependency_kind,
                    dependency_name,
                    dependency,
                    issue_kind,
                    Some(advisory.id().as_str()),
                ));
            }
        }

        if dependency.is_outdated() {
            items.push(build_item(
                subject,
                package_name,
                dependency_kind,
                dependency_name,
                dependency,
                IssueKind::Outdated,
                None,
            ));
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn build_item(
    subject: &FeedSubject,
    package_name: &CrateName,
    dependency_kind: DependencyKind,
    dependency_name: &CrateName,
    dependency: &AnalyzedDependency,
    issue_kind: IssueKind,
    advisory_id: Option<&str>,
) -> FeedItem {
    let package_name = package_name.as_ref().to_owned();
    let dependency_name = dependency_name.as_ref().to_owned();
    let required = dependency.required.to_string();
    let mut identity_fields = subject.identity_fields();
    identity_fields.extend([
        package_name.clone(),
        dependency_kind.as_str().to_owned(),
        dependency_name.clone(),
        issue_kind.as_str().to_owned(),
        required.clone(),
        advisory_id.unwrap_or_default().to_owned(),
    ]);

    FeedItem {
        issue_kind,
        package_name,
        dependency_kind,
        dependency_name,
        advisory_id: advisory_id.map(ToOwned::to_owned),
        required,
        latest_that_matches: dependency
            .latest_that_matches
            .as_ref()
            .map(ToString::to_string),
        latest: dependency.latest.as_ref().map(ToString::to_string),
        guid: format!(
            "{GUID_PREFIX}{}",
            semantic_hash(
                GUID_HASH_CONTEXT,
                identity_fields.iter().map(String::as_str)
            )
        ),
    }
}

fn semantic_hash<'a>(context: &'static str, fields: impl IntoIterator<Item = &'a str>) -> String {
    let mut hasher = blake3::Hasher::new_derive_key(context);
    for field in fields {
        hasher.update(&(field.len() as u64).to_be_bytes());
        hasher.update(field.as_bytes());
    }
    hasher.finalize().to_hex().to_string()
}

fn representation_hash(bytes: &[u8]) -> String {
    let mut hasher = blake3::Hasher::new_derive_key(ETAG_HASH_CONTEXT);
    hasher.update(bytes);
    hasher.finalize().to_hex().to_string()
}

fn etag_matches(if_none_match: &str, etag: &EntityTag) -> bool {
    if_none_match.split(',').map(str::trim).any(|candidate| {
        candidate == "*"
            || candidate
                .parse::<EntityTag>()
                .is_ok_and(|candidate| candidate.weak_eq(etag))
    })
}

fn normalized_repo_path(path: Option<&str>) -> Option<String> {
    path.map(str::trim)
        .map(|path| path.trim_matches('/'))
        .filter(|path| !path.is_empty())
        .map(|path| {
            relative_path::RelativePath::new(path)
                .normalize()
                .to_string()
        })
        .filter(|path| !path.is_empty() && path != ".")
}

#[cfg(test)]
mod tests {
    use indexmap::IndexMap;
    use rss::Channel;
    use rustsec::Advisory;
    use semver::{Version, VersionReq};

    use super::*;
    use crate::models::crates::AnalyzedDependencies;

    fn dependency(required: &str, matching: &str, latest: &str) -> AnalyzedDependency {
        AnalyzedDependency {
            required: VersionReq::parse(required).unwrap(),
            latest_that_matches: Some(Version::parse(matching).unwrap()),
            latest: Some(Version::parse(latest).unwrap()),
            vulnerabilities: Vec::new(),
        }
    }

    fn advisory(id: &str, patched: &str) -> Advisory {
        format!(
            r#"```toml
[advisory]
id = "{id}"
package = "demo-dependency"
date = "2026-01-02"
url = "https://rustsec.org/advisories/{id}.html"

[versions]
patched = ["{patched}"]
```

# Example

CDATA terminator: ]]> and XML characters: < & >.
"#
        )
        .parse()
        .unwrap()
    }

    fn outcome(dependency: AnalyzedDependency) -> AnalyzeDependenciesOutcome {
        let mut main = IndexMap::new();
        main.insert("demo-dependency".parse().unwrap(), dependency);
        AnalyzeDependenciesOutcome {
            crates: vec![(
                "demo-package".parse().unwrap(),
                AnalyzedDependencies {
                    main,
                    dev: IndexMap::new(),
                    build: IndexMap::new(),
                },
            )],
            duration: std::time::Duration::ZERO,
        }
    }

    fn empty_outcome() -> AnalyzeDependenciesOutcome {
        AnalyzeDependenciesOutcome {
            crates: vec![(
                "demo-package".parse().unwrap(),
                AnalyzedDependencies {
                    main: IndexMap::new(),
                    dev: IndexMap::new(),
                    build: IndexMap::new(),
                },
            )],
            duration: std::time::Duration::ZERO,
        }
    }

    fn subject() -> FeedSubject {
        FeedSubject::crate_latest(CratePath::from_parts("demo", "1.0.0").unwrap())
    }

    #[test]
    fn generated_feed_round_trips_with_atom_self_link_and_opaque_guid() {
        let mut dep = dependency(">=1, <2", "1.5.0", "2.0.0");
        dep.vulnerabilities
            .push(advisory("RUSTSEC-2026-0001", ">=1.6.0"));
        let xml = render(&outcome(dep), &subject());
        let channel = Channel::read_from(xml.as_bytes()).unwrap();

        let atom = channel.atom_ext().unwrap();
        assert_eq!(atom.links().len(), 1);
        assert_eq!(atom.links()[0].rel, "self");
        assert_eq!(
            atom.links()[0].mime_type.as_deref(),
            Some("application/rss+xml")
        );
        assert_eq!(
            atom.links()[0].href,
            "http://localhost:8080/crate/demo/latest/feed.xml"
        );
        assert_eq!(channel.items().len(), 2);
        assert!(channel.items().iter().all(|item| {
            item.guid()
                .is_some_and(|guid| !guid.is_permalink() && guid.value().starts_with(GUID_PREFIX))
        }));
        assert!(
            channel
                .items()
                .iter()
                .all(|item| item.description().unwrap().contains("Dependency kind:"))
        );
    }

    #[test]
    fn healthy_feed_has_no_items() {
        let xml = render(&empty_outcome(), &subject());
        let channel = Channel::read_from(xml.as_bytes()).unwrap();

        assert!(channel.items().is_empty());
        assert!(channel.pub_date().is_none());
        assert!(channel.last_build_date().is_none());
    }

    #[test]
    fn latest_release_changes_description_and_etag_but_not_outdated_guid() {
        let earlier = outcome(dependency("^1", "1.9.0", "2.0.0"));
        let later = outcome(dependency("^1", "1.9.0", "3.0.0"));
        let earlier_items = feed_items(&earlier, &subject());
        let later_items = feed_items(&later, &subject());

        assert_eq!(earlier_items[0].guid, later_items[0].guid);
        assert_ne!(render(&earlier, &subject()), render(&later, &subject()));
    }

    #[test]
    fn latest_and_pinned_feeds_have_distinct_guids() {
        let analysis = outcome(dependency("^1", "1.9.0", "2.0.0"));
        let latest = FeedSubject::crate_latest(CratePath::from_parts("demo", "1.0.0").unwrap());
        let pinned = FeedSubject::crate_pinned(CratePath::from_parts("demo", "1.0.0").unwrap());

        assert_ne!(
            feed_items(&analysis, &latest)[0].guid,
            feed_items(&analysis, &pinned)[0].guid
        );
    }

    #[test]
    fn pinned_versions_and_repo_paths_have_distinct_guids() {
        let analysis = outcome(dependency("^1", "1.9.0", "2.0.0"));
        let pinned_v1 = FeedSubject::crate_pinned(CratePath::from_parts("demo", "1.0.0").unwrap());
        let pinned_v2 = FeedSubject::crate_pinned(CratePath::from_parts("demo", "1.1.0").unwrap());
        assert_ne!(
            feed_items(&analysis, &pinned_v1)[0].guid,
            feed_items(&analysis, &pinned_v2)[0].guid
        );

        let repo = RepoPath::from_parts("github", "deps-rs", "deps.rs").unwrap();
        let root = FeedSubject::repo(repo.clone(), None);
        let member = FeedSubject::repo(repo, Some("libs/badge"));
        assert_ne!(
            feed_items(&analysis, &root)[0].guid,
            feed_items(&analysis, &member)[0].guid
        );
    }

    #[test]
    fn requirement_and_either_severity_transition_produce_new_guids() {
        let earlier = outcome(dependency("^1", "1.5.0", "2.0.0"));
        let changed_requirement = outcome(dependency(">=1, <2", "1.5.0", "2.0.0"));
        assert_ne!(
            feed_items(&earlier, &subject())[0].guid,
            feed_items(&changed_requirement, &subject())[0].guid
        );

        let mut maybe = dependency("^1", "1.6.0", "2.0.0");
        maybe
            .vulnerabilities
            .push(advisory("RUSTSEC-2026-0001", ">=1.6.0"));
        let mut insecure = dependency("^1", "1.5.0", "2.0.0");
        insecure
            .vulnerabilities
            .push(advisory("RUSTSEC-2026-0001", ">=1.6.0"));
        let maybe_guid = feed_items(&outcome(maybe), &subject())[0].guid.clone();
        let insecure_guid = feed_items(&outcome(insecure), &subject())[0].guid.clone();
        assert_ne!(maybe_guid, insecure_guid);
    }

    #[test]
    fn repo_feed_urls_canonicalize_paths_and_escape_atom_self_links() {
        let repo = RepoPath::from_parts("github", "deps-rs", "deps.rs").unwrap();
        let subject = FeedSubject::repo(repo, Some("/libs/../libs/badge & tools/"));
        let xml = render(&empty_outcome(), &subject);
        let channel = Channel::read_from(xml.as_bytes()).unwrap();

        assert_eq!(
            channel.link(),
            "http://localhost:8080/repo/github/deps-rs/deps.rs?path=libs%2Fbadge+%26+tools"
        );
        assert_eq!(
            channel.atom_ext().unwrap().links()[0].href,
            "http://localhost:8080/repo/github/deps-rs/deps.rs/feed.xml?path=libs%2Fbadge+%26+tools"
        );
        assert!(xml.contains("&amp;"));
    }

    #[test]
    fn gitea_sites_keep_all_url_path_segments() {
        let repo = RepoPath::from_parts("gitea/example.com/git", "deps-rs", "deps.rs").unwrap();
        let subject = FeedSubject::repo(repo, None);

        assert_eq!(
            subject.feed_url().as_str(),
            "http://localhost:8080/repo/gitea/example.com/git/deps-rs/deps.rs/feed.xml"
        );
    }

    #[test]
    fn advisories_are_independent_and_security_items_sort_first() {
        let mut dep = dependency("^1", "1.5.0", "2.0.0");
        dep.vulnerabilities
            .push(advisory("RUSTSEC-2026-0002", ">=1.6.0"));
        dep.vulnerabilities
            .push(advisory("RUSTSEC-2026-0001", ">=1.6.0"));
        let items = feed_items(&outcome(dep), &subject());

        assert_eq!(items.len(), 3);
        assert_eq!(items[0].advisory_id.as_deref(), Some("RUSTSEC-2026-0001"));
        assert_eq!(items[1].advisory_id.as_deref(), Some("RUSTSEC-2026-0002"));
        assert_eq!(items[2].issue_kind, IssueKind::Outdated);
    }

    #[actix_web::test]
    async fn response_supports_weak_conditional_requests_and_cache_headers() {
        let analysis = outcome(dependency("^1", "1.9.0", "2.0.0"));
        let request = actix_web::test::TestRequest::default().to_http_request();
        let initial = response(&request, &analysis, &subject());
        assert_eq!(initial.status(), StatusCode::OK);
        assert_eq!(
            initial.headers().get("cache-control").unwrap(),
            FEED_CACHE_CONTROL
        );
        let etag = initial.headers().get("etag").unwrap().clone();

        let request = actix_web::test::TestRequest::default()
            .insert_header((IF_NONE_MATCH, format!("W/{}", etag.to_str().unwrap())))
            .to_http_request();
        let unchanged = response(&request, &analysis, &subject());
        assert_eq!(unchanged.status(), StatusCode::NOT_MODIFIED);
        assert_eq!(unchanged.headers().get("etag"), Some(&etag));
        assert_eq!(
            unchanged.headers().get("cache-control").unwrap(),
            FEED_CACHE_CONTROL
        );
        assert!(
            actix_web::body::to_bytes(unchanged.into_body())
                .await
                .unwrap()
                .is_empty()
        );

        let request = actix_web::test::TestRequest::default()
            .insert_header((IF_NONE_MATCH, "\"unrelated\", *"))
            .to_http_request();
        assert_eq!(
            response(&request, &analysis, &subject()).status(),
            StatusCode::NOT_MODIFIED
        );
    }

    #[test]
    fn special_characters_remain_parseable() {
        let mut dep = dependency(">=1, <2", "1.5.0", "2.0.0");
        dep.vulnerabilities
            .push(advisory("RUSTSEC-2026-0001", ">=1.6.0"));
        let xml = render(&outcome(dep), &subject());

        Channel::read_from(xml.as_bytes()).unwrap();
        assert!(xml.contains("&gt;=1, &lt;2"));
    }

    #[test]
    fn rss_dependency_preserves_cdata_terminators() {
        let expected = "before ]]> after";
        let item = ItemBuilder::default()
            .description(Some(expected.to_owned()))
            .build();
        let channel = ChannelBuilder::default()
            .title("test")
            .link("https://example.com")
            .description("test")
            .item(item)
            .build();
        let xml = channel.to_string();
        let parsed = Channel::read_from(xml.as_bytes()).unwrap();

        assert_eq!(parsed.items()[0].description(), Some(expected));
    }
}
