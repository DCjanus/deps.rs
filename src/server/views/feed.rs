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

/// RSS 条目中的依赖分组，用于区分普通依赖、开发依赖和构建依赖。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum DependencyKind {
    /// `[dependencies]` 中声明的普通依赖。
    Main,
    /// `[dev-dependencies]` 中声明的开发依赖。
    Dev,
    /// `[build-dependencies]` 中声明的构建依赖。
    Build,
}

impl DependencyKind {
    /// 返回写入 RSS category 和 GUID 的稳定字符串标识。
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            DependencyKind::Main => "main",
            DependencyKind::Dev => "dev",
            DependencyKind::Build => "build",
        }
    }
}

/// RSS 条目描述的问题类型。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum FeedIssueKind {
    /// 依赖版本已经落后于可用的新版本。
    Outdated,
    /// 依赖版本命中 RustSec 安全公告。
    Insecure,
}

impl FeedIssueKind {
    /// 返回写入 RSS category、标题和 GUID 的稳定字符串标识。
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            FeedIssueKind::Outdated => "outdated",
            FeedIssueKind::Insecure => "insecure",
        }
    }
}

/// 内部使用的 RSS 条目模型，先收集并排序，再转换成 `rss::Item`。
#[derive(Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct FeedItem {
    /// 声明该依赖的 package 名称，workspace feed 用它区分不同成员。
    pub(crate) package_name: String,
    /// 该依赖来自普通、开发还是构建依赖分组。
    pub(crate) dependency_kind: DependencyKind,
    /// 被报告为过期或不安全的依赖名称。
    pub(crate) dependency_name: String,
    /// 当前条目报告的问题类型。
    pub(crate) issue_kind: FeedIssueKind,
    /// 展示给 RSS 订阅器的条目标题。
    pub(crate) title: String,
    /// 展示给 RSS 订阅器的版本状态摘要。
    pub(crate) description: String,
    /// RSS GUID，用于让客户端判断是否是同一条依赖声明事件。
    pub(crate) guid: String,
}

