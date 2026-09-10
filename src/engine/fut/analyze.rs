use futures_util::StreamExt as _;

use crate::{
    Engine,
    engine::{AnalyzeDependenciesError, machines::analyzer::DependencyAnalyzer},
    interactors::crates::QueryCrateError,
    models::crates::{AnalyzedDependencies, CrateDep, CrateDeps, CrateName},
};

fn filter_external((name, dep): (CrateName, CrateDep)) -> Option<CrateName> {
    if dep.is_external() { Some(name) } else { None }
}

pub async fn analyze_dependencies(
    engine: Engine,
    deps: CrateDeps,
) -> Result<AnalyzedDependencies, AnalyzeDependenciesError> {
    let advisory_db = engine
        .fetch_advisory_db()
        .await
        .map_err(AnalyzeDependenciesError::Upstream)?;
    let mut analyzer = DependencyAnalyzer::new(&deps, Some(advisory_db));

    let main_deps = deps.main.into_iter().filter_map(filter_external);
    let dev_deps = deps.dev.into_iter().filter_map(filter_external);
    let build_deps = deps.build.into_iter().filter_map(filter_external);

    let deps_iter = main_deps.chain(dev_deps).chain(build_deps);
    let mut releases = engine.fetch_releases(deps_iter);

    while let Some(release) = releases.next().await {
        let release = release.map_err(|err| match err {
            QueryCrateError::NotFound(name) => AnalyzeDependenciesError::DependencyNotFound(name),
            QueryCrateError::Query(err) => AnalyzeDependenciesError::Upstream(err),
        })?;
        analyzer.process(release)
    }

    Ok(analyzer.finalize())
}