/// 根据依赖分析结果渲染 RSS HTTP 响应，并处理 `If-None-Match` 条件请求。
pub(crate) fn response(
    request: &HttpRequest,
    analysis_outcome: &AnalyzeDependenciesOutcome,
    subject_path: &SubjectPath,
    repo_path: Option<&str>,
    status_url: &str,
) -> HttpResponse {
    let body = render(analysis_outcome, subject_path, repo_path, status_url);
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

/// 将依赖分析结果渲染成完整的 RSS 2.0 XML 字符串。
pub(crate) fn render(
    analysis_outcome: &AnalyzeDependenciesOutcome,
    subject_path: &SubjectPath,
    repo_path: Option<&str>,
    status_url: &str,
) -> String {
    let items = feed_items(analysis_outcome, subject_path, repo_path)
        .into_iter()
        .map(|item| rss_item(item, status_url))
        .collect::<Vec<_>>();

    channel(subject_path, repo_path, status_url, items).to_string()
}

/// 构造 RSS channel 元数据和条目列表。
fn channel(
    subject_path: &SubjectPath,
    repo_path: Option<&str>,
    status_url: &str,
    items: Vec<Item>,
) -> rss::Channel {
    ChannelBuilder::default()
        .title(channel_title(subject_path, repo_path))
        .link(status_url)
        .description("Outdated and insecure dependency status reported by deps.rs.")
        .ttl(Some(FEED_TTL_MINUTES.to_string()))
        .items(items)
        .build()
}

/// 判断 `If-None-Match` 里的任一 ETag 是否与当前 feed ETag 弱匹配。
fn etag_matches(if_none_match: &str, etag: &EntityTag) -> bool {
    if_none_match.split(',').map(str::trim).any(|candidate| {
        candidate == "*"
            || candidate
                .parse::<EntityTag>()
                .is_ok_and(|candidate| candidate.weak_eq(etag))
    })
}

/// 生成 RSS channel 标题；repo feed 会把子路径纳入标题，方便区分多个子项目。
pub(crate) fn channel_title(subject_path: &SubjectPath, path: Option<&str>) -> String {
    match subject_path {
        SubjectPath::Repo(repo_path) => {
            let mut name = format!(
                "{}/{}/{}",
                repo_path.site,
                repo_path.qual.as_ref(),
                repo_path.name.as_ref()
            );

            if let Some(path) = normalized_repo_path(path) {
                name.push('/');
                name.push_str(&path);
            }

            format!("deps.rs: {name} dependency status")
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

/// 把内部 `FeedItem` 转换为 rss crate 使用的 `Item`。
fn rss_item(item: FeedItem, status_url: &str) -> Item {
    ItemBuilder::default()
        .title(item.title)
        .link(Some(status_url.to_string()))
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

/// 构造 RSS category。
fn category(name: &str) -> rss::Category {
    CategoryBuilder::default().name(name).build()
}

/// 从完整依赖分析结果中收集所有需要出现在 feed 中的问题条目。
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

/// 收集某个 package 的普通、开发和构建依赖问题条目。
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

/// 根据单个依赖的分析结果，按需生成过期和不安全条目。
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

/// 构造单条 feed item，并为该依赖声明生成稳定 GUID。
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
            issue_label(issue_kind, dep)
        ),
        description: item_description(dep),
        package_name,
        dependency_kind,
        dependency_name,
        issue_kind,
        guid,
    }
}

/// 根据问题类型和漏洞影响范围，生成条目标题里的问题标签。
fn issue_label(issue_kind: FeedIssueKind, dep: &AnalyzedDependency) -> &'static str {
    match issue_kind {
        FeedIssueKind::Outdated => issue_kind.as_str(),
        FeedIssueKind::Insecure if dep.is_always_insecure() => issue_kind.as_str(),
        FeedIssueKind::Insecure => "maybe insecure",
    }
}

/// 生成单条 RSS item 的版本状态描述。
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

/// 为同一 subject、package、依赖分组、问题类型和版本要求生成稳定 GUID。
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

/// 归一化 repo 子路径，避免前后斜杠影响标题和 GUID。
fn normalized_repo_path(path: Option<&str>) -> Option<String> {
    path.map(str::trim)
        .map(|path| path.trim_matches('/'))
        .filter(|path| !path.is_empty())
        .map(ToOwned::to_owned)
}

/// 生成 feed subject 标识；repo feed 会纳入子路径以区分不同子项目。
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
    use rustsec::Advisory;
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

    fn advisory() -> Advisory {
        r#"```toml
[advisory]
id = "RUSTSEC-2001-2101"
package = "tokio"
date = "2001-02-03"
url = "https://rustsec.org/advisories/RUSTSEC-2001-2101.html"

[versions]
patched = [">= 1.2.3"]
```

# Example vulnerability

Example advisory.
"#
        .parse()
        .unwrap()
    }

    fn vulnerable_dep(latest: &str) -> AnalyzedDependency {
        let mut dep = dep("<2.0", Some(latest), Some(latest));
        dep.vulnerabilities.push(advisory());
        dep
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
    fn insecure_title_distinguishes_maybe_insecure_dependencies() {
        let items = feed_items(&outcome(vulnerable_dep("1.2.3")), &subject(), None);

        assert_eq!(items[0].title, "demo: tokio is maybe insecure");
    }

    #[test]
    fn insecure_title_keeps_always_insecure_dependencies_strong() {
        let items = feed_items(&outcome(vulnerable_dep("1.2.2")), &subject(), None);

        assert_eq!(items[0].title, "demo: tokio is insecure");
    }

    #[test]
    fn repo_path_is_in_channel_title() {
        let xml = render(
            &outcome(dep("~1.32", Some("1.32.9"), Some("1.33.0"))),
            &repo_subject(),
            Some("/service-a/"),
            "https://deps.rs/repo/github/deps-rs/deps.rs?path=service-a",
        );

        assert!(xml.contains(
            "<title>deps.rs: github/deps-rs/deps.rs/service-a dependency status</title>"
        ));
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
}
